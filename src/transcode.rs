use std::io::Cursor;

use crate::database::Database;
use crate::image_format::ImageFormat;
use image::{DynamicImage, ImageError, ImageReader};
use uuid::Uuid;

#[derive(Debug)]
pub enum TranscoderError {
    ImageError(ImageError),
    NotComputed,
    NotFound,
    InternalServerError(Box<dyn std::error::Error>),
}

#[derive(Debug, Clone, Copy)]
pub struct TranscodeTarget {
    pub image_format: Option<ImageFormat>,
    pub image_width: Option<u32>,
    pub image_height: Option<u32>,
}

pub async fn transcode(
    image: DynamicImage,
    settings: TranscodeTarget,
) -> Result<Vec<u8>, ImageError> {
    tokio::task::spawn_blocking(move || {
        let image = if settings.image_width.is_some() || settings.image_height.is_some() {
            let width = settings.image_width.unwrap_or(image.width());
            let height = settings.image_height.unwrap_or(image.height());
            image.resize(width, height, image::imageops::FilterType::Lanczos3)
        } else {
            image
        };

        let mut bytes: Vec<u8> = Vec::new();
        let mut cursor = Cursor::new(&mut bytes);

        image.write_to(
            &mut cursor,
            settings
                .image_format
                .unwrap_or(ImageFormat(image::ImageFormat::Png))
                .format(),
        )?;

        Ok(bytes)
    })
    .await
    .expect("Could not join threads")
}

pub async fn get_image(
    image_id: Uuid,
    settings: TranscodeTarget,
    database: &Database,
) -> Result<Vec<u8>, TranscoderError> {
    let database_result = database
        .get_image_location(&image_id, settings.image_format.unwrap_or_default())
        .await;
    match database_result {
        Ok(image_path) => {
            if settings.image_width.is_none() && settings.image_height.is_none() {
                tokio::fs::read(image_path)
                    .await
                    .map_err(|x| TranscoderError::InternalServerError(Box::new(x)))
            } else {
                let image = tokio::task::spawn_blocking(move || {
                    ImageReader::open(image_path)
                        .unwrap()
                        .decode()
                        .expect("file path returned by database was unable to be opened")
                })
                .await
                .unwrap();

                transcode(image, settings)
                    .await
                    .map_err(TranscoderError::ImageError)
            }
        }
        Err(crate::database::GetImageError::NotComputed) => Err(TranscoderError::NotComputed),
        Err(crate::database::GetImageError::FoundButNotInFormat(image_path)) => {
            let wrong_format_image = tokio::task::spawn_blocking(move || {
                let mut imagereader = ImageReader::open(image_path).unwrap();
                imagereader.no_limits();
                imagereader.decode().unwrap()
            })
            .await
            .unwrap();

            let data = transcode(wrong_format_image, settings)
                .await
                .map_err(TranscoderError::ImageError)?;
            database
                .save_raw_image(
                    data.clone(),
                    image_id,
                    settings.image_format.unwrap_or_default(),
                )
                .await
                .map_err(|e| TranscoderError::InternalServerError(Box::new(e)))?;
            Ok(data)
        }
        Err(crate::database::GetImageError::NotFound) => Err(TranscoderError::NotFound),
        Err(crate::database::GetImageError::InternalServerError(e)) => {
            Err(TranscoderError::InternalServerError(Box::new(e)))
        }
    }
}
