//! Implements talking to the Github API

use bytes::buf::BufExt as _;
use hyper;
use hyper::{Body, Request, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json;
use snafu::{ResultExt, Snafu};
use url::Url;

use crate::HttpClient;

/// Used to talk to the Github API.
///
/// Can safely be cloned.
#[derive(Debug, Clone)]
pub struct GithubApi {
    pub http_client: HttpClient,
}

/// An error occured talking to Github.
#[derive(Debug, Snafu)]
pub enum HttpError {
    /// Failed to parse response as expected JSON object/
    #[snafu(display("Failed to parse JSON response from GitHub: {}", source))]
    DeserializeError { source: serde_json::Error },
    /// HTTP request failed/
    #[snafu(display("Failed to send request to GitHub: {}", source))]
    Http { source: hyper::Error },
    /// Got non-2xx response.
    #[snafu(display("Got non-200 response from GitHub: {}", code))]
    Status { code: StatusCode },
}

impl GithubApi {
    /// Exchange received OAuth code with Github.
    pub async fn exchange_oauth_code(
        &self,
        client_id: &str,
        client_secret: &str,
        code: &str,
    ) -> Result<GithubCallbackAuthResponse, HttpError> {
        let mut gh = Url::parse("https://github.com/login/oauth/access_token").unwrap();

        gh.query_pairs_mut()
            .append_pair("client_id", client_id)
            .append_pair("client_secret", client_secret)
            .append_pair("code", code);

        let req = Request::post(gh.to_string()).header(hyper::header::ACCEPT, "application/json");

        let resp = self
            .http_client
            .request(req.body(Body::empty()).unwrap())
            .await
            .map_err(|e| HttpError::Http { source: e })?;

        Ok(parse_resp_as_json(resp).await?)
    }

    /// Given a user access token from Github get the user's Github ID and
    /// display name.
    pub async fn get_authenticated_user(
        &self,
        token: &str,
    ) -> Result<GithubUserResponse, HttpError> {
        let url = "https://api.github.com/user";

        let req = Request::get(url)
            .header(hyper::header::ACCEPT, "application/json")
            .header(hyper::header::USER_AGENT, "rust shaft")
            .header(hyper::header::AUTHORIZATION, format!("token {}", token));

        let resp = self
            .http_client
            .request(req.body(Body::empty()).unwrap())
            .await
            .map_err(|e| HttpError::Http { source: e })?;

        Ok(parse_resp_as_json(resp).await?)
    }

    /// Check if the Github user with given access token is a member of the org.
    pub async fn get_if_member_of_org(
        &self,
        token: &str,
        org: &str,
    ) -> Result<Option<GithubOrganizationMembership>, HttpError> {
        let url = format!("https://api.github.com/user/memberships/orgs/{}", org);

        let req = Request::get(url)
            .header(hyper::header::ACCEPT, "application/json")
            .header(hyper::header::USER_AGENT, "rust shaft")
            .header(hyper::header::AUTHORIZATION, format!("token {}", token));

        let resp = self
            .http_client
            .request(req.body(Body::empty()).unwrap())
            .await
            .map_err(|e| HttpError::Http { source: e })?;

        match parse_resp_as_json(resp).await {
            Ok(r) => Ok(Some(r)),
            Err(HttpError::Status { code }) if code == StatusCode::FORBIDDEN => Ok(None),
            Err(err) => Err(err),
        }
    }
}

/// Parse HTTP response into JSON object.
async fn parse_resp_as_json<C>(resp: hyper::Response<Body>) -> Result<C, HttpError>
where
    C: DeserializeOwned + 'static,
{
    if !resp.status().is_success() {
        return Err(HttpError::Status {
            code: resp.status(),
        });
    }

    let body = hyper::body::aggregate(resp)
        .await
        .map_err(|e| HttpError::Http { source: e })?;

    let res = serde_json::from_reader(body.reader()).context(DeserializeError)?;

    Ok(res)
}

/// Github API response to `/login/oauth/access_token`
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GithubCallbackAuthResponse {
    /// An access token for the user we're authed against.
    pub access_token: String,
    /// The permissions scope the token has.
    pub scope: String,
}

/// Github API repsonse to `/user`
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GithubUserResponse {
    /// The user's Github login ID
    pub login: String,
    /// The user's Github display name (if any)
    pub name: Option<String>,
}

/// Github API response to `/user/memberships/orgs/{org}`
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GithubOrganizationMembership {
    /// The user's membership state in the org
    state: String,
    /// The user's role in the org
    role: String,
}
