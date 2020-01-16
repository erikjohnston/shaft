use actix_http::httpmessage::HttpMessage;
use actix_web::test;
use awc::cookie::SameSite;
use bytes::Bytes;
use futures::future::{self, BoxFuture, FutureExt, TryFutureExt};
use handlebars::Handlebars;
use http::header::HeaderValue;
use hyper::{self, Body, Request, Response};
use serde_json::{self, json};
use url::Url;

use std::collections::BTreeMap;

use shaft::db::SqliteDatabase;
use shaft::github::{HttpError, MockGenericHttpClient};
use shaft::rest::{register_servlets, AppConfig, AppState, AuthenticateUser, MiddlewareLogger};

const SCHEMA: &str = r#"
    CREATE TABLE tokens ( user_id TEXT NOT NULL, token TEXT NOT NULL );
    CREATE TABLE github_users (user_id text primary key not null, github_id text not null);
    CREATE TABLE users ( user_id TEXT NOT NULL UNIQUE, display_name TEXT );
    CREATE TABLE IF NOT EXISTS "transactions" (id integer primary key autoincrement not null, shafter TEXT NOT NULL, shaftee TEXT NOT NULL, amount BIGINT NOT NULL, time_sec BIGINT NOT NULL, reason TEXT NOT NULL);
"#;

fn setup_app(http_client: Option<MockGenericHttpClient>) -> (test::TestServer, AppState) {
    let config = AppConfig {
        github_client_id: "fake_client_id".to_owned(),
        github_client_secret: "fake_client_secret".to_owned(),
        github_state: "fake_state".to_owned(),
        web_root: String::new(),
        required_org: "fake_org".to_owned(),
        resource_dir: "res".to_owned(),
    };

    let database = SqliteDatabase::with_path(":memory:");
    database.run_statements(SCHEMA).unwrap();

    let mock_http_client = http_client.unwrap_or_default();

    let app_state =
        AppState::with_http_client(config, Handlebars::new(), database, mock_http_client);

    let drain = slog::Discard;
    let logger = slog::Logger::root(drain, slog::o!());
    let logger_middleware = MiddlewareLogger::new(logger);

    let state = app_state.clone();
    let srv = test::start(move || {
        let logger_middleware = logger_middleware.clone();

        actix_web::App::new()
            .data(state.clone())
            .app_data(state.clone())
            .wrap(AuthenticateUser::new(state.database.clone()))
            .wrap_fn(move |req, srv| logger_middleware.wrap(req, srv))
            .configure(|config| register_servlets(config, &state))
    });

    (srv, app_state)
}

#[actix_rt::test]
async fn test_health() {
    let (srv, _) = setup_app(None);

    let req = srv.get("/health");
    let mut response = req.send().await.unwrap();
    assert!(response.status().is_success());

    let result = response.body().await.unwrap();
    assert_eq!(result, Bytes::from_static(b"OK"))
}

