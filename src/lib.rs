#[macro_use]
extern crate slog;

use hyper_tls::HttpsConnector;

/// Short hand for our HTTPS enabled outbound HTTP client.
type HttpClient = hyper::Client<HttpsConnector<hyper::client::HttpConnector>>;

pub mod db;
mod error;
mod github;
pub mod rest;
mod settings;
