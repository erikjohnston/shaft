//! The web form API for interacting with shaft.

use actix_http::httpmessage::HttpMessage;
use actix_web::web::ServiceConfig;
use actix_web::{error, web, Error, HttpRequest, HttpResponse};
use chrono;
use futures::{Future, IntoFuture};
use hyper::header::{LOCATION, SET_COOKIE};
use itertools::Itertools;

use crate::db;
use crate::rest::{AppState, AuthenticatedUser, ShaftUserBody};

use slog::Logger;

/// Register servlets with HTTP app
pub fn register_servlets(config: &mut ServiceConfig) {
    config
        .route("/", web::get().to_async(root))
        .route("/home", web::get().to_async(get_balances))
        .route("/login", web::get().to_async(show_login))
        .route("/logout", web::post().to_async(logout))
        .route("/transactions", web::get().to_async(get_transactions))
        // GET /shaft should redirect /home as we cannot extract a reason to preserve from it.
        .route("/shaft", web::get().to_async(root))
        .route("/shaft", web::post().to_async(shaft_user));
}

/// The top level root. Redirects to /home or /login.
fn root(
    (req, state): (HttpRequest, web::Data<AppState>),
) -> Box<dyn Future<Item = HttpResponse, Error = Error>> {
    if let Some(token) = req.cookie("token") {
        let f = state
            .database
            .get_user_from_token(token.value().to_string())
            .map_err(error::ErrorInternalServerError)
            .map(move |user_opt| {
                if user_opt.is_some() {
                    HttpResponse::Found().header(LOCATION, "home").finish()
                } else {
                    HttpResponse::Found().header(LOCATION, "login").finish()
                }
            })
            .map_err(error::ErrorInternalServerError);

        Box::new(f)
    } else {
        let f = futures::future::ok(HttpResponse::Found().header(LOCATION, "login").finish());

        Box::new(f)
    }
}

fn get_balances(
    (user, state): (AuthenticatedUser, web::Data<AppState>),
) -> Box<dyn Future<Item = HttpResponse, Error = Error>> {
    get_balances_impl((user, state), None)
}

/// Get home page with current balances of all users.
fn get_balances_impl(
    (user, state): (AuthenticatedUser, web::Data<AppState>),
    preserved_reason: Option<String>,
) -> Box<dyn Future<Item = HttpResponse, Error = Error>> {
    let hb = state.handlebars.clone();
    let f = state
        .database
        .get_all_users()
        .map_err(error::ErrorInternalServerError)
        .and_then(move |all_users| {
            let mut vec = all_users.values().collect_vec();
            vec.sort_by_key(|e| e.balance);

            let s = hb
                .render(
                    "index",
                    &json!({
                        "display_name": &user.display_name,
                        "balances": vec,
                        "reason": &preserved_reason,
                    }),
                )
                .map_err(|s| error::ErrorInternalServerError(s.to_string()))?;

            let r = HttpResponse::Ok()
                .content_type("text/html")
                .content_length(s.len() as u64)
                .body(s);

            Ok(r)
        });

    Box::new(f)
}

/// Get list of recent transcations page.
fn get_transactions(
    (user, state): (AuthenticatedUser, web::Data<AppState>),
) -> Box<dyn Future<Item = HttpResponse, Error = Error>> {
    let hb = state.handlebars.clone();
    let f = state
        .database
        .get_all_users()
        .join(state.database.get_last_transactions(20))
        .map_err(error::ErrorInternalServerError)
        .and_then(move |(all_users, transactions)| {
            hb.render(
                "transactions",
                &json!({
                    "display_name": &user.display_name,
                    "transactions": transactions
                        .into_iter()
                        .map(|txn| json!({
                            "amount": txn.amount,
                            "shafter_id": txn.shafter,
                            "shafter_name": all_users.get(&txn.shafter)
                                .map(|u| &u.display_name as &str)
                                .unwrap_or(&txn.shafter),
                            "shaftee_id": txn.shaftee,
                            "shaftee_name": all_users.get(&txn.shaftee)
                                .map(|u| &u.display_name as &str)
                                .unwrap_or(&txn.shaftee),
                            "date": format!("{}", txn.datetime.format("%d %b %Y")),
                            "reason": txn.reason,
                        }))
                        .collect_vec(),
                }),
            )
            .map_err(|s| error::ErrorInternalServerError(s.to_string()))
        })
        .map(|s| {
            HttpResponse::Ok()
                .content_type("text/html")
                .content_length(s.len() as u64)
                .body(s)
        })
        .from_err();

    Box::new(f)
}

/// Commit a new tranaction request
fn shaft_user(
    (user, req, state, body): (
        AuthenticatedUser,
        HttpRequest,
        web::Data<AppState>,
        web::Form<ShaftUserBody>,
    ),
) -> Box<dyn Future<Item = HttpResponse, Error = Error>> {
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
            reason: reason.clone(),
        })
        .map_err(error::ErrorInternalServerError)
        .and_then(move |_| {
            info!(
                logger, "Shafted user";
                "other_user" => other_user, "amount" => amount
            );

            get_balances_impl((user, state), Some(reason))
        });

    Box::new(f)
}

/// Login page.
fn show_login(state: web::Data<AppState>) -> Result<HttpResponse, Error> {
    let hb = &state.handlebars;
    let s = hb
        .render("login", &json!({}))
        .map_err(|s| error::ErrorInternalServerError(s.to_string()))?;

    let r = HttpResponse::Ok()
        .content_type("text/html")
        .content_length(s.len() as u64)
        .body(s);

    Ok(r)
}

/// Logout user session.
fn logout(
    (req, state): (HttpRequest, web::Data<AppState>),
) -> Box<dyn Future<Item = HttpResponse, Error = Error>> {
    let logger = req
        .extensions()
        .get::<Logger>()
        .expect("no logger installed in request")
        .clone();

    let db = state.database.clone();

    let resp = HttpResponse::Found()
        .header(LOCATION, ".")
        .header(
            SET_COOKIE,
            "token=; HttpOnly; Secure; Path=/; Expires=Thu, 01 Jan 1970 00:00:00 GMT; SameSite=lax",
        )
        .body("Signed out\n");

    info!(logger, "Got logout request");

    if let Some(token) = req.cookie("token") {
        Box::new(
            db.delete_token(token.value().to_string())
                .map_err(error::ErrorInternalServerError)
                .map(|_| resp),
        )
    } else {
        Box::new(Ok(resp).into_future())
    }
}
