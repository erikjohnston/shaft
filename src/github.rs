use futures::{Future, Stream};
use hyper;
use hyper::{Body, Request, StatusCode};
use serde::de::DeserializeOwned;
use serde_json;
use url::Url;

use crate::HttpClient;

#[derive(Debug, Clone)]
pub struct GithubApi {
    pub http_client: HttpClient,
}

quick_error! {
    #[derive(Debug)]
    pub enum HttpError {
        DeserializeError(err: serde_json::Error) {
            from()
        }
        Http(err: hyper::Error) {
            from()
        }
        Status(code: StatusCode) {
            description("Non 2xx response received")
            display("Got response {}", code)
            from()
        }
    }
}

impl GithubApi {
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
                if let HttpError::Status(status) = err {
                    if status == StatusCode::FORBIDDEN {
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

fn parse_resp_as_json<F, C>(resp: F) -> Box<Future<Item = C, Error = HttpError>>
where
    F: Future<Item = hyper::Response<Body>, Error = hyper::Error> + 'static,
    C: DeserializeOwned + 'static,
{
    let f = resp
        .from_err()
        .and_then(|res| -> Result<_, HttpError> {
            if res.status().is_success() {
                Ok(res)
            } else {
                Err(res.status().into())
            }
        })
        .and_then(|res| {
            // TODO: Limit max amount read
            res.into_body().concat2().from_err()
        })
        .and_then(|vec| -> Result<C, _> {
            let res = serde_json::from_slice(&vec[..])?;

            Ok(res)
        });

    Box::new(f)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GithubCallbackAuthResponse {
    pub access_token: String,
    pub scope: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GithubUserResponse {
    pub login: String,
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GithubOrganizationMembership {
    state: String,
    role: String,
}
