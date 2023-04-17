use actix_web::{HttpResponse, ResponseError};
use anyhow::Error as AnyhowError;
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};
use log::error;

pub struct WebError(pub AnyhowError);

impl Display for WebError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        Display::fmt(&self.0, f)
    }
}

impl Debug for WebError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        Debug::fmt(&self.0, f)
    }
}

impl ResponseError for WebError {
    fn error_response(&self) -> HttpResponse {
        error!("Error: {:?}", self.0);
        HttpResponse::InternalServerError().finish()
    }
}
