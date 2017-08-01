use hyper;
use hyper::{Request, StatusCode, Method};
use hyper::header::{Accept, Authorization, UserAgent};
use futures::{Future, Stream};
use serde::de::DeserializeOwned;
use serde_json;
use url::Url;

use HttpClient;


#[derive(Debug, Clone)]
pub struct GithubApi {
    pub http_client: HttpClient,
}


quick_error!{
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

        let url = gh.to_string().parse().unwrap();

        let mut req = Request::new(Method::Post, url);
        req.headers_mut().set(Accept::json());

        let f = parse_resp_as_json(self.http_client.request(req));

        Box::new(f)
    }

    pub fn get_authenticated_user(
        &self,
        token: &str,
    ) -> Box<Future<Item = GithubUserResponse, Error = HttpError>> {
        let url = "https://api.github.com/user".parse().unwrap();

        let mut req = Request::new(Method::Get, url);
        req.headers_mut().set(Accept::json());
        req.headers_mut().set(UserAgent::new("rust-gleam-shaft"));
        req.headers_mut()
            .set(Authorization(format!("token {}", token)));

        let f = parse_resp_as_json(self.http_client.request(req));

        Box::new(f)
    }

    pub fn get_if_member_of_org(
        &self,
        token: &str,
        org: &str,
    ) -> Box<Future<Item = Option<GithubOrganizationMembership>, Error = HttpError>> {
        let url = format!("https://api.github.com/user/memberships/orgs/{}", org)
            .parse()
            .unwrap();

        let mut req = Request::new(Method::Get, url);
        req.headers_mut().set(Accept::json());
        req.headers_mut().set(UserAgent::new("rust-gleam-shaft"));
        req.headers_mut()
            .set(Authorization(format!("token {}", token)));

        let f = parse_resp_as_json(self.http_client.request(req))
            .map(|org| Some(org))
            .or_else(|err| {
                if let HttpError::Status(status) = err {
                    if status == StatusCode::Forbidden {
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
    F: Future<Item = hyper::Response, Error = hyper::Error> + 'static,
    C: DeserializeOwned + 'static,
{
    let f = resp.from_err()
        .and_then(|res| -> Result<_, HttpError> {
            if res.status().is_success() {
                Ok(res)
            } else {
                Err(res.status().into())
            }
        })
        .and_then(|res| {
            // TODO: Limit max amount read
            res.body().concat2().from_err()
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
