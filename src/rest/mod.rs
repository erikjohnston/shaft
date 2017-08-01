use gleam::{Server, IntoResponse};
use chrono;
use hyper;
use hyper::StatusCode;
use hyper::header::ContentType;
use futures_cpupool::CpuPool;
use handlebars;
use serde_json;
use serde::Serialize;

use std::fmt::Display;
use std::rc::Rc;

use db;
use HttpClient;

mod api;
mod auth;
mod github_login;
mod body_parser;
mod static_files;
mod web;

use self::auth::AuthenticatedUser;


pub fn register_servlets(server: &mut Server) {
    github_login::register_servlets(server);
    api::register_servlets(server);
    static_files::register_servlets(server);
    web::register_servlets(server);
}


#[derive(GleamState)]
pub struct AppState {
    pub db: Rc<db::Database>,
    pub http_client: HttpClient,
    pub handlebars: Rc<handlebars::Handlebars>,
    pub config: AppConfig,
    pub cpu_pool: CpuPool,
}

#[derive(GleamState)]
pub struct AppStateConfig {
    pub config: AppConfig,
}

#[derive(Clone)]
pub struct AppConfig {
    pub github_client_id: String,
    pub github_client_secret: String,
    pub github_state: String,
    pub web_root: String,
    pub required_org: String,
}


pub fn get_expires_string() -> String {
    let dt = chrono::Utc::now() + chrono::Duration::weeks(2);
    const ITEMS: &'static [chrono::format::Item<'static>] = &[
        chrono::format::Item::Fixed(chrono::format::Fixed::RFC2822)
    ];
    dt.format_with_items(ITEMS.iter().cloned()).to_string()
}


fn format_pence_as_pounds(pence: i64) -> String {
    if pence < 0 {
        format!("-£{:2}.{:02}", -pence/100, -pence % 100)
    } else {
        format!("£{:2}.{:02}", pence/100, pence % 100)
    }
}

pub fn format_pence_as_pounds_helper(
    h: &handlebars::Helper, _: &handlebars::Handlebars, rc: &mut handlebars::RenderContext
) -> Result<(), handlebars::RenderError> {
    let param = h.param(0).unwrap();

    match *param.value() {
        serde_json::Value::Number(ref number) => {
            let pence = number.as_i64()
                .ok_or_else(|| handlebars::RenderError::new("Param must be a number"))?;
            rc.writer.write(format_pence_as_pounds(pence).as_bytes())?;
            Ok(())
        }
        _ => {
            Err(handlebars::RenderError::new("Param must be a number"))
        }
    }
}


#[derive(Deserialize)]
struct ShaftUserBody {
    other_user: String,
    amount: i64,
    reason: String,
}


pub struct Html(String);

impl IntoResponse for Html {
    fn into_response(self) -> hyper::Response {
        hyper::Response::new()
            .with_header(ContentType::html())
            .with_body(self.0)
    }
}

impl<T> From<T> for Html where String: From<T> {
    fn from(t: T) -> Self {
        Html(t.into())
    }
}

pub struct Json(String);

impl IntoResponse for Json {
    fn into_response(self) -> hyper::Response {
        hyper::Response::new()
            .with_header(ContentType::json())
            .with_body(self.0)
    }
}

impl Json {
    fn new<T>(t: T) -> Result<Self, InternalServerError> where T: Serialize {
        Ok(Json(serde_json::to_string(&t)?))
    }
}

#[derive(Debug, Clone)]
pub struct InternalServerError(String);

impl IntoResponse for InternalServerError {
    fn into_response(self) -> hyper::Response {
        hyper::Response::new()
            .with_status(StatusCode::InternalServerError)
            .with_body(self.0)
    }
}

impl<E> From<E> for InternalServerError where E: Display {
    fn from(err: E) -> Self {
        InternalServerError(format!("{}", err))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NotFound;

impl IntoResponse for NotFound {
    fn into_response(self) -> hyper::Response {
        hyper::Response::new()
            .with_status(StatusCode::NotFound)
            .with_body("Not found")
    }
}



quick_error!{
    #[derive(Debug)]
    pub enum HttpError {
        Internal(err: InternalServerError) {
            from()
        }
        NotFound(err: NotFound) {
            from()
        }
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> hyper::Response {
        match self {
            HttpError::Internal(err) => err.into_response(),
            HttpError::NotFound(err) => err.into_response(),
        }
    }
}