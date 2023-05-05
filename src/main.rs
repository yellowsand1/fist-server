use std::env;
use std::time::Duration;
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, post, Responder, web};
use anyhow::Result;
use config::{Config, File};
use log::info;
use fist::errors::WebError;
use fist::fist_core::process_sync_info;
use fist::SETTINGS;

#[post("/fist/core")]
async fn core(request: HttpRequest, sync_info: String) -> Result<impl Responder, WebError> {
    process_sync_info(sync_info, request).await.map_err(WebError)?;
    Ok(HttpResponse::Ok().body("ok"))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let mut config_file_path = env::current_exe()
        .expect("Failed to get current executable path")
        .parent()
        .expect("Failed to get parent directory of the executable")
        .to_path_buf();
    config_file_path.push("config.toml");

    //config initialize
    let mut config = Config::default();
    config.merge(File::from(config_file_path)).expect("config error");
    SETTINGS.write().await.merge(config).expect("config error");

    //log initialize
    env::set_var("RUST_LOG", SETTINGS.read().await.get_string("log_level").unwrap());
    env_logger::init();
    info!("Fist server start to initialize !");

    //actix configure
    let server = HttpServer::new(move || {
        App::new()
            // .app_data(web::Data::new(AppState {
            //     app_name: "fist".to_string(),
            // }))
            .service(core)
    })
        .keep_alive(Duration::from_secs(75))
        .workers(SETTINGS.read().await.get_int("server_workers").unwrap() as usize)
        .bind(("127.0.0.1", SETTINGS.read().await.get_int("server_port").unwrap() as u16))?;
    info!("Fist server start to run on port:{:?}!", SETTINGS.read().await.get_int("server_port").unwrap());

    //actix run
    server
        .run()
        .await
}
