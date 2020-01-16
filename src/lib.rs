#[macro_use]
extern crate slog;

use hyper_tls::HttpsConnector;

/// Short hand for our HTTPS enabled outbound HTTP client.
type HttpClient = hyper::Client<HttpsConnector<hyper::client::HttpConnector>>;

pub mod db;
pub mod error;
pub mod github;
pub mod rest;
pub mod settings;
