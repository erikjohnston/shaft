use gleam::{Ctx, Server, IntoBody, Params};
use hyper::{Request, Response, StatusCode};
use futures::{Future, Stream};
use serde::de::DeserializeOwned;
use serde_urlencoded;
use serde_json;

use std::ops::Deref;


pub struct FormUrlEncoded<T>(pub T);

impl<'a, T> IntoBody for FormUrlEncoded<T>
    where T: DeserializeOwned + 'static,
{
    type Error = Response;
    type Future = Box<Future<Item=Self, Error=Self::Error>>;

    fn into_body(_: &Server, _: &Ctx, req: Request, _: Params) -> Self::Future {
        // TODO: Check correct data type.

        let f = req.body()
            .concat2()
            .map_err(|e| {
                Response::new()
                    .with_status(StatusCode::InternalServerError)
                    .with_body(format!("Failed to read body: {}", e))
            })
            .and_then(|vec| {
                serde_urlencoded::from_bytes::<T>(&vec)
                    .map_err(|e| {
                        Response::new()
                            .with_status(StatusCode::BadRequest)
                            .with_body(format!("Invalid body: {}", e))
                    })
            })
            .map(|t| FormUrlEncoded(t));

        Box::new(f)
    }
}

impl<T> AsRef<T> for FormUrlEncoded<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T> Deref for FormUrlEncoded<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}


pub struct JsonBody<T>(pub T);

impl<'a, T> IntoBody for JsonBody<T>
    where T: DeserializeOwned + 'static,
{
    type Error = Response;
    type Future = Box<Future<Item=Self, Error=Self::Error>>;

    fn into_body(_: &Server, _: &Ctx, req: Request, _: Params) -> Self::Future {
        // TODO: Check correct data type.

        let f = req.body()
            .concat2()
            .map_err(|e| {
                Response::new()
                    .with_status(StatusCode::InternalServerError)
                    .with_body(format!("Failed to read body: {}", e))
            })
            .and_then(|vec| {
                serde_json::from_slice::<T>(&vec)
                    .map_err(|e| {
                        Response::new()
                            .with_status(StatusCode::BadRequest)
                            .with_body(format!("Invalid body: {}", e))
                    })
            })
            .map(|t| JsonBody(t));

        Box::new(f)
    }
}

impl<T> AsRef<T> for JsonBody<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T> Deref for JsonBody<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}