use axum::{extract::DefaultBodyLimit, response::Html, routing::get, Router};
use database::Database;
use tower_http::trace::TraceLayer;
use tracing::info;

use std::{error::Error, net::SocketAddr, path::PathBuf};

mod api;
pub mod database;
mod transcoder;

pub struct Config {
    pub max_image_width: Option<u32>,
    pub max_image_height: Option<u32>,
    pub max_image_size: Option<usize>,
    pub max_memory_usage: Option<u32>,
    pub backend_port: u16,
    pub database_url: String,
    pub image_path : PathBuf,
}

pub async fn run(config: Config) -> Result<(), Box<dyn Error>> {
    let database = get_database(&config).await?;
    let app = get_router(&config, database);
    let listener = get_listener(&config).await?;

    info!("Running server on: 127.0.0.1:{}", config.backend_port);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn get_database(config: &Config) -> Result<Database, Box<dyn Error>> {
    database::Database::new(config).await
}

fn get_router(config: &Config, database: Database) -> Router {
    let body_limit = match config.max_image_size {
        Some(limit) => DefaultBodyLimit::max(limit),
        None => DefaultBodyLimit::disable(),
    };

    Router::new()
        .nest("/api", api::router(&body_limit, database))
        .route("/", get(index))
        .layer(TraceLayer::new_for_http())
}
async fn get_listener(config: &Config) -> tokio::io::Result<tokio::net::TcpListener> {
    let address = SocketAddr::from(([127, 0, 0, 1], config.backend_port));

    tokio::net::TcpListener::bind(address).await
}

//Mainly for testing usage, provides visual gui for uploading file
async fn index() -> Html<&'static str> {
    Html(std::include_str!("../public/index.html"))
}
