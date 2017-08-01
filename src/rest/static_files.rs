use gleam::{Ctx, Server};
use hyper::{self, Method, Response};
use hyper::header::ContentLength;
use futures::Future;
use futures::future;

use std::path::Path;
use std::fs::File;
use std::io::Read;

use rest::{AppState, InternalServerError, HttpError, NotFound};


pub fn register_servlets(server: &mut Server) {
    server.add_route(Method::Get, "/static/*", render_static);
}

#[derive(GleamFromRequest)]
struct StaticRequest {
    path: String,
}

fn render_static(_: Ctx, state: AppState, req: StaticRequest)
    -> Box<Future<Item = Response, Error = HttpError>>
{
    if !req.path.starts_with("/static/") {
        return future::err(
            InternalServerError("Invalid static path".into()).into()
        ).boxed();
    }

    let fs_path = format!("resources{}", req.path);

    if fs_path.contains("./") || fs_path.contains("../") {
        return future::err(NotFound.into()).boxed();
    }

    if Path::new(&fs_path).is_file() {
        return state.cpu_pool.spawn_fn(move || {
            let mut f = File::open(&fs_path).unwrap();

            let mut source = Vec::new();
            f.read_to_end(&mut source).unwrap();

            Ok(
                Response::new()
                    .with_header(ContentLength(source.len() as u64))
                    .with_body(source)
            )
        }).boxed()
    } else {
        return future::err(NotFound.into()).boxed();
    }
}
