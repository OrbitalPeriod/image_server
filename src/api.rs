use axum::{
    debug_handler,
    extract::{DefaultBodyLimit, Multipart, State},
    response::Html,
    routing::post,
    Router,
};
use image::ImageReader;
use std::{io::Cursor, sync::Arc};
use tracing::info;

use crate::database::Database;

struct ApiState {
    pub database: Database,
}

pub fn router(body_limit: &DefaultBodyLimit, database: Database) -> Router {
    let api_state = Arc::new(ApiState { database });

    Router::new()
        .route("/upload", post(upload))
        .with_state(api_state)
        .layer(body_limit.clone())
}

#[debug_handler]
async fn upload(
    State(state): State<Arc<ApiState>>,
    mut multipart: Multipart,
) -> Html<&'static str> {
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

    let mut reader = ImageReader::new(Cursor::new(file_data));
    reader.no_limits();
    let image_data = reader.with_guessed_format().unwrap();
    match image_data.format() {
        Some(_image) => {
            let _ = state.database.save_image(image_data).await.unwrap();
            Html("Good job!")
        }
        None => {
            info!("Invalid image format...");
            Html("Something went wrong...")
        }
    }
}
