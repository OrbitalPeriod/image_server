use axum::{
    extract::{DefaultBodyLimit, Multipart},
    routing::post,
    Router,
};
use image::ImageReader;
use tracing::info;
use std::io::Cursor;
use tokio::io::AsyncWriteExt;

pub fn router(body_limit: &DefaultBodyLimit) -> Router {
    Router::new().route("/upload", post(upload).layer(body_limit.clone()))
}

async fn upload(mut multipart: Multipart) {
    info!("accepted request");
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
        match image_data.format() {
            Some(_image) => {}
            None => {}
        }
    }

    let mut file = tokio::fs::File::create("test.png").await.unwrap();
    match file.write_all(&file_data).await {
        Ok(_) => {}
        Err(e) => {}
    };
}
