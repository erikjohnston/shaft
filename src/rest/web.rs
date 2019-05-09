//! The web form API for interacting with shaft.

use actix_web::{error, App, Error, Form, HttpRequest, HttpResponse};
use chrono;
use futures::{Future, IntoFuture};
use hyper::header::{LOCATION, SET_COOKIE};
use hyper::Method;
use itertools::Itertools;

use crate::db;
use crate::rest::{AppState, AuthenticatedUser, ShaftUserBody};

use slog::Logger;

/// Register servlets with HTTP app
pub fn register_servlets(app: App<AppState>) -> App<AppState> {
    app.resource(r"/", |r| r.method(Method::GET).f(root))
        .resource(r"/home", |r| r.method(Method::GET).with(get_balances))
        .resource(r"/login", |r| r.method(Method::GET).f(show_login))
        .resource(r"/logout", |r| r.method(Method::POST).f(logout))
        .resource(r"/transactions", |r| {
            r.method(Method::GET).with(get_transactions)
        })
        .resource(r"/shaft", |r| r.method(Method::POST).with(shaft_user))
}

/// The top level root. Redirects to /home or /login.
fn root(req: &HttpRequest<AppState>) -> Box<Future<Item = HttpResponse, Error = Error>> {
    if let Some(token) = req.cookie("token") {
        let f = req
            .state()
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

/// Get home page with current balances of all users.
fn get_balances(
    (user, req): (AuthenticatedUser, HttpRequest<AppState>),
) -> Box<Future<Item = HttpResponse, Error = Error>> {
    let hb = req.state().handlebars.clone();
    let f = req
        .state()
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
    (user, req): (AuthenticatedUser, HttpRequest<AppState>),
) -> Box<Future<Item = HttpResponse, Error = Error>> {
    let hb = req.state().handlebars.clone();
    let f = req
        .state()
        .database
        .get_all_users()
        .join(req.state().database.get_last_transactions(20))
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
    (user, req, body): (
        AuthenticatedUser,
        HttpRequest<AppState>,
        Form<ShaftUserBody>,
    ),
) -> Box<Future<Item = HttpResponse, Error = Error>> {
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
        .map_err(error::ErrorInternalServerError)
        .map(move |_| {
            info!(
                logger, "Shafted user";
                "other_user" => other_user, "amount" => amount
            );

            HttpResponse::Found()
                .header(LOCATION, ".")
                .body("Success\n")
        });

    Box::new(f)
}

/// Login page.
fn show_login(req: &HttpRequest<AppState>) -> Result<HttpResponse, Error> {
    let hb = &req.state().handlebars;
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
fn logout(req: &HttpRequest<AppState>) -> Box<Future<Item = HttpResponse, Error = Error>> {
    let logger = req
        .extensions()
        .get::<Logger>()
        .expect("no logger installed in request")
        .clone();

    let db = req.state().database.clone();

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
