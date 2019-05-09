//! A logging middleware using [slog]

use actix_web::middleware::{Finished, Middleware, Started};
use actix_web::{error::Result, HttpRequest, HttpResponse};
use rand::{thread_rng, Rng};
use slog::Logger;

use crate::rest::AppState;

/// A unique ID assigned to each inbound request
pub struct RequestID(pub u32);

/// A middleware that logs proccessed requests usig [slog].
pub struct MiddlewareLogger {
    logger: Logger,
}

impl MiddlewareLogger {
    pub fn new(logger: Logger) -> MiddlewareLogger {
        MiddlewareLogger { logger }
    }
}

impl Middleware<AppState> for MiddlewareLogger {
    fn start(&self, req: &HttpRequest<AppState>) -> Result<Started> {
        let request_id: u32 = thread_rng().gen();
        let logger = self.logger.new(o!(
            "request_id" => request_id,
            "path" => req.path().to_string(),
            "method" => req.method().to_string(),
        ));

        req.extensions_mut().insert(RequestID(request_id));
        req.extensions_mut().insert(logger);

        Ok(Started::Done)
    }

    fn finish(&self, req: &HttpRequest<AppState>, resp: &HttpResponse) -> Finished {
        let logger = req
            .extensions()
            .get::<Logger>()
            .expect("no logger installed in request")
            .clone();

        info!(logger, "Processed request"; "status_code" => resp.status().as_u16());

        Finished::Done
    }
}
