use gleam::{Ctx, Server};
use hyper;
use hyper::{Method, StatusCode};
use hyper::header::{Location, SetCookie};
use futures::{Future, IntoFuture, future};
use url::Url;

use github;
use rest::{AppState, AppStateConfig, get_expires_string};


pub fn register_servlets(server: &mut Server) {
    server.add_route(Method::Get, "/github/login", github_login);
    server.add_route(Method::Get, "/github/callback", github_callback);
}


fn github_login(_: Ctx, state: AppStateConfig, _req: ()) -> Result<hyper::Response, ()> {
    let mut gh = Url::parse("https://github.com/login/oauth/authorize").expect("valid url");

    gh.query_pairs_mut()
        .append_pair("client_id", &state.config.github_client_id)
        .append_pair("state", &state.config.github_state)
        .append_pair("scope", "read:org");

    let redirect_url = gh.to_string();

    Ok(
        hyper::Response::new()
            .with_status(StatusCode::Found)
            .with_body(format!("Redirecting to {}\n", &redirect_url))
            .with_header(Location::new(redirect_url)),
    )
}


#[derive(GleamFromRequest)]
struct GithubCallbackRequest {
    param_code: String,
    param_state: String,
}


fn github_callback(
    _: Ctx,
    state: AppState,
    req: GithubCallbackRequest,
) -> Box<Future<Item = hyper::Response, Error = hyper::Response>> {
    if req.param_state != state.config.github_state {
        let res = hyper::Response::new()
            .with_status(StatusCode::BadRequest)
            .with_body(format!("State param mismatch"));
        return Box::new(Err(res).into_future());
    }

    let db = state.db.clone();
    let db2 = state.db.clone();

    let http_client = state.http_client.clone();
    let gh_api = github::GithubApi { http_client };

    let web_root = state.config.web_root.clone();
    let required_org = state.config.required_org.clone();

    let f = gh_api
        .exchange_oauth_code(
            &state.config.github_client_id,
            &state.config.github_client_secret,
            &req.param_code,
        )
        .map_err(|e| format!("{}", e))
        .and_then(move |callback| {
            gh_api.get_authenticated_user(&callback.access_token)
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
                                let f = gh_api.get_if_member_of_org(
                                    &callback.access_token, &required_org
                                )
                                .map_err(|e| format!("{}", e))
                                .and_then(move |opt| {
                                    if opt.is_some() {
                                        future::Either::A(
                                            db.add_user_by_github_id(
                                                github_user_id.clone(),
                                                github_name.unwrap_or(github_user_id)
                                            )
                                                .map_err(|e| format!("{}", e))
                                        )
                                    } else {
                                        future::Either::B(future::err(
                                            "user not in org".into()
                                        ))
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
            hyper::Response::new()
                .with_status(StatusCode::Found)
                .with_header(SetCookie(vec![
                    format!(
                        "token={}; HttpOnly; Secure; Path=/; Expires={}; SameSite=lax",
                        token, get_expires_string(),
                    )
                ]))
                .with_header(Location::new(web_root))
        })
        .map_err(|e| {
            hyper::Response::new()
                .with_status(StatusCode::ServiceUnavailable)
                .with_body(format!("Error: {}", e))
        });

    Box::new(f)
}
