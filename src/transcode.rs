use std::io::Cursor;

use crate::database::Database;
use crate::image_format::ImageFormat;
use image::{DynamicImage, ImageReader};
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
pub struct TranscodeTarget {
    pub image_format: Option<ImageFormat>,
    pub image_width: Option<u32>,
    pub image_height: Option<u32>,
}

pub async fn transcode(image: DynamicImage, settings: TranscodeTarget) -> Vec<u8> {
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

        image
            .write_to(
                &mut cursor,
                settings
                    .image_format
                    .unwrap_or(ImageFormat(image::ImageFormat::Png))
                    .format(),
            )
            .unwrap();

        bytes
    })
    .await
    .unwrap()
}

pub async fn get_image(image_id: Uuid, settings: TranscodeTarget, database: &Database) -> Vec<u8> {
    let database_result = database
        .get_image_location(&image_id, settings.image_format.unwrap_or_default())
        .await;
    match database_result {
        Ok(image_path) => {
            if settings.image_width.is_none() && settings.image_height.is_none() {
                tokio::fs::read(image_path).await.unwrap()
            } else {
                let image = tokio::task::spawn_blocking(move || {
                    ImageReader::open(image_path).unwrap().decode().unwrap()
                })
                .await
                .unwrap();

                transcode(image, settings).await
            }
        }
        Err(crate::database::DatabaseError::NotComputed) => {
            todo!()
        }
        Err(crate::database::DatabaseError::FoundButNotInFormat(image_path)) => {
            let wrong_format_image = tokio::task::spawn_blocking(move || {
                let mut imagereader = ImageReader::open(image_path).unwrap();
                imagereader.no_limits();
                imagereader.decode().unwrap()
            })
            .await
            .unwrap();

            let data = transcode(wrong_format_image, settings).await;
            database
                .save_raw_image(
                    data.clone(),
                    image_id,
                    settings.image_format.unwrap_or_default(),
                )
                .await;
            data
        }
        Err(crate::database::DatabaseError::NotFound) => {
            todo!()
        }
    }
}
