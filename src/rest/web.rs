use gleam::{Ctx, Server};
use chrono;
use hyper::{self, Method, Response, StatusCode};
use hyper::header::{Header, Cookie, Location, SetCookie};
use futures::{Future, IntoFuture};
use itertools::Itertools;

use db;
use rest::{AppState, AuthenticatedUser, InternalServerError, Html, ShaftUserBody};
use rest::body_parser::FormUrlEncoded;
use rest::auth::get_user_from_cookie;



pub fn register_servlets(server: &mut Server) {
    server.add_route(Method::Get, "/", root);
    server.add_route(Method::Get, "/home", get_balances);
    server.add_route(Method::Get, "/login", show_login);
    server.add_route(Method::Post, "/logout", logout);
    server.add_route(Method::Get, "/transactions", get_transactions);
    server.add_route_with_body(Method::Post, "/shaft", shaft_user);
}


fn root(_: Ctx, state: AppState, req: CookieRequest)
    -> Box<Future<Item = Response, Error = InternalServerError>>
{
    if let Some (header_cookie) = req.header_cookie {
        let f = Cookie::parse_header(&header_cookie.into())
            .map_err(InternalServerError::from)
            .into_future()
            .and_then(move |cookie| {
                get_user_from_cookie(state.db.clone(), &cookie)
            })
            .map(move |user_opt| {
                if user_opt.is_some() {
                    Response::new()
                        .with_status(StatusCode::Found)
                        .with_header(Location::new("home"))
                } else {
                    Response::new()
                        .with_status(StatusCode::Found)
                        .with_header(Location::new("login"))
                }
            });

        Box::new(f)
    } else {
        Ok(
            Response::new()
                .with_status(StatusCode::Found)
                .with_header(Location::new("login"))
        )
        .into_future()
        .boxed()
    }
}

fn get_balances(_: Ctx, state: AppState, user: AuthenticatedUser)
    -> Box<Future<Item = Html, Error = InternalServerError>>
{
    let hb = state.handlebars.clone();
    let f = state.db.get_all_users()
        .map_err(InternalServerError::from)
        .and_then(move |all_users| {
            let mut vec = all_users.values().collect_vec();
            vec.sort_unstable_by_key(|e| e.balance);

            let s = hb.render("index", &json!({
                "display_name": &user.display_name,
                "balances": vec,
            }))?;

            Ok(Html(s))
        })
        .from_err();

    Box::new(f)
}

fn get_transactions(_: Ctx, state: AppState, user: AuthenticatedUser)
    -> Box<Future<Item = Html, Error = InternalServerError>>
{
    let hb = state.handlebars.clone();
    let f = state.db.get_all_users()
        .join(state.db.get_last_transactions(20))
        .map_err(InternalServerError::from)
        .and_then(move |(all_users, transactions)| {
            let s = hb.render("transactions", &json!({
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
            }))?;

            Ok(Html(s))
        })
        .from_err();

    Box::new(f)
}

fn shaft_user(
    ctx: Ctx,
    state: AppState,
    req: AuthenticatedUser,
    body: FormUrlEncoded<ShaftUserBody>
)
    -> Box<Future<Item = Response, Error = InternalServerError>>
{
    let ShaftUserBody { other_user, amount, reason } = body.0;

    let f = state.db.shaft_user(db::Transaction {
            shafter: req.user_id.clone(),
            shaftee: other_user.clone(),
            amount: amount,
            datetime: chrono::Utc::now(),
            reason: reason,
        })
        .from_err()
        .map(move |_| {
            info!(
                ctx, "Shafted user";
                "other_user" => other_user, "amount" => amount
            );

            Response::new()
                .with_status(StatusCode::Found)
                .with_body("Success\n")
                .with_header(Location::new("."))
        });

    Box::new(f)
}


fn show_login(_: Ctx, state: AppState, _: ())
    -> Result<Html, InternalServerError>
{
    let hb = state.handlebars.clone();
    let s = hb.render("login", &json!({}))?;

    Ok(Html(s))
}

fn logout(ctx: Ctx, state: AppState, req: CookieRequest)
    -> Box<Future<Item = Response, Error = InternalServerError>>
{
    let db = state.db.clone();

    let resp = Response::new()
        .with_status(StatusCode::Found)
        .with_body("Signed out\n")
        .with_header(Location::new("."))
        .with_header(SetCookie(vec![
            "token=; HttpOnly; Secure; Path=/; Expires=Thu, 01 Jan 1970 00:00:00 GMT; SameSite=lax".into()
        ]));

    info!(
        ctx, "Got logout request"
    );

    let f = req.header_cookie.and_then(|header_cookie|{
            Cookie::parse_header(&header_cookie.into()).ok()
        })
        .and_then(|cookie| {
            cookie.get("token").map(String::from)
        })
        .map(|token| {
            db.delete_token(token)
                .map_err(InternalServerError::from)
                .boxed()
        })
        .unwrap_or_else(|| Ok(()).into_future().boxed())
        .map(|_| resp);

    Box::new(f)
}


#[derive(GleamFromRequest)]
struct CookieRequest {
    header_cookie: Option<String>,
}