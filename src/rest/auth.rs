use actix_web::{self, error, middleware, Error, FromRequest, HttpRequest, HttpResponse};
use cookie::Cookie;
use futures::{Future, IntoFuture};

use crate::db;
use crate::rest::AppState;

use slog::Logger;

pub struct AuthenticateUser;

#[derive(Clone)]
pub struct AuthenticatedUser {
    pub user_id: String,
    pub display_name: String,
}

impl middleware::Middleware<AppState> for AuthenticateUser {
    fn start(&self, req: &HttpRequest<AppState>) -> actix_web::Result<middleware::Started> {
        let logger = req
            .extensions()
            .get::<Logger>()
            .expect("no logger installed in request")
            .clone();

        let req = req.clone();
        let db = req.state().database.clone();

        let cookie_result = {
            req.cookie("token")
                .map(|cookie| cookie.value().to_string())
                .ok_or_else(|| HttpResponse::Unauthorized().body("Please login again"))
        };

        let f = cookie_result
            .into_future()
            .and_then(move |token| {
                db.get_user_from_token(token).map_err(|err| {
                    HttpResponse::InternalServerError().body(format!("Error: {}", err))
                })
            })
            .and_then(move |user_id_opt| {
                user_id_opt.ok_or_else(|| HttpResponse::Unauthorized().body("Please login again"))
            })
            .and_then(move |user| {
                let logger = logger.new(o!("user_id" => user.user_id.clone()));
                info!(logger, "Authenticated user");
                req.extensions_mut().insert(logger);

                req.extensions_mut().insert(AuthenticatedUser {
                    user_id: user.user_id,
                    display_name: user.display_name,
                });

                Ok(None)
            })
            .or_else(|err| Ok(Some(err)));

        Ok(middleware::Started::Future(Box::new(f)))
    }
}

impl FromRequest<AppState> for AuthenticatedUser {
    type Config = ();
    type Result = Result<AuthenticatedUser, Error>;

    fn from_request(req: &HttpRequest<AppState>, _: &Self::Config) -> Self::Result {
        req.extensions()
            .get::<AuthenticatedUser>()
            .map(Clone::clone)
            .ok_or_else(|| error::ErrorUnauthorized("Please login again"))
    }
}

pub fn get_user_from_cookie(
    db: &db::Database,
    cookie: &Cookie,
) -> Box<Future<Item = Option<db::User>, Error = Error>> {
    let f = db
        .get_user_from_token(cookie.value().to_string())
        .map_err(error::ErrorInternalServerError);
    Box::new(f)
}
