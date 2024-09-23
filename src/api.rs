use axum::{
    body::Bytes,
    debug_handler,
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::{Response, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
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
        .layer(body_limit.clone())
        .route("/:image_id", get(serve_image))
        .with_state(api_state)
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
            state.database.save_image(image_data).await.unwrap();
            Html("Good job!")
        }
        None => {
            info!("Invalid image format...");
            Html("Something went wrong...")
        }
    }
}

#[debug_handler]
async fn serve_image(
    State(state): State<Arc<ApiState>>,
    Path(image_id): Path<i32>,
) -> impl IntoResponse {
    let image_path = state.database.get_image_location(image_id).await.unwrap();

    let mime_type = mime_guess::from_path(&image_path).first_or_octet_stream();
    let image = tokio::fs::read(&image_path).await.unwrap();

    let bytes = Bytes::from(image);
    let body = axum::body::Body::from(bytes);

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", mime_type.as_ref())
        .body(body)
        .unwrap()
}
