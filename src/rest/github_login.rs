use actix_web::{App, Error, HttpRequest, HttpResponse, Query, State};
use futures::{future, Future, IntoFuture};
use hyper;
use hyper::Method;
use url::Url;

use crate::github;
use crate::rest::{get_expires_string, AppState};

pub fn register_servlets(app: App<AppState>) -> App<AppState> {
    app.resource("/github/login", |r| r.method(Method::GET).f(github_login))
        .resource("/github/callback", |r| {
            r.method(Method::GET).with(github_callback)
        })
}

fn github_login(req: &HttpRequest<AppState>) -> Result<HttpResponse, Error> {
    let state = req.state();

    let mut gh = Url::parse("https://github.com/login/oauth/authorize").expect("valid url");

    gh.query_pairs_mut()
        .append_pair("client_id", &state.config.github_client_id)
        .append_pair("state", &state.config.github_state)
        .append_pair("scope", "read:org");

    let redirect_url = gh.to_string();

    Ok(HttpResponse::Found()
        .header(hyper::header::LOCATION, redirect_url.clone())
        .body(format!("Redirecting to {}\n", &redirect_url)))
}

#[derive(Deserialize)]
struct GithubCallbackRequest {
    code: String,
    state: String,
}

fn github_callback(
    (query, state): (Query<GithubCallbackRequest>, State<AppState>),
) -> Box<Future<Item = HttpResponse, Error = Error>> {
    if query.state != state.config.github_state {
        let res = HttpResponse::BadRequest().body("State param mismatch");
        return Box::new(Ok(res).into_future());
    }

    let db = state.database.clone();
    let db2 = state.database.clone();

    let http_client = state.http_client.clone();
    let gh_api = github::GithubApi { http_client };

    let web_root = state.config.web_root.clone();
    let required_org = state.config.required_org.clone();

    let f = gh_api
        .exchange_oauth_code(
            &state.config.github_client_id,
            &state.config.github_client_secret,
            &query.code,
        )
        .map_err(|e| format!("{}", e))
        .and_then(move |callback| {
            gh_api
                .get_authenticated_user(&callback.access_token)
                .map_err(|e| format!("{}", e))
                .and_then(move |user| {
                    let github_user_id = user.login.clone();
                    let github_name = user.name.clone();

                    db.get_user_by_github_id(user.login)
                        .map_err(|e| format!("{}", e))
                        .and_then(move |user_id_opt| {
                            if let Some(user_id) = user_id_opt {
                                future::Either::A(Ok(user_id).into_future())
                            } else {
                                let f = gh_api
                                    .get_if_member_of_org(&callback.access_token, &required_org)
                                    .map_err(|e| format!("{}", e))
                                    .and_then(move |opt| {
                                        if opt.is_some() {
                                            future::Either::A(
                                                db.add_user_by_github_id(
                                                    github_user_id.clone(),
                                                    github_name.unwrap_or(github_user_id),
                                                )
                                                .map_err(|e| format!("{}", e)),
                                            )
                                        } else {
                                            future::Either::B(future::err("user not in org".into()))
                                        }
                                    });

                                future::Either::B(f)
                            }
                        })
                })
        })
        .and_then(move |user_id| {
            db2.create_token_for_user(user_id)
                .map_err(|e| format!("{}", e))
        })
        .map(|token| {
            HttpResponse::Found()
                .header(
                    hyper::header::SET_COOKIE,
                    format!(
                        "token={}; HttpOnly; Secure; Path=/; Expires={}; SameSite=lax",
                        token,
                        get_expires_string(),
                    ),
                )
                .header(hyper::header::LOCATION, web_root)
                .finish()
        })
        .or_else(|e| Ok(HttpResponse::ServiceUnavailable().body(format!("Error: {}", e))));

    Box::new(f)
}
