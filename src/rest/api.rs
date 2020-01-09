//! The JSON API for interacting with shaft

use actix_web::web::{Json, ServiceConfig};
use actix_web::{error::ErrorInternalServerError, web, Error, HttpRequest};
use chrono;
use serde::Serialize;
use snafu::ResultExt;

use crate::db;
use crate::error::{DatabaseError, ShaftError};
use crate::rest::{AppState, AuthenticatedUser, ShaftUserBody};

use slog::Logger;

/// Register servlets with HTTP app
pub fn register_servlets(config: &mut ServiceConfig) {
    config.route("/api/balances", web::get().to(get_api_balances));
    config.route("/api/transactions", web::get().to(get_api_transactions));
    config.route("/api/shaft", web::post().to(shaft_user));
}

/// Get all user's balances as a map from user ID to [User](crate::db::User)
/// object.
async fn get_api_balances(
    (state, _user): (web::Data<AppState>, AuthenticatedUser),
) -> Result<Json<impl Serialize>, Error> {
    state
        .database
        .get_all_users()
        .await
        .map_err(ErrorInternalServerError)
        .map(Json)
}

/// Get most recent transactions
async fn get_api_transactions(
    (state, _user): (web::Data<AppState>, AuthenticatedUser),
) -> Result<Json<Vec<db::Transaction>>, Error> {
    state
        .database
        .get_last_transactions(20)
        .await
        .map_err(ErrorInternalServerError)
        .map(Json)
}

/// Create a new transaction.
///
/// Returns an empty json object.
async fn shaft_user(
    (req, state, user, body): (
        HttpRequest,
        web::Data<AppState>,
        AuthenticatedUser,
        Json<ShaftUserBody>,
    ),
) -> Result<Json<impl Serialize>, ShaftError> {
    let logger = req
        .extensions()
        .get::<Logger>()
        .expect("no logger installed in request")
        .clone();

    let ShaftUserBody {
        other_user,
        amount,
        reason,
    } = body.0;

    state
        .database
        .shaft_user(db::Transaction {
            shafter: user.user_id.clone(),
            shaftee: other_user.clone(),
            amount,
            datetime: chrono::Utc::now(),
            reason,
        })
        .await
        .context(DatabaseError)?;

    info!(
        logger, "Shafted user";
        "other_user" => other_user, "amount" => amount
    );

    Ok(Json(json!({})))
}
