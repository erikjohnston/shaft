//! A logging middleware using [slog]

use actix_http::httpmessage::HttpMessage;
use actix_service::Service;
use actix_web::dev::{MessageBody, ServiceRequest, ServiceResponse};
use actix_web::{self, Error};
use futures::{Future, IntoFuture};
use rand::{thread_rng, Rng};
use slog::Logger;

/// A unique ID assigned to each inbound request
pub struct RequestID(pub u32);

/// A middleware that logs proccessed requests usig [slog].
#[derive(Clone)]
pub struct MiddlewareLogger {
    logger: Logger,
}

impl MiddlewareLogger {
    pub fn new(logger: Logger) -> MiddlewareLogger {
        MiddlewareLogger { logger }
    }

    pub fn wrap<B, S>(
        &self,
        req: ServiceRequest,
        srv: &mut S,
    ) -> impl IntoFuture<Item = ServiceResponse<B>, Error = Error>
    where
        B: MessageBody,
        S: Service<Request = ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    {
        let request_id: u32 = thread_rng().gen();
        let logger = self.logger.new(o!(
            "request_id" => request_id,
            "path" => req.path().to_string(),
            "method" => req.method().to_string(),
        ));

        let resp_logger = logger.clone();

        req.extensions_mut().insert(RequestID(request_id));
        req.extensions_mut().insert(logger);

        srv.call(req).then(move |res| {
            match res {
                Ok(ref resp) => {
                    info!(resp_logger, "Processed request"; "status_code" => resp.status().as_u16())
                }
                Err(ref err) => {
                    info!(resp_logger, "Processed request"; "err" => format!("{}", err))
                }
            };
            res
        })
    }
}
