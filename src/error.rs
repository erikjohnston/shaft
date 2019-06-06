use snafu::Backtrace;
use actix_web::error::ResponseError;

use crate::{db, github};

#[derive(Debug, Snafu)]
#[snafu(visibility = "pub(crate)")]
pub enum ShaftError {
    #[snafu(display("{}", source))]
    DatabaseError {
        source: db::DatabaseError,
        backtrace: Backtrace,
    },

    #[snafu(display("{}", source))]
    GithubError {
        source: github::HttpError,
        backtrace: Backtrace,
    },
}


impl ResponseError for ShaftError {}
