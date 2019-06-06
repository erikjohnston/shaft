//! The JSON API for interacting with shaft

use actix_web::web::{Json, ServiceConfig};
use actix_web::{error::ErrorInternalServerError, web, Error, HttpRequest};
use chrono;
use futures::Future;
use serde::Serialize;

use crate::db;
use crate::rest::{AppState, AuthenticatedUser, ShaftUserBody};

use slog::Logger;

/// Register servlets with HTTP app
pub fn register_servlets(config: &mut ServiceConfig) {
    config.route("/api/balances", web::get().to_async(get_api_balances));
    config.route(
        "/api/transactions",
        web::get().to_async(get_api_transactions),
    );
    config.route("/api/shaft", web::post().to_async(shaft_user));
}

/// Get all user's balances as a map from user ID to [User](crate::db::User)
/// object.
fn get_api_balances(
    (state, _user): (web::Data<AppState>, AuthenticatedUser),
) -> Box<dyn Future<Item = Json<impl Serialize>, Error = Error>> {
    let f = state
        .database
        .get_all_users()
        .map_err(ErrorInternalServerError)
        .map(Json);

    Box::new(f)
}

/// Get most recent transactions
fn get_api_transactions(
    (state, _user): (web::Data<AppState>, AuthenticatedUser),
) -> Box<dyn Future<Item = Json<Vec<db::Transaction>>, Error = Error>> {
    let f = state
        .database
        .get_last_transactions(20)
        .map_err(ErrorInternalServerError)
        .map(Json);

    Box::new(f)
}

/// Create a new transaction.
///
/// Returns an empty json object.
fn shaft_user(
    (req, state, user, body): (
        HttpRequest,
        web::Data<AppState>,
        AuthenticatedUser,
        Json<ShaftUserBody>,
    ),
) -> Box<dyn Future<Item = Json<impl Serialize>, Error = Error>> {
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

    let f = state
        .database
        .shaft_user(db::Transaction {
            shafter: user.user_id.clone(),
            shaftee: other_user.clone(),
            amount,
            datetime: chrono::Utc::now(),
            reason,
        })
        .map_err(ErrorInternalServerError)
        .map(move |_| {
            info!(
                logger, "Shafted user";
                "other_user" => other_user, "amount" => amount
            );

            Json(json!({}))
        });

    Box::new(f)
}
