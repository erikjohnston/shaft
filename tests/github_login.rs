use actix_web::test;
use bytes::Bytes;
use handlebars::Handlebars;
use http::header::HeaderValue;
use url::Url;

use std::collections::BTreeMap;

use shaft::db::SqliteDatabase;
use shaft::rest::{register_servlets, AppConfig, AppState, AuthenticateUser};

fn setup_app() -> (test::TestServer, AppState) {
    let config = AppConfig {
        github_client_id: "fake_client_id".to_owned(),
        github_client_secret: "fake_client_secret".to_owned(),
        github_state: "fake_state".to_owned(),
        web_root: String::new(),
        required_org: "fake_org".to_owned(),
        resource_dir: "res".to_owned(),
    };

    let database = SqliteDatabase::with_path(":memory:");

    let app_state = AppState::new(config, Handlebars::new(), database);

    let state = app_state.clone();
    let srv = test::start(move || {
        actix_web::App::new()
            .data(state.clone())
            .wrap(AuthenticateUser::new(state.database.clone()))
            .configure(|config| register_servlets(config, &state))
    });

    (srv, app_state)
}

#[actix_rt::test]
async fn test_health() {
    let (srv, _) = setup_app();

    let req = srv.get("/health");
    let mut response = req.send().await.unwrap();
    assert!(response.status().is_success());

    let result = response.body().await.unwrap();
    assert_eq!(result, Bytes::from_static(b"OK"))
}

#[actix_rt::test]
async fn test_initial_redirect() {
    let (srv, _) = setup_app();

    let req = srv.get("/");
    let response = req.send().await.unwrap();
    assert!(response.status().is_redirection());

    assert_eq!(
        response.headers().get("location"),
        Some(&HeaderValue::from_static("login"))
    );
}

#[actix_rt::test]
async fn test_github_login() {
    let (srv, app_state) = setup_app();

    // Check that the client gets redirected to the right github page.
    let req = srv.get("/github/login");
    let response = req.send().await.unwrap();
    assert!(response.status().is_redirection());

    let url = Url::parse(
        response
            .headers()
            .get("location")
            .expect("location header")
            .to_str()
            .expect("uft8"),
    )
    .unwrap();

    assert_eq!(url.scheme(), "https");
    assert_eq!(url.host_str(), Some("github.com"));
    assert_eq!(url.path(), "/login/oauth/authorize");

    let query_map: BTreeMap<String, String> = url.query_pairs().into_owned().collect();

    assert_eq!(
        query_map,
        vec![
            (
                "client_id".to_owned(),
                app_state.config.github_client_id.clone()
            ),
            ("state".to_owned(), app_state.config.github_state.clone()),
            ("scope".to_owned(), "read:org".to_owned()),
        ]
        .into_iter()
        .collect()
    );
}
