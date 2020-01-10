//! The web form API for interacting with shaft.

use actix_http::httpmessage::HttpMessage;
use actix_web::web::ServiceConfig;
use actix_web::{error, web, Error, HttpRequest, HttpResponse};
use chrono;
use hyper::header::{LOCATION, SET_COOKIE};
use itertools::Itertools;

use crate::db;
use crate::rest::{AppState, AuthenticatedUser, ShaftUserBody};

use slog::Logger;

/// Register servlets with HTTP app
pub fn register_servlets(config: &mut ServiceConfig) {
    config
        .route("/", web::get().to(root))
        .route("/home", web::get().to(get_balances))
        .route("/login", web::get().to(show_login))
        .route("/logout", web::post().to(logout))
        .route("/transactions", web::get().to(get_transactions))
        .route("/shaft", web::post().to(shaft_user))
        .route("/health", web::get().to(|| async { "OK" }));
}

/// The top level root. Redirects to /home or /login.
async fn root((req, state): (HttpRequest, web::Data<AppState>)) -> Result<HttpResponse, Error> {
    if let Some(token) = req.cookie("token") {
        let user_opt = state
            .database
            .get_user_from_token(token.value().to_string())
            .await
            .map_err(error::ErrorInternalServerError)?;
        if user_opt.is_some() {
            Ok(HttpResponse::Found().header(LOCATION, "home").finish())
        } else {
            Ok(HttpResponse::Found().header(LOCATION, "login").finish())
        }
    } else {
        Ok(HttpResponse::Found().header(LOCATION, "login").finish())
    }
}

/// Get home page with current balances of all users.
async fn get_balances(
    (user, state): (AuthenticatedUser, web::Data<AppState>),
) -> Result<HttpResponse, Error> {
    let hb = state.handlebars.clone();
    let all_users = state
        .database
        .get_all_users()
        .await
        .map_err(error::ErrorInternalServerError)?;

    let mut vec = all_users.values().collect_vec();
    vec.sort_by_key(|e| e.balance);

    let s = hb
        .render(
            "index",
            &json!({
                "display_name": &user.display_name,
                "balances": vec,
            }),
        )
        .map_err(|s| error::ErrorInternalServerError(s.to_string()))?;

    let r = HttpResponse::Ok()
        .content_type("text/html")
        .content_length(s.len() as u64)
        .body(s);

    Ok(r)
}

/// Get list of recent transcations page.
async fn get_transactions(
    (user, state): (AuthenticatedUser, web::Data<AppState>),
) -> Result<HttpResponse, Error> {
    let all_users = state
        .database
        .get_all_users()
        .await
        .map_err(error::ErrorInternalServerError)?;

    let transactions = state
        .database
        .get_last_transactions(20)
        .await
        .map_err(error::ErrorInternalServerError)?;

    let page = state
        .handlebars
        .render(
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
        .map_err(|e| error::ErrorInternalServerError(e.to_string()))?;

    Ok(HttpResponse::Ok()
        .content_type("text/html")
        .content_length(page.len() as u64)
        .body(page))
}

/// Commit a new tranaction request
async fn shaft_user(
    (user, req, state, body): (
        AuthenticatedUser,
        HttpRequest,
        web::Data<AppState>,
        web::Form<ShaftUserBody>,
    ),
) -> Result<HttpResponse, Error> {
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
        .map_err(error::ErrorInternalServerError)?;

    info!(
        logger, "Shafted user";
        "other_user" => other_user, "amount" => amount
    );

    Ok(HttpResponse::Found()
        .header(LOCATION, ".")
        .body("Success\n"))
}

/// Login page.
async fn show_login(state: web::Data<AppState>) -> Result<HttpResponse, Error> {
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
async fn logout((req, state): (HttpRequest, web::Data<AppState>)) -> Result<HttpResponse, Error> {
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
        db.delete_token(token.value().to_string())
            .await
            .map_err(error::ErrorInternalServerError)?;
    }

    Ok(resp)
}
