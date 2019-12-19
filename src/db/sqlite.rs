use chrono;
use chrono::TimeZone;
use futures::{compat::Future01CompatExt, Future, FutureExt};
use futures_cpupool::CpuPool;
use linear_map::LinearMap;
use r2d2;
use r2d2_sqlite::SqliteConnectionManager;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use rusqlite;
use snafu::ResultExt;

use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use crate::db::{ConnectionPoolError, Database, DatabaseError, SqliteError, Transaction, User};

/// An implementation of [Database] using sqlite.Database
///
/// Safe to clone as the thread and connection pools will be shared.
#[derive(Clone)]
pub struct SqliteDatabase {
    /// Thread pool used to do database operations.
    cpu_pool: CpuPool,
    /// SQLite connection pool.
    db_pool: Arc<r2d2::Pool<SqliteConnectionManager>>,
}

impl SqliteDatabase {
    /// Create new instance with given path. If file does not exist a new
    /// database is created.
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
    fn get_user_by_github_id(
        &self,
        github_user_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>, DatabaseError>>>> {
        let db_pool = self.db_pool.clone();

        self.cpu_pool
            .spawn_fn(move || -> Result<_, DatabaseError> {
                let conn = db_pool.get().context(ConnectionPoolError)?;

                let row = conn
                    .query_row(
                        "SELECT user_id FROM github_users WHERE github_id = $1",
                        &[&github_user_id],
                        |row| row.get(0),
                    )
                    .map(Some)
                    .or_else(|err| {
                        if let rusqlite::Error::QueryReturnedNoRows = err {
                            Ok(None)
                        } else {
                            Err(err)
                        }
                    })
                    .context(SqliteError)?;

                Ok(row)
            })
            .compat()
            .boxed()
    }

    fn add_user_by_github_id(
        &self,
        github_user_id: String,
        display_name: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, DatabaseError>>>> {
        let db_pool = self.db_pool.clone();

        self.cpu_pool
            .spawn_fn(move || -> Result<_, DatabaseError> {
                let conn = db_pool.get().context(ConnectionPoolError)?;

                conn.execute(
                    "INSERT INTO github_users (user_id, github_id)
                VALUES ($1, $1)",
                    &[&github_user_id],
                )
                .context(SqliteError)?;

                conn.execute(
                    "INSERT INTO users (user_id, display_name)
                VALUES ($1, $2)",
                    &[&github_user_id, &display_name],
                )
                .context(SqliteError)?;

                Ok(github_user_id)
            })
            .compat()
            .boxed()
    }

    fn create_token_for_user(
        &self,
        user_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, DatabaseError>>>> {
        let db_pool = self.db_pool.clone();

        self.cpu_pool
            .spawn_fn(move || -> Result<_, DatabaseError> {
                let conn = db_pool.get().context(ConnectionPoolError)?;

                let token: String = thread_rng().sample_iter(&Alphanumeric).take(32).collect();

                conn.execute(
                    "INSERT INTO tokens (user_id, token) VALUES ($1, $2)",
                    &[&user_id, &token],
                )
                .context(SqliteError)?;

                Ok(token)
            })
            .compat()
            .boxed()
    }

    fn delete_token(
        &self,
        token: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), DatabaseError>>>> {
        let db_pool = self.db_pool.clone();

        self.cpu_pool
            .spawn_fn(move || -> Result<_, DatabaseError> {
                let conn = db_pool.get().context(ConnectionPoolError)?;

                conn.execute("DELETE FROM tokens WHERE token = $1", &[&token])
                    .context(SqliteError)?;

                Ok(())
            })
            .compat()
            .boxed()
    }

    fn get_user_from_token(
        &self,
        token: String,
    ) -> Pin<Box<dyn Future<Output = Result<Option<User>, DatabaseError>>>> {
        let db_pool = self.db_pool.clone();

        self.cpu_pool
            .spawn_fn(move || -> Result<_, DatabaseError> {
                let conn = db_pool.get().context(ConnectionPoolError)?;

                let row = conn
                    .query_row(
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
                        |row| {
                            Ok(User {
                                user_id: row.get(0)?,
                                display_name: row.get(1)?,
                                balance: row.get(2)?,
                            })
                        },
                    )
                    .map(Some)
                    .or_else(|err| {
                        if let rusqlite::Error::QueryReturnedNoRows = err {
                            Ok(None)
                        } else {
                            Err(err)
                        }
                    })
                    .context(SqliteError)?;

                Ok(row)
            })
            .compat()
            .boxed()
    }

    fn get_balance_for_user(
        &self,
        user: String,
    ) -> Pin<Box<dyn Future<Output = Result<i64, DatabaseError>>>> {
        let db_pool = self.db_pool.clone();

        self.cpu_pool
            .spawn_fn(move || -> Result<_, DatabaseError> {
                let conn = db_pool.get().context(ConnectionPoolError)?;

                let row = conn
                    .query_row(
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
                        |row| row.get(0),
                    )
                    .context(SqliteError)?;

                Ok(row)
            })
            .compat()
            .boxed()
    }

    fn get_all_users(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<LinearMap<String, User>, DatabaseError>>>> {
        let db_pool = self.db_pool.clone();

        self.cpu_pool
            .spawn_fn(move || -> Result<_, DatabaseError> {
                let conn = db_pool.get().context(ConnectionPoolError)?;

                let mut stmt = conn
                    .prepare(
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
                "#,
                    )
                    .context(SqliteError)?;

                let rows: Result<LinearMap<String, User>, _> = stmt
                    .query_map(params![], |row| {
                        Ok((
                            row.get(0)?,
                            User {
                                user_id: row.get(0)?,
                                display_name: row.get(1)?,
                                balance: row.get(2)?,
                            },
                        ))
                    })
                    .context(SqliteError)?
                    .collect();

                Ok(rows.context(SqliteError)?)
            })
            .compat()
            .boxed()
    }

    fn shaft_user(
        &self,
        transaction: Transaction,
    ) -> Pin<Box<dyn Future<Output = Result<(), DatabaseError>>>> {
        let db_pool = self.db_pool.clone();

        self.cpu_pool
            .spawn_fn(move || -> Result<_, DatabaseError> {
                let conn = db_pool.get().context(ConnectionPoolError)?;

                match conn.query_row(
                    "SELECT user_id FROM users WHERE user_id = $1",
                    &[&transaction.shaftee],
                    |_row| Ok(()),
                ) {
                    Ok(_) => (),
                    Err(rusqlite::Error::QueryReturnedNoRows) => {
                        return Err(DatabaseError::UnknownUser {
                            user_id: transaction.shaftee,
                        })
                    }
                    Err(err) => Err(err).context(SqliteError)?,
                }

                let mut stmt = conn
                    .prepare(
                        "INSERT INTO transactions (shafter, shaftee, amount, time_sec, reason)\
                     VALUES ($1, $2, $3, $4, $5)",
                    )
                    .context(SqliteError)?;

                stmt.execute(params![
                    &transaction.shafter,
                    &transaction.shaftee,
                    &transaction.amount,
                    &transaction.datetime.timestamp(),
                    &transaction.reason,
                ])
                .context(SqliteError)?;

                Ok(())
            })
            .compat()
            .boxed()
    }

    fn get_last_transactions(
        &self,
        limit: u32,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Transaction>, DatabaseError>>>> {
        let db_pool = self.db_pool.clone();

        self.cpu_pool
            .spawn_fn(move || -> Result<_, DatabaseError> {
                let conn = db_pool.get().context(ConnectionPoolError)?;

                let mut stmt = conn
                    .prepare(
                        r#"SELECT shafter, shaftee, amount, time_sec, reason
                FROM transactions
                ORDER BY id DESC
                LIMIT $1
                "#,
                    )
                    .context(SqliteError)?;

                let rows: Result<Vec<_>, _> = stmt
                    .query_map(&[&limit], |row| {
                        Ok(Transaction {
                            shafter: row.get(0)?,
                            shaftee: row.get(1)?,
                            amount: row.get(2)?,
                            datetime: chrono::Utc.timestamp(row.get(3)?, 0),
                            reason: row.get(4)?,
                        })
                    })
                    .context(SqliteError)?
                    .collect();

                Ok(rows.context(SqliteError)?)
            })
            .compat()
            .boxed()
    }
}