#[actix_rt::test]
async fn test_initial_redirect() {
    let (srv, _) = setup_app(None);

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
    let (srv, app_state) = setup_app(None);

    // Check that the client gets redirected to the right github page.
    let req = srv.get("/github/login");
    let response = req.send().await.unwrap();
    assert_eq!(response.status(), 302);

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

/// Test the github callback API and that tokens are correctly exchanged.
#[actix_rt::test]
async fn test_github_callback() {
    let mut mock_http_client = MockGenericHttpClient::new();

    mock_http_client
        .expect_request()
        .withf(|req: &Request<Body>| {
            // TODO: Check url and query string.
            req.method() == "POST" && req.uri().path() == "/login/oauth/access_token"
        })
        .returning(
            |_| -> BoxFuture<'static, Result<Response<Body>, HttpError>> {
                future::ready(
                    Response::builder().status(200).body(
                        serde_json::to_string(&json!({
                            "access_token": "fake_token",
                            "scope": "fake_scope",
                        }))
                        .unwrap()
                        .into(),
                    ),
                )
                .map_err(|source| HttpError::Http { source })
                .boxed()
            },
        );

    mock_http_client
        .expect_request()
        .withf(|req: &Request<Body>| {
            // TODO: Check url and query string.
            req.method() == "GET" && req.uri().path() == "/user"
        })
        .returning(
            |_| -> BoxFuture<'static, Result<Response<Body>, HttpError>> {
                future::ready(
                    Response::builder().status(200).body(
                        serde_json::to_string(&json!({
                            "login": "fake_login",
                            "name": "fake_name",
                        }))
                        .unwrap()
                        .into(),
                    ),
                )
                .map_err(|source| HttpError::Http { source })
                .boxed()
            },
        );

    mock_http_client
        .expect_request()
        .withf(|req: &Request<Body>| {
            // TODO: Check url and query string.
            req.method() == "GET" && req.uri().path() == "/user/memberships/orgs/fake_org"
        })
        .returning(
            |_| -> BoxFuture<'static, Result<Response<Body>, HttpError>> {
                future::ready(
                    Response::builder().status(200).body(
                        serde_json::to_string(&json!({
                            "state": "fake_state",
                            "role": "fake_role",
                        }))
                        .unwrap()
                        .into(),
                    ),
                )
                .map_err(|source| HttpError::Http { source })
                .boxed()
            },
        );

    let (srv, _) = setup_app(Some(mock_http_client));

    // Check that the client gets redirected to the right github page.
    let req = srv.get("/github/callback?code=1234&state=fake_state");
    let mut response = req.send().await.unwrap();
    let body = response.body().await.unwrap();

    // We should get redirected back to root.
    assert_eq!(
        response.status(),
        302,
        "Non-302 response: {:?}. body: {}",
        response,
        std::str::from_utf8(&body).expect("valid utf8 response")
    );

    assert_eq!(
        response.headers().get("Location"),
        Some(&HeaderValue::from_static("/"))
    );

    // We should have a set cookie header
    let cookies = response.cookies().expect("cookie");

    assert_eq!(
        cookies.len(),
        1,
        "Found unexpected number of cookies: {:?}",
        cookies
    );

    let first_cookie = &cookies[0];

    assert_eq!(first_cookie.http_only(), Some(true));
    assert_eq!(first_cookie.secure(), Some(true));
    assert_eq!(first_cookie.path(), Some("/"));
    assert_eq!(first_cookie.same_site(), Some(SameSite::Lax));
    assert_eq!(first_cookie.name(), "token");
    assert!(
        first_cookie.value().len() > 10,
        "Token length too small: {}",
        first_cookie.value()
    );

    // If we send a request to `/api/balances` we should get a 200

    let req = srv.get("/api/balances").cookie(first_cookie.clone());
    let mut response = req.send().await.unwrap();
    let body = response.body().await.unwrap();
    assert_eq!(
        response.status(),
        200,
        "Non-200 response: {:?}. body: {}",
        response,
        std::str::from_utf8(&body).expect("valid utf8 response")
    );
}

/// Test the github callback API correctly denies people from the wrong org.
#[actix_rt::test]
async fn test_github_callback_wrong_org() {
    let mut mock_http_client = MockGenericHttpClient::new();

    mock_http_client
        .expect_request()
        .withf(|req: &Request<Body>| {
            // TODO: Check url and query string.
            req.method() == "POST" && req.uri().path() == "/login/oauth/access_token"
        })
        .returning(
            |_| -> BoxFuture<'static, Result<Response<Body>, HttpError>> {
                future::ready(
                    Response::builder().status(200).body(
                        serde_json::to_string(&json!({
                            "access_token": "fake_token",
                            "scope": "fake_scope",
                        }))
                        .unwrap()
                        .into(),
                    ),
                )
                .map_err(|source| HttpError::Http { source })
                .boxed()
            },
        );

    mock_http_client
        .expect_request()
        .withf(|req: &Request<Body>| {
            // TODO: Check url and query string.
            req.method() == "GET" && req.uri().path() == "/user"
        })
        .returning(
            |_| -> BoxFuture<'static, Result<Response<Body>, HttpError>> {
                future::ready(
                    Response::builder().status(200).body(
                        serde_json::to_string(&json!({
                            "login": "fake_login",
                            "name": "fake_name",
                        }))
                        .unwrap()
                        .into(),
                    ),
                )
                .map_err(|source| HttpError::Http { source })
                .boxed()
            },
        );

    mock_http_client
        .expect_request()
        .withf(|req: &Request<Body>| {
            // TODO: Check url and query string.
            req.method() == "GET" && req.uri().path() == "/user/memberships/orgs/fake_org"
        })
        .returning(
            |_| -> BoxFuture<'static, Result<Response<Body>, HttpError>> {
                future::ready(
                    Response::builder()
                        .status(403)
                        .body(serde_json::to_string(&json!({})).unwrap().into()),
                )
                .map_err(|source| HttpError::Http { source })
                .boxed()
            },
        );

    let (srv, _) = setup_app(Some(mock_http_client));

    // Check that the client gets redirected to the right github page.
    let req = srv.get("/github/callback?code=1234&state=fake_state");
    let mut response = req.send().await.unwrap();
    let body = response.body().await.unwrap();

    // We should get redirected back to root.
    assert_eq!(
        response.status(),
        403,
        "Non-403 response: {:?}. body: {}",
        response,
        std::str::from_utf8(&body).expect("valid utf8 response")
    );
}
