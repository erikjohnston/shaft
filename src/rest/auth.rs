use gleam::{Ctx, FromRequest, Params, Server};
use hyper;
use hyper::StatusCode;
use hyper::header::Cookie;
use futures::{Future, IntoFuture};

use std::rc::Rc;

use db;
use rest::InternalServerError;


pub struct AuthenticatedUser {
    pub user_id: String,
    pub display_name: String,
}

impl FromRequest for AuthenticatedUser {
    type Error = hyper::Response;
    type Future = Box<Future<Item=Self, Error=Self::Error>>;

    fn from_request(server: &Server, ctx: &Ctx, req: &hyper::Request, _: &Params)
        -> Self::Future
    {
        let db: &Rc<db::Database> = server.get_state().expect("missing db");
        let db = db.clone();

        let mut ctx = ctx.clone();

        let f = req.headers().get()
            .and_then(|cookie: &Cookie| {
                cookie.get("token").map(String::from)
            })
            .ok_or_else(|| {
                hyper::Response::new()
                    .with_status(StatusCode::Unauthorized)
                    .with_body("Please login again")
            })
            .into_future()
            .and_then(move |token| {
                db.get_user_from_token(token)
                    .map_err(|err| {
                        hyper::Response::new()
                            .with_status(StatusCode::InternalServerError)
                            .with_body(format!("Error: {}", err))
                    })
            })
            .and_then(move |user_id_opt| {
                user_id_opt.ok_or_else(|| {
                        hyper::Response::new()
                            .with_status(StatusCode::Unauthorized)
                            .with_body("Please login again")
                    })
            })
            .map(move |user| {
                ctx.update_logger(o!("user_id" => user.user_id.clone()));
                info!(ctx, "Authenticated user");
                AuthenticatedUser {
                    user_id: user.user_id,
                    display_name: user.display_name,
                }
            });

        Box::new(f)
    }
}


pub fn get_user_from_cookie(db: Rc<db::Database>, cookie: &Cookie)
    -> Box<Future<Item=Option<db::User>, Error=InternalServerError>>
{
    if let Some(token) = cookie.get("token").map(String::from) {
        let f = db.get_user_from_token(token)
            .map_err(InternalServerError::from);
        Box::new(f)
    } else {
        Box::new(Ok(None).into_future())
    }
}
