//! The JSON API for interacting with shaft

use actix_web::{error::ErrorInternalServerError, App, Error, HttpRequest, Json};
use chrono;
use futures::Future;
use hyper::Method;
use serde::Serialize;

use crate::db;
use crate::rest::{AppState, AuthenticatedUser, ShaftUserBody};

use slog::Logger;

/// Register servlets with HTTP app
pub fn register_servlets(app: App<AppState>) -> App<AppState> {
    app.resource("/api/balances", |r| {
        r.method(Method::GET).with(get_api_balances)
    })
    .resource("/api/transactions", |r| {
        r.method(Method::GET).with(get_api_transactions)
    })
    .resource("/api/shaft", |r| r.method(Method::POST).with(shaft_user))
}

/// Get all user's balances as a map from user ID to [User](crate::db::User)
/// object.
fn get_api_balances(
    (req, _user): (HttpRequest<AppState>, AuthenticatedUser),
) -> Box<Future<Item = Json<impl Serialize>, Error = Error>> {
    let f = req
        .state()
        .database
        .get_all_users()
        .map_err(ErrorInternalServerError)
        .map(Json);

    Box::new(f)
}

/// Get most recent transactions
fn get_api_transactions(
    (req, _user): (HttpRequest<AppState>, AuthenticatedUser),
) -> Box<Future<Item = Json<Vec<db::Transaction>>, Error = Error>> {
    let f = req
        .state()
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
    (req, user, body): (
        HttpRequest<AppState>,
        AuthenticatedUser,
        Json<ShaftUserBody>,
    ),
) -> Box<Future<Item = Json<impl Serialize>, Error = Error>> {
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

    let f = req
        .state()
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
