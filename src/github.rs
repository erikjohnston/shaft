//! Implements talking to the Github API

use futures::{Future, Stream};
use hyper;
use hyper::{Body, Request, StatusCode};
use serde::de::DeserializeOwned;
use serde_json;
use snafu::ResultExt;
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
    pub fn exchange_oauth_code(
        &self,
        client_id: &str,
        client_secret: &str,
        code: &str,
    ) -> Box<Future<Item = GithubCallbackAuthResponse, Error = HttpError>> {
        let mut gh = Url::parse("https://github.com/login/oauth/access_token").unwrap();

        gh.query_pairs_mut()
            .append_pair("client_id", client_id)
            .append_pair("client_secret", client_secret)
            .append_pair("code", code);

        let mut req = Request::post(gh.to_string());
        req.header(hyper::header::ACCEPT, "application/json");

        let f = parse_resp_as_json(self.http_client.request(req.body(Body::empty()).unwrap()));

        Box::new(f)
    }

    /// Given a user access token from Github get the user's Github ID and
    /// display name.
    pub fn get_authenticated_user(
        &self,
        token: &str,
    ) -> Box<Future<Item = GithubUserResponse, Error = HttpError>> {
        let url = "https://api.github.com/user";

        let mut req = Request::get(url);
        req.header(hyper::header::ACCEPT, "application/json");
        req.header(hyper::header::USER_AGENT, "rust shaft");
        req.header(hyper::header::AUTHORIZATION, format!("token {}", token));

        let f = parse_resp_as_json(self.http_client.request(req.body(Body::empty()).unwrap()));

        Box::new(f)
    }

    /// Check if the Github user with given access token is a member of the org.
    pub fn get_if_member_of_org(
        &self,
        token: &str,
        org: &str,
    ) -> Box<Future<Item = Option<GithubOrganizationMembership>, Error = HttpError>> {
        let url = format!("https://api.github.com/user/memberships/orgs/{}", org);

        let mut req = Request::get(url);
        req.header(hyper::header::ACCEPT, "application/json");
        req.header(hyper::header::USER_AGENT, "rust shaft");
        req.header(hyper::header::AUTHORIZATION, format!("token {}", token));

        let f = parse_resp_as_json(self.http_client.request(req.body(Body::empty()).unwrap()))
            .map(Some)
            .or_else(|err| {
                if let HttpError::Status { code } = err {
                    if code == StatusCode::FORBIDDEN {
                        Ok(None)
                    } else {
                        Err(err)
                    }
                } else {
                    Err(err)
                }
            });

        Box::new(f)
    }
}

/// Parse HTTP response into JSON object.
fn parse_resp_as_json<F, C>(resp: F) -> Box<Future<Item = C, Error = HttpError>>
where
    F: Future<Item = hyper::Response<Body>, Error = hyper::Error> + 'static,
    C: DeserializeOwned + 'static,
{
    let f = resp
        .map_err(|e| HttpError::Http { source: e })
        .and_then(|res| -> Result<_, HttpError> {
            if res.status().is_success() {
                Ok(res)
            } else {
                Err(HttpError::Status { code: res.status() })
            }
        })
        .and_then(|res| {
            // TODO: Limit max amount read
            res.into_body()
                .concat2()
                .map_err(|e| HttpError::Http { source: e })
        })
        .and_then(|vec| -> Result<C, _> {
            let res = serde_json::from_slice(&vec[..]).context(DeserializeError)?;

            Ok(res)
        });

    Box::new(f)
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
