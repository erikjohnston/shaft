//! Handles authenticating an incoming request.

use actix_http::error;
use actix_http::httpmessage::HttpMessage;
use actix_service::{Service, Transform};
use actix_web::dev::{Payload, ServiceRequest, ServiceResponse};
use actix_web::{self, Error, FromRequest, HttpRequest, HttpResponse};
use futures::future::ok;
use futures::Future;
use futures::FutureExt;
use hyper::header::LOCATION;
use slog::Logger;

use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll};

use crate::db::Database;
use crate::rest::AppState;

/// Middleware for annotating requests with valid user authentication.
///
/// **Note**: Does not deny unauthenticated requests.
pub struct AuthenticateUser {
    database: Arc<dyn Database>,
}

impl AuthenticateUser {
    pub fn new(database: Arc<dyn Database>) -> AuthenticateUser {
        AuthenticateUser { database }
    }
}

impl<S, B> Transform<S> for AuthenticateUser
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = AuthenticateUserService<S>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Transform, Self::InitError>>>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(AuthenticateUserService {
            database: self.database.clone(),
            service: Rc::new(RefCell::new(service)),
        })
        .boxed_local()
    }
}

pub struct AuthenticateUserService<S> {
    database: Arc<dyn Database>,
    service: Rc<RefCell<S>>,
}

/// An authenticated user session.
///
/// Implements FromRequest so can be used as an extractor to require a valid
/// session for the endpoint.
#[derive(Clone)]
pub struct AuthenticatedUser {
    pub user_id: String,
    pub display_name: String,
}

impl<S, B> Service for AuthenticateUserService<S>
where
    B: 'static,
    S: Service<Request = ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&mut self, ctx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.borrow_mut().poll_ready(ctx)
    }

    fn call(&mut self, req: ServiceRequest) -> Self::Future {
        let db = self.database.clone();
        let service = self.service.clone();

        let token = if let Some(token) = req.cookie("token") {
            token.value().to_string()
        } else {
            return service.borrow_mut().call(req).boxed_local();
        };

        async move {
            let user_opt = db
                .get_user_from_token(token)
                .await
                .map_err(error::ErrorInternalServerError)?;

            if let Some(user) = user_opt {
                let logger = req
                    .extensions()
                    .get::<Logger>()
                    .expect("logger no longer installed in request")
                    .clone();
                let logger = logger.new(o!("user_id" => user.user_id.clone()));
                info!(logger, "Authenticated user");
                req.extensions_mut().insert(logger);

                req.extensions_mut().insert(AuthenticatedUser {
                    user_id: user.user_id,
                    display_name: user.display_name,
                });
            }

            service.borrow_mut().call(req).await
        }
        .boxed_local()
    }
}

impl FromRequest for AuthenticatedUser {
    type Config = ();
    type Error = Error;
    type Future = futures::future::LocalBoxFuture<'static, Result<AuthenticatedUser, Error>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let root = &req.app_data::<AppState>().unwrap().config.web_root;
        let login_url = format!("{}/login", root);

        let res = req
            .extensions()
            .get::<AuthenticatedUser>()
            .map(Clone::clone)
            .ok_or_else(|| {
                let resp = HttpResponse::Found().header(&LOCATION, login_url).finish();
                error::InternalError::from_response("Please login", resp).into()
            });

        async { res }.boxed_local()
    }
}
