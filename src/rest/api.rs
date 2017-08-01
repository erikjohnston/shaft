use gleam::{Ctx, Server};
use chrono;
use hyper::Method;
use futures::Future;

use db;
use rest::{AppState, AuthenticatedUser, InternalServerError, Json, ShaftUserBody};
use rest::body_parser::JsonBody;


pub fn register_servlets(server: &mut Server) {
    server.add_route(Method::Get, "/api/balances", get_api_balances);
    server.add_route(Method::Get, "/api/transactions", get_api_transactions);

    server.add_route_with_body(Method::Post, "/api/shaft", shaft_user);
}


fn get_api_balances(_: Ctx, state: AppState, _: AuthenticatedUser)
    -> Box<Future<Item = Json, Error = InternalServerError>>
{
    let f = state.db.get_all_users()
        .map_err(InternalServerError::from)
        .and_then(Json::new);

    Box::new(f)
}

fn get_api_transactions(_: Ctx, state: AppState, _: AuthenticatedUser)
    -> Box<Future<Item = Json, Error = InternalServerError>>
{
    let f = state.db.get_last_transactions(20)
        .map_err(InternalServerError::from)
        .and_then(Json::new);

    Box::new(f)
}


fn shaft_user(
    ctx: Ctx,
    state: AppState,
    req: AuthenticatedUser,
    body: JsonBody<ShaftUserBody>
)
    -> Box<Future<Item = Json, Error = InternalServerError>>
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
        .and_then(move |_| {
            info!(
                ctx, "Shafted user";
                "other_user" => other_user, "amount" => amount
            );

            Json::new(json!({}))
        });

    Box::new(f)
}
