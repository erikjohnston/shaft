//! Handles login flow using Github OAuth.

use actix_web::web::ServiceConfig;
use actix_web::{error, web, Error, HttpResponse};
use futures_util::future::TryFutureExt;
use hyper;
use url::Url;

use crate::github;
use crate::rest::{get_expires_string, AppState};

/// Register servlets with HTTP app
pub fn register_servlets(config: &mut ServiceConfig) {
    config.route("/github/login", web::get().to(github_login));
    config.route("/github/callback", web::get().to(github_callback));
}

/// Handles inbound `/github/login` request to start OAuth flow.
async fn github_login(state: web::Data<AppState>) -> Result<HttpResponse, Error> {
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

/// The Github API request recived at `/github/callback`
#[derive(Deserialize)]
struct GithubCallbackRequest {
    /// Code that can be exchanged for a user token.
    code: String,
    /// A string that we expect to match the configured state string.
    state: String,
}

/// Handles inbound `/github/callback` request from github that includes code we
/// can exchange for a user's access token.
async fn github_callback(
    (query, state): (web::Query<GithubCallbackRequest>, web::Data<AppState>),
) -> Result<HttpResponse, Error> {
    if query.state != state.config.github_state {
        let res = HttpResponse::BadRequest().body("State param mismatch");
        return Ok(res);
    }

    let db = state.database.clone();
    let db2 = state.database.clone();

    let http_client = state.http_client.clone();
    let gh_api = github::GithubApi { http_client };

    let web_root = state.config.web_root.clone();
    let required_org = state.config.required_org.clone();

    let callback = gh_api
        .exchange_oauth_code(
            &state.config.github_client_id,
            &state.config.github_client_secret,
            &query.code,
        )
        .await
        .map_err(error::ErrorServiceUnavailable)?;

    let user = gh_api
        .get_authenticated_user(&callback.access_token)
        .await
        .map_err(error::ErrorInternalServerError)?;

    let github_user_id = user.login.clone();
    let github_name = user.name.clone();

    let user_id_opt = db
        .get_user_by_github_id(user.login)
        .map_err(error::ErrorInternalServerError)
        .await?;

    let user_id = if let Some(user_id) = user_id_opt {
        user_id
    } else {
        let opt = gh_api
            .get_if_member_of_org(&callback.access_token, &required_org)
            .map_err(error::ErrorInternalServerError)
            .await?;

        if opt.is_some() {
            db.add_user_by_github_id(
                github_user_id.clone(),
                github_name.unwrap_or(github_user_id),
            )
            .map_err(error::ErrorInternalServerError)
            .await?
        } else {
            return Err(error::ErrorForbidden("user not in org"));
        }
    };

    let token = db2
        .create_token_for_user(user_id)
        .map_err(error::ErrorInternalServerError)
        .await?;

    Ok(HttpResponse::Found()
        .header(
            hyper::header::SET_COOKIE,
            format!(
                "token={}; HttpOnly; Secure; Path=/; Expires={}; SameSite=lax",
                token,
                get_expires_string(),
            ),
        )
        .header(hyper::header::LOCATION, format!("{}/", web_root))
        .finish())
}
