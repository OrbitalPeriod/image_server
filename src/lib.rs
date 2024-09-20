use axum::{
    extract::{DefaultBodyLimit, Multipart},
    response::Html,
    routing::get,
    Router,
};
use image::ImageReader;
use std::{error::Error, net::SocketAddr};
use std::io::Cursor;
use tokio::io::AsyncWriteExt;

pub struct Config {
    pub max_image_width: Option<u32>,
    pub max_image_height: Option<u32>,
    pub max_image_size: Option<usize>,
    pub max_memory_usage: Option<u32>,
    pub backend_port : u16,
}

pub async fn run(config: Config) -> Result<(), Box<dyn Error>> {
    let app = get_router(&config);
    let listener = get_listener(&config).await?;

    axum::serve(listener, app).await?;
    Ok(())
}

fn get_router(config: &Config) -> Router {
    let body_limit = match config.max_image_size{
        Some(limit) => DefaultBodyLimit::max(limit),
        None => DefaultBodyLimit::disable(),
    };

    Router::new().route(
        "/",
        get(index)
            .post(upload)
            .layer(body_limit),
    )
}
async fn get_listener(config: &Config) -> tokio::io::Result<tokio::net::TcpListener> {
    let address = SocketAddr::from(([127,0,0,1], config.backend_port));

    tokio::net::TcpListener::bind(address).await
}

//Mainly for testing usage, provides visual gui for uploading file
async fn index() -> Html<&'static str> {
    Html(std::include_str!("../public/index.html"))
}

async fn upload(mut multipart: Multipart) {
    let mut file_data: Vec<u8> = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .expect("failed to get the next field")
    {
        let data = field.bytes().await.unwrap();
        data.iter().for_each(|byte| file_data.push(*byte));
    }

    let file_data = file_data;
    {
        let mut reader = ImageReader::new(Cursor::new(&file_data));
        reader.no_limits();
        let image_data = reader.with_guessed_format().unwrap();
        if let Some(image) = image_data.format() {
        } else {
        }
    }

    let mut file = tokio::fs::File::create("test.png").await.unwrap();
    match file.write_all(&file_data).await {
        Ok(_) => {}
        Err(e) => {}
    };
}
