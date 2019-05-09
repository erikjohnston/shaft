//! Renders static files in resource directory

use std::path::Path;

use actix_web::{fs, App};

use crate::rest::AppState;

pub fn register_servlets(app: App<AppState>) -> App<AppState> {
    let res_dir = Path::new(&app.state().config.resource_dir);
    let static_dir = res_dir.join("static");

    app.handler("/static", fs::StaticFiles::new(static_dir).unwrap())
}
