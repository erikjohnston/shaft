use crate::rest::AppState;
use actix_web::{fs, App};

pub fn register_servlets(app: App<AppState>) -> App<AppState> {
    let dir = app.state().config.resource_dir.clone();

    app.handler("/static", fs::StaticFiles::new(dir).unwrap())
}
