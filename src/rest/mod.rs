use actix_web::App;
use chrono;
use handlebars;
use serde_json;

use std::sync::Arc;

use crate::db;
use crate::HttpClient;

mod api;
mod auth;
mod github_login;
mod logger;
mod static_files;
mod web;

use self::auth::{AuthenticateUser, AuthenticatedUser};
pub use self::logger::MiddlewareLogger;

pub fn register_servlets(app: App<AppState>) -> App<AppState> {
    let app = app.middleware(AuthenticateUser);

    let app = github_login::register_servlets(app);
    let app = api::register_servlets(app);
    let app = static_files::register_servlets(app);
    web::register_servlets(app)
}

#[derive(Clone)]
pub struct AppState {
    pub database: Arc<db::Database>,
    pub config: AppConfig,
    pub cpu_pool: futures_cpupool::CpuPool,
    pub handlebars: Arc<handlebars::Handlebars>,
    pub http_client: HttpClient,
}

#[derive(Clone)]
pub struct AppConfig {
    pub github_client_id: String,
    pub github_client_secret: String,
    pub github_state: String,
    pub web_root: String,
    pub required_org: String,
    pub resource_dir: String,
}

pub fn get_expires_string() -> String {
    let dt = chrono::Utc::now() + chrono::Duration::weeks(2);
    const ITEMS: &[chrono::format::Item<'static>] =
        &[chrono::format::Item::Fixed(chrono::format::Fixed::RFC2822)];
    dt.format_with_items(ITEMS.iter().cloned()).to_string()
}

fn format_pence_as_pounds(pence: i64) -> String {
    if pence < 0 {
        format!("-£{:2}.{:02}", -pence / 100, -pence % 100)
    } else {
        format!("£{:2}.{:02}", pence / 100, pence % 100)
    }
}

pub fn format_pence_as_pounds_helper(
    h: &handlebars::Helper,
    _: &handlebars::Handlebars,
    _: &handlebars::Context,
    _: &mut handlebars::RenderContext,
    out: &mut handlebars::Output,
) -> Result<(), handlebars::RenderError> {
    let param = h.param(0).unwrap();

    match *param.value() {
        serde_json::Value::Number(ref number) => {
            let pence = number
                .as_i64()
                .ok_or_else(|| handlebars::RenderError::new("Param must be a number"))?;
            out.write(&format_pence_as_pounds(pence))?;
            Ok(())
        }
        _ => Err(handlebars::RenderError::new("Param must be a number")),
    }
}

#[derive(Deserialize)]
struct ShaftUserBody {
    other_user: String,
    amount: i64,
    reason: String,
}
