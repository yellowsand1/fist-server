extern crate core;

pub mod model;
pub mod fist_core;
pub mod errors;
pub mod encrypt;

use actix_web::http::StatusCode;
use actix_web::HttpResponse;
use lazy_static::lazy_static;
use tokio::sync::RwLock;
use config::Config;
use log::error;
use anyhow::Error as AnyhowError;
use actix_web::error::Error as ActixError;

lazy_static! {
    pub static ref SETTINGS: RwLock<Config> = RwLock::new(Config::default());
}
