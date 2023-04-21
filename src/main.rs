use std::env;
use std::sync::Mutex;
use actix_web::{App, get, HttpRequest, HttpResponse, HttpServer, post, Responder, web};
use config::{Config, File};
use log::{error, info};
use anyhow::Result;
use fist::errors::WebError;
use fist::fist_core::process_sync_info;
use fist::SETTINGS;

#[post("/fist/core")]
async fn post(request: HttpRequest, sync_info: String) -> Result<impl Responder, WebError> {
    process_sync_info(sync_info, request).await.map_err(WebError)?;
    Ok(HttpResponse::Ok())
}

struct AppState {
    app_name: String,
}

struct AppStateWithCounter {
    counter: Mutex<i32>, // <- Mutex is necessary to mutate safely across threads
}

async fn manual_hello() -> impl Responder {
    HttpResponse::Ok().body("hi there!")
}

#[get("/index")]
async fn index(data: web::Data<AppState>) -> String {
    let app_name = &data.app_name; // <- get app_name
    format!("Hello {app_name}!") // <- response with app_name
}

use std::net::SocketAddr;

#[get("/test")]
async fn test(request: HttpRequest) -> Result<impl Responder, WebError> {
    let peer_addr: Option<SocketAddr> = request.peer_addr();
    if let Some(addr) = peer_addr {
        info!("Client IP: {}, Port: {}", addr.ip(), addr.port());
    } else {
        error!("Unable to get client address,{:?}",request);
    }
    Ok(HttpResponse::Ok())
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
            .app_data(web::Data::new(AppState {
                app_name: "fist".to_string(),

            }))
            .service(index)
            .service(post)
            .service(test)
            .route("hey", web::get().to(manual_hello))
    })
        .bind(("127.0.0.1", SETTINGS.read().await.get_int("server_port").unwrap() as u16))?;
    info!("Fist server start to run on port:{:?}!", SETTINGS.read().await.get_int("server_port").unwrap());

    //actix run
    server
        .run()
        .await
}
