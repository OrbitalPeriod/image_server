use axum::{
    body::Bytes,
    debug_handler,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{Response, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use image::{ImageFormat, ImageReader};
use serde::{de, Deserialize, Deserializer};
use std::{io::Cursor, str::FromStr, sync::Arc};
use tracing::info;
use uuid::Uuid;

use crate::{
    database::Database,
    transcode::{transcode, TranscodeTarget},
};

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
async fn upload(State(state): State<Arc<ApiState>>, mut multipart: Multipart) -> Html<String> {
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
            let uuid = state.database.save_image(image_data).await.unwrap();
            Html(format!("Good job! file has uuid: {:?}", uuid))
        }
        None => {
            info!("Invalid image format...");
            Html("Something went wrong...".into())
        }
    }
}

#[derive(Deserialize)]
struct ImageSettings {
    #[serde(default, deserialize_with = "empty_string_as_none_image_format")]
    pub format: Option<ImageFormat>,
    #[serde(default, deserialize_with = "empty_string_as_none_u32")]
    pub width: Option<u32>,
    #[serde(default, deserialize_with = "empty_string_as_none_u32")]
    pub height: Option<u32>,
}

impl Into<TranscodeTarget> for ImageSettings {
    fn into(self) -> TranscodeTarget {
        TranscodeTarget {
            image_format: self.format,
            image_width: self.width,
            image_height: self.height,
        }
    }
}

fn empty_string_as_none_image_format<'de, D>(de: D) -> Result<Option<ImageFormat>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(de)?;
    match opt.as_deref() {
        None | Some("") => Ok(None),
        Some("png") => Ok(Some(ImageFormat::Png)),
        Some("jpg") | Some("jpeg") => Ok(Some(ImageFormat::Jpeg)),
        Some("webp") => Ok(Some(ImageFormat::WebP)),
        Some("hdr") => Ok(Some(ImageFormat::Hdr)),
        Some("avif") => Ok(Some(ImageFormat::Avif)),
        Some(other) => Err(de::Error::custom(format!(
            "unsupported image format: {}",
            other
        ))),
    }
}

fn empty_string_as_none_u32<'de, D>(de: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(de)?;
    match opt.as_deref() {
        None | Some("") => Ok(None),
        Some(s) => u32::from_str(s).map_err(de::Error::custom).map(Some),
    }
}

#[debug_handler]
async fn serve_image(
    State(state): State<Arc<ApiState>>,
    Path(image_identifier): Path<String>,
    Query(query): Query<ImageSettings>,
) -> impl IntoResponse {
    let uuid = Uuid::from_str(&image_identifier).unwrap();
    let image_path = state.database.get_image_location(&uuid).await.unwrap();
    let mime_format = query.format.unwrap_or(ImageFormat::Png);

    let image = if query.width.is_some() || query.height.is_some() || query.format.is_some() {
        let mut image = ImageReader::open(&image_path).unwrap();
        image.no_limits();
        let image_data = image.decode().unwrap();
        transcode(image_data, query.into()).await
    } else {
        tokio::fs::read(&image_path).await.unwrap()
    };

    let bytes = Bytes::from(image);
    let body = axum::body::Body::from(bytes);

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", mime_format.to_mime_type())
        .body(body)
        .unwrap()
}
