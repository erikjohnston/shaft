//! Handles talking to local data store.

use chrono;
use futures::Future;
use linear_map::LinearMap;
use r2d2;
use rusqlite;
use serde;
use snafu::Backtrace;

mod postgres;
mod sqlite;

pub use self::postgres::PostgresDatabase;
pub use self::sqlite::SqliteDatabase;

/// A single transaction between two users.
#[derive(Clone, Debug, Serialize)]
pub struct Transaction {
    /// The user who is creating the transaction.
    pub shafter: String,
    /// The other party in the transaction.
    pub shaftee: String,
    /// The amount of money in pence. Positive means shafter is owed the amount,
    /// negative means shafter owes the amount.
    pub amount: i64,
    /// Time transaction happened.
    #[serde(serialize_with = "serialize_time")]
    pub datetime: chrono::DateTime<chrono::Utc>,
    /// A human readable description of the transaction.
    pub reason: String,
}

/// A user and their balance
#[derive(Debug, Clone, Serialize)]
pub struct User {
    /// Their internal shaft user ID
    pub user_id: String,
    /// Their display name
    pub display_name: String,
    /// Their current balance
    pub balance: i64,
}

/// A generic datastore for the app
pub trait Database: Send + Sync {
    /// Get local user ID by their Github login ID
    fn get_user_by_github_id(
        &self,
        github_user_id: String,
    ) -> Box<dyn Future<Item = Option<String>, Error = DatabaseError>>;

    /// Add a new user from github
    fn add_user_by_github_id(
        &self,
        github_user_id: String,
        display_name: String,
    ) -> Box<dyn Future<Item = String, Error = DatabaseError>>;

    /// Create a new Shaft access token
    fn create_token_for_user(
        &self,
        user_id: String,
    ) -> Box<dyn Future<Item = String, Error = DatabaseError>>;

    /// Delete a Shaft access token.
    fn delete_token(&self, token: String) -> Box<dyn Future<Item = (), Error = DatabaseError>>;

    /// Get a user by Shaft access token.
    fn get_user_from_token(
        &self,
        token: String,
    ) -> Box<dyn Future<Item = Option<User>, Error = DatabaseError>>;

    /// Get a user's balance in pence
    fn get_balance_for_user(
        &self,
        user: String,
    ) -> Box<dyn Future<Item = i64, Error = DatabaseError>>;

    /// Get a map of all users from local user ID to [User] object
    fn get_all_users(
        &self,
    ) -> Box<dyn Future<Item = LinearMap<String, User>, Error = DatabaseError>>;

    /// Commit a new Shaft [Transaction]
    fn shaft_user(
        &self,
        transaction: Transaction,
    ) -> Box<dyn Future<Item = (), Error = DatabaseError>>;

    /// Get a list of the most recent Shaft transactions
    fn get_last_transactions(
        &self,
        limit: u32,
    ) -> Box<dyn Future<Item = Vec<Transaction>, Error = DatabaseError>>;
}

/// Error using database.
#[derive(Debug, Snafu)]
pub enum DatabaseError {
    /// Error getting a database connection.
    #[snafu(display("DB Pool error: {}", source))]
    ConnectionPoolError {
        source: r2d2::Error,
        backtrace: Backtrace,
    },

    /// SQLite error.
    #[snafu(display("Sqlite error: {}", source))]
    SqliteError {
        source: rusqlite::Error,
        backtrace: Backtrace,
    },

    /// Postgres error.
    #[snafu(display("Postgres error: {}", source))]
    PostgresError {
        source: ::postgres::Error,
        backtrace: Backtrace,
    },

    /// One of the users is unknown.
    #[snafu(display("Unknown user: {}", user_id))]
    UnknownUser { user_id: String },
}

/// Serialize time into timestamp.
fn serialize_time<S>(date: &chrono::DateTime<chrono::Utc>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_i64(date.timestamp())
}
