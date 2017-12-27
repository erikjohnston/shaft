use chrono;
use chrono::TimeZone;
use futures_cpupool::CpuPool;
use futures::Future;
use r2d2;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite;
use rand::{Rng, thread_rng};
use serde;
use linear_map::LinearMap;

use std::sync::Arc;
use std::path::Path;


#[derive(Clone, Debug, Serialize)]
pub struct Transaction {
    pub shafter: String,
    pub shaftee: String,
    pub amount: i64,
    #[serde(serialize_with = "serialize_time")]
    pub datetime: chrono::DateTime<chrono::Utc>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub user_id: String,
    pub display_name: String,
    pub balance: i64,
}


pub trait Database: Send + Sync {
    fn get_user_by_github_id(&self, github_user_id: String)
        -> Box<Future<Item=Option<String>, Error=DatabaseError>>;

    fn add_user_by_github_id(&self, github_user_id: String, display_name: String)
        -> Box<Future<Item=String, Error=DatabaseError>>;

    fn create_token_for_user(&self, user_id: String)
        -> Box<Future<Item=String, Error=DatabaseError>>;

    fn delete_token(&self, token: String)
        -> Box<Future<Item=(), Error=DatabaseError>>;

    fn get_user_from_token(&self, token: String)
        -> Box<Future<Item=Option<User>, Error=DatabaseError>>;

    fn get_balance_for_user(&self, user: String)
        -> Box<Future<Item=i64, Error=DatabaseError>>;

    fn get_all_users(&self)
        -> Box<Future<Item=LinearMap<String, User>, Error=DatabaseError>>;

    fn shaft_user(&self, transaction: Transaction)
        -> Box<Future<Item=(), Error=ShaftUserError>>;

    fn get_last_transactions(&self, limit: u32)
        -> Box<Future<Item=Vec<Transaction>, Error=DatabaseError>>;
}


#[derive(Clone)]
pub struct SqliteDatabase {
    cpu_pool: CpuPool,
    db_pool: Arc<r2d2::Pool<SqliteConnectionManager>>,
}


impl SqliteDatabase {
    pub fn with_path<P: AsRef<Path>>(path: P) -> SqliteDatabase {
        let manager = SqliteConnectionManager::file(path);
        let pool = r2d2::Pool::new(manager).unwrap();

        SqliteDatabase {
            cpu_pool: CpuPool::new_num_cpus(),
            db_pool: Arc::new(pool),
        }
    }
}


impl Database for SqliteDatabase {
    fn get_user_by_github_id(&self, github_user_id: String)
        -> Box<Future<Item=Option<String>, Error=DatabaseError>>
    {
        let db_pool = self.db_pool.clone();

        let f = self.cpu_pool.spawn_fn(move || -> Result<_, DatabaseError>  {
            let conn = db_pool.get()?;

            let row = conn.query_row(
                "SELECT user_id FROM github_users WHERE github_id = $1",
                &[&github_user_id],
                |row| -> String {
                    row.get(0)
                }
            )
            .map(|row| Some(row))
            .or_else(|err| {
                if let rusqlite::Error::QueryReturnedNoRows = err {
                    Ok(None)
                } else {
                    Err(err)
                }
            })?;

            Ok(row)
        });

        Box::new(f)
    }

    fn add_user_by_github_id(&self, github_user_id: String, display_name: String)
        -> Box<Future<Item=String, Error=DatabaseError>>
    {
        let db_pool = self.db_pool.clone();

        let f = self.cpu_pool.spawn_fn(move || -> Result<_, DatabaseError>  {
            let conn = db_pool.get()?;

            conn.execute(
                "INSERT INTO github_users (user_id, github_id)
                VALUES ($1, $1)",
                &[&github_user_id],
            )?;

             conn.execute(
                "INSERT INTO users (user_id, display_name)
                VALUES ($1, $2)",
                &[&github_user_id, &display_name],
            )?;

            Ok(github_user_id)
        });

        Box::new(f)
    }

    fn create_token_for_user(&self, user_id: String)
        -> Box<Future<Item=String, Error=DatabaseError>>
    {
        let db_pool = self.db_pool.clone();

        let f = self.cpu_pool.spawn_fn(move || -> Result<_, DatabaseError>  {
            let conn = db_pool.get()?;

            let token: String = thread_rng().gen_ascii_chars().take(32).collect();

            conn.execute(
                "INSERT INTO tokens (user_id, token) VALUES ($1, $2)",
                &[&user_id, &token],
            )?;

            Ok(token)
        });

        Box::new(f)
    }

    fn delete_token(&self, token: String)
        -> Box<Future<Item=(), Error=DatabaseError>>
    {
        let db_pool = self.db_pool.clone();

        let f = self.cpu_pool.spawn_fn(move || -> Result<_, DatabaseError>  {
            let conn = db_pool.get()?;

            conn.execute(
                "DELETE FROM tokens WHERE token = $1",
                &[&token],
            )?;

            Ok(())
        });

        Box::new(f)
    }

    fn get_user_from_token(&self, token: String)
        -> Box<Future<Item=Option<User>, Error=DatabaseError>>
    {
        let db_pool = self.db_pool.clone();

        let f = self.cpu_pool.spawn_fn(move || -> Result<_, DatabaseError>  {
            let conn = db_pool.get()?;

            let row = conn.query_row(
                r#"
                SELECT user_id, display_name, COALESCE(balance, 0)
                FROM tokens
                INNER JOIN users USING (user_id)
                LEFT JOIN (
                    SELECT user_id, SUM(amount) as balance
                    FROM (
                        SELECT shafter AS user_id, SUM(amount) AS amount
                        FROM transactions GROUP BY shafter
                        UNION ALL
                        SELECT shaftee AS user_id, -SUM(amount) AS amount
                        FROM transactions GROUP BY shaftee
                    ) t GROUP BY user_id
                )
                USING (user_id)
                WHERE token = $1
                "#,
                &[&token],
                |row| -> User {
                    User {
                        user_id: row.get(0),
                        display_name: row.get(1),
                        balance: row.get(2),
                    }
                }
            )
            .map(|row| Some(row))
            .or_else(|err| {
                if let rusqlite::Error::QueryReturnedNoRows = err {
                    Ok(None)
                } else {
                    Err(err)
                }
            })?;

            Ok(row)
        });

        Box::new(f)
    }

