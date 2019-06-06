//! Handles all REST endpoints

use actix_web::web::ServiceConfig;
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

pub use self::auth::{AuthenticateUser, AuthenticatedUser};
pub use self::logger::MiddlewareLogger;

/// Registers all servlets in this module with the HTTP app.
pub fn register_servlets(config: &mut ServiceConfig, state: &AppState) {
    github_login::register_servlets(config);
    api::register_servlets(config);
    static_files::register_servlets(config, state);
    web::register_servlets(config)
}

// Holds the state for the shared state of the app. Gets cloned to each thread.
#[derive(Clone)]
pub struct AppState {
    pub database: Arc<dyn db::Database>,
    pub config: AppConfig,
    pub cpu_pool: futures_cpupool::CpuPool,
    pub handlebars: Arc<handlebars::Handlebars>,
    pub http_client: HttpClient,
}

/// Read only config for the app
#[derive(Clone)]
pub struct AppConfig {
    pub github_client_id: String,
    pub github_client_secret: String,
    pub github_state: String,
    pub web_root: String,
    pub required_org: String,
    pub resource_dir: String,
}

/// Formats the current time plus two weeks into a cookie expires field.
pub fn get_expires_string() -> String {
    let dt = chrono::Utc::now() + chrono::Duration::weeks(2);
    const ITEMS: &[chrono::format::Item<'static>] =
        &[chrono::format::Item::Fixed(chrono::format::Fixed::RFC2822)];
    dt.format_with_items(ITEMS.iter().cloned()).to_string()
}

/// Format pence into a pretty pounds string
fn format_pence_as_pounds(pence: i64) -> String {
    if pence < 0 {
        format!("-£{:2}.{:02}", -pence / 100, -pence % 100)
    } else {
        format!("£{:2}.{:02}", pence / 100, pence % 100)
    }
}

/// Handlebars helper function for formatting pence as points.
pub fn format_pence_as_pounds_helper(
    h: &handlebars::Helper,
    _: &handlebars::Handlebars,
    _: &handlebars::Context,
    _: &mut handlebars::RenderContext,
    out: &mut dyn handlebars::Output,
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

/// The body of a incoming request shaft the given user.
#[derive(Deserialize)]
struct ShaftUserBody {
    /// The other party in the transaction.
    other_user: String,
    /// The amount in pence owed. Positive means shafter is owed money by other
    /// user, negative means shafer owes money.
    amount: i64,
    /// The human readable description of the transasction.
    reason: String,
}
