//! Renders static files in resource directory

use std::path::Path;

use actix_web::web::ServiceConfig;

use crate::rest::AppState;

pub fn register_servlets(config: &mut ServiceConfig, state: &AppState) {
    let res_dir = Path::new(&state.config.resource_dir);
    let static_dir = res_dir.join("static");

    config.service(actix_files::Files::new("/static", static_dir));
}