    fn get_balance_for_user(&self, user: String)
        -> Box<Future<Item=i64, Error=DatabaseError>>
    {
        let db_pool = self.db_pool.clone();

        let f = self.cpu_pool.spawn_fn(move || -> Result<_, DatabaseError>  {
            let conn = db_pool.get()?;

            let row = conn.query_row(
                r#"SELECT (
                    SELECT COALESCE(SUM(amount), 0)
                    FROM transactions
                    WHERE shafter = $1
                ) - (
                    SELECT COALESCE(SUM(amount), 0)
                    FROM transactions
                    WHERE shaftee = $1
                )"#,
                &[&user],
                |row| -> i64 {
                    row.get(0)
                }
            )?;

            Ok(row)
        });

        Box::new(f)
    }

    fn get_all_users(&self)
        -> Box<Future<Item=LinearMap<String, User>, Error=DatabaseError>>
    {
        let db_pool = self.db_pool.clone();

        let f = self.cpu_pool.spawn_fn(move || -> Result<_, DatabaseError>  {
            let conn = db_pool.get()?;

            let mut stmt = conn.prepare(
                r#"
                SELECT user_id, display_name, COALESCE(balance, 0) AS balance
                FROM users
                LEFT JOIN (
                    SELECT user_id, SUM(amount) as balance
                    FROM (
                        SELECT shafter AS user_id, SUM(amount) AS amount
                        FROM transactions GROUP BY shafter
                        UNION ALL
                        SELECT shaftee AS user_id, -SUM(amount) AS amount
                        FROM transactions GROUP BY shaftee
                    ) t GROUP BY user_id
                )
                USING (user_id)
                ORDER BY balance ASC
                "#
            )?;

            let rows: Result<LinearMap<String, User>, _> = stmt.query_map(
                &[],
                |row| -> (String, User) {
                    (row.get(0), User {
                        user_id: row.get(0),
                        display_name: row.get(1),
                        balance: row.get(2),
                    })
                }
            )?.collect();

            Ok(rows?)
        });

        Box::new(f)
    }

    fn shaft_user(&self, transaction: Transaction)
        -> Box<Future<Item=(), Error=ShaftUserError>>
    {
        let db_pool = self.db_pool.clone();

        let f = self.cpu_pool.spawn_fn(move || -> Result<_, ShaftUserError>  {
            let conn = db_pool.get()?;

            match conn.query_row(
                "SELECT user_id FROM users WHERE user_id = $1",
                &[&transaction.shaftee],
                |_row| {}
            ) {
                Ok(_) => (),
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    return Err(ShaftUserError::UnknownUser(transaction.shaftee))
                }
                Err(err) => {
                    return Err(ShaftUserError::Database(err.into()))
                }
            }

            let mut stmt = conn.prepare(
                "INSERT INTO transactions (shafter, shaftee, amount, time_sec, reason)\
                VALUES ($1, $2, $3, $4, $5)"
            )?;

            stmt.execute(&[
                &transaction.shafter,
                &transaction.shaftee,
                &transaction.amount,
                &transaction.datetime.timestamp(),
                &transaction.reason,
            ])?;

            Ok(())
        });

        Box::new(f)
    }

    fn get_last_transactions(&self, limit: u32)
        -> Box<Future<Item=Vec<Transaction>, Error=DatabaseError>>
    {
        let db_pool = self.db_pool.clone();

        let f = self.cpu_pool.spawn_fn(move || -> Result<_, DatabaseError>  {
            let conn = db_pool.get()?;

            let mut stmt = conn.prepare(
                r#"SELECT shafter, shaftee, amount, time_sec, reason
                FROM transactions
                ORDER BY id DESC
                LIMIT $1
                "#
            )?;

            let rows: Result<Vec<_>, _> = stmt.query_map(
                &[&limit],
                |row| {
                    Transaction {
                        shafter: row.get(0),
                        shaftee: row.get(1),
                        amount: row.get(2),
                        datetime: chrono::Utc.timestamp(row.get(3), 0),
                        reason: row.get(4),
                    }
                }
            )?.collect();

            Ok(rows?)
        });

        Box::new(f)
    }
}


quick_error! {
    #[derive(Debug)]
    pub enum DatabaseError {
        ConnectionPool(err: r2d2::Error) {
            from()
            display("DB Pool error: {}", err)
        }
        SqliteError(err: rusqlite::Error) {
            from()
            display("Sqlite Pool error: {}", err)
        }
    }
}


quick_error! {
    #[derive(Debug)]
    pub enum ShaftUserError {
        Database(err: DatabaseError) {
            from()
            from(e: r2d2::Error) -> (DatabaseError::from(e))
            from(e: rusqlite::Error) -> (DatabaseError::from(e))
            display("{}", err)
        }
        UnknownUser(user_id: String) {
            from()
            display("Unknown user: {}", user_id)
        }
    }
}



fn serialize_time<S>(date: &chrono::DateTime<chrono::Utc>, serializer: S)
    -> Result<S::Ok, S::Error>
where S: serde::Serializer
{
    serializer.serialize_i64(date.timestamp())
}
