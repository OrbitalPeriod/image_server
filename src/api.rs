use crate::{
    image_format::ImageFormat,
    transcode::{self, TranscoderError},
};
use axum::{
    body::Bytes,
    debug_handler,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{Response, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use chrono::Duration;
use image::ImageReader;
use serde::{de, Deserialize, Deserializer};
use std::{io::{BufReader, Cursor}, str::FromStr, sync::Arc};
use tracing::{info, info_span, warn, trace};
use uuid::Uuid;

use crate::{database::Database, transcode::TranscodeTarget};

struct ApiState {
    pub database: Database,
}


pub fn router(body_limit: &DefaultBodyLimit, database: Database) -> Router {
    let api_state = Arc::new(ApiState {
        database,
    });

    Router::new()
        .route("/upload", post(upload))
        .layer(body_limit.clone())
        .route("/:image_id", get(serve_image))
        .with_state(api_state)
}

#[derive(Deserialize)]
struct UploadSettings{
    ttl_secs : Option<i64>,
}

#[debug_handler]
async fn upload(
    State(state): State<Arc<ApiState>>,
    Query(uploadsettings): Query<UploadSettings>,
    mut multipart: Multipart,
) -> Html<String> {
    let ttl = uploadsettings.ttl_secs.map(Duration::seconds);

    trace!("uploading image");
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
    trace!("done uploading image");

    let mut reader = ImageReader::new(BufReader::new(Cursor::new(file_data)));
    reader.no_limits();
    let image_data = reader.with_guessed_format().unwrap();
    match image_data.format() {
        Some(_image) => {
            match state
                .database
                .save_image(
                    image_data,
                    ImageFormat::PNG,
                    ttl,
                ) //TODO: This is not at 2 minutes, make it variable with config.
                .await
            {
                Ok(uuid) => Html(format!("Good job! file has uuid: {:?}", uuid)),
                Err(e) => {
                    warn!("Error trying to save new image to database: {e:?}");
                    Html("Internal server error...".to_string())
                }
            }
        }
        None => {
            info!("Invalid image format...");
            Html("Invalid image format...".into())
        }
    }
}

#[derive(Deserialize, Clone, Copy)]
struct ImageSettings {
    #[serde(default, deserialize_with = "empty_string_as_none_image_format")]
    pub format: Option<ImageFormat>,
    #[serde(default, deserialize_with = "empty_string_as_none_u32")]
    pub width: Option<u32>,
    #[serde(default, deserialize_with = "empty_string_as_none_u32")]
    pub height: Option<u32>,
}

impl From<ImageSettings> for TranscodeTarget {
    fn from(val: ImageSettings) -> Self {
        TranscodeTarget {
            image_format: val.format,
            image_width: val.width,
            image_height: val.height,
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
        Some("png") => Ok(Some(ImageFormat::PNG)),
        Some("jpg") | Some("jpeg") => Ok(Some(ImageFormat::JPG)),
        Some("webp") => Ok(Some(ImageFormat::WEBP)),
        Some("hdr") => Ok(Some(ImageFormat::HDR)),
        Some("avif") => Ok(Some(ImageFormat::AVIF)),
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
    let uuid = match Uuid::from_str(&image_identifier) {
        Ok(uuid) => uuid,
        Err(_) => return build_response(StatusCode::BAD_REQUEST, "Invalid image id".into()),
    };

    let span = info_span!("serve_image", %uuid);
    let _guard = span.enter();

    let image = match transcode::get_image(uuid, query.into(), &state.database, None).await {
        Ok(image) => image,
        Err(TranscoderError::ImageError(e)) => {
            warn!("Image could not be computed: {e:?}");
            return build_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "ERROR TRANSCODING IMAGES".into(),
            );
        }
        Err(TranscoderError::NotComputed) => {
            return build_response(StatusCode::NOT_FOUND, "Image not yet computed".into());
        }
        Err(TranscoderError::NotFound) => {
            return build_response(StatusCode::NOT_FOUND, "Image not found".into());
        }
        Err(TranscoderError::InternalServerError(e)) => {
            warn!("Something went wrong trying to get an image: {e:?}");
            return build_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL SERVER ERROR".into(),
            );
        }
    };
    let mime_format = query.format.unwrap_or(ImageFormat::PNG);

    let bytes = Bytes::from(image);
    let body = axum::body::Body::from(bytes);

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", mime_format.to_mime_type())
        .body(body)
        .unwrap()
}

fn build_response(status: StatusCode, message: &'static str) -> Response<axum::body::Body> {
    let body = axum::body::Body::from(Bytes::from(message));
    Response::builder().status(status).body(body).unwrap()
}
