use std::{
    error::Error,
    fmt::{Display, Write}, // Add this line to bring the Write trait into scope
    io::{BufRead, Read, Seek},
    path::{Path, PathBuf},
    sync::Arc,
};

use derive_more::derive::Display;
use sqlx::prelude::*;
use tracing::warn;

use crate::image_format::ImageFormat;
use image::{ImageError, ImageReader};
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::sync::mpsc::{Receiver, Sender};
use uuid::Uuid;

use crate::Config;

#[derive(Debug)]
pub enum GetImageError {
    NotComputed,
    NotFound,
    FoundButNotInFormat(ImagePath),
    InternalServerError(sqlx::Error),
}

#[derive(Debug, Display)]
pub enum SaveImageError {
    InternalServerError(sqlx::Error),
}

impl std::error::Error for SaveImageError {}

pub struct Database {
    pool: PgPool,
    image_location: PathBuf,
    transmitter: Sender<DatabaseMessage>,
}

enum DatabaseMessage {
    Computed(Uuid, ImageFormat),
}

impl Database {
    pub async fn new(config: &Config) -> Result<Database, Box<dyn Error>> {
        let (tx, rx) = tokio::sync::mpsc::channel(1024);
        let pool = PgPoolOptions::new().connect(&config.database_url).await?;
        let receiver_pool = pool.clone();
        tokio::spawn(async move { DatabaseReceiver::compute_message(rx, receiver_pool).await });

        Ok(Database {
            pool,
            image_location: config.image_path.clone(),
            transmitter: tx,
        })
    }

    pub async fn save_image<R>(
        &self,
        imagereader: ImageReader<R>,
        image_format: ImageFormat,
    ) -> Result<Uuid, SaveImageError>
    where
        R: Read + Seek + Send + BufRead + 'static,
    {
        let file_identifier = loop {
            let uid = uuid::Uuid::new_v4();

            if !self
                .file_exists(&uid)
                .await
                .map_err(SaveImageError::InternalServerError)?
            {
                break uid;
            }
        };

        let file_path = ImagePath::new(&self.image_location, &file_identifier, image_format);

        sqlx::query!(
            "INSERT INTO images (image_identifier, image_format) VALUES ($1, $2)",
            file_identifier,
            image_format.to_str()
        )
        .execute(&self.pool)
        .await
        .map_err(SaveImageError::InternalServerError)?;

        let transmitter = self.transmitter.clone();
        tokio::task::spawn_blocking(move || {
            let image = match imagereader.decode() {
                Ok(image) => image,
                Err(e) => {
                    warn!("Could not decode image with ID: {file_identifier} because: {e:?}");
                    panic!("Error decoding image");
                }
            };
            if let Err(e) = image.save_with_format(file_path, image_format.format()) {
                warn!("Could not save image with ID: {file_identifier} because: {e:?}");
            }
            tokio::runtime::Handle::current().block_on(async {
                transmitter
                    .send(DatabaseMessage::Computed(file_identifier, image_format))
                    .await
                    .expect("Could not send message on channel")
            });
        });

        Ok(file_identifier)
    }

    pub async fn save_raw_image(
        &self,
        data: Vec<u8>,
        image_identifier: Uuid,
        image_format: ImageFormat,
    ) -> Result<(), sqlx::Error> {
        let file_path = ImagePath::new(&self.image_location, &image_identifier, image_format);
        let _result = sqlx::query!(
            "INSERT INTO images (image_identifier, image_format) VALUES ($1, $2)",
            image_identifier,
            image_format.to_str()
        )
        .execute(&self.pool)
        .await?;

        let transmitter = self.transmitter.clone();
        tokio::spawn(async move {
            if let Err(e) = tokio::fs::write(file_path, data.as_slice()).await {
                warn!("Could not save raw image: {image_identifier} because : {e:?}")
            }
            transmitter
                .send(DatabaseMessage::Computed(image_identifier, image_format))
                .await
                .expect("Could not send image on channel");
        });

        Ok(())
    }

    pub async fn get_image_location(
        &self,
        file_identifier: &Uuid,
        image_format: ImageFormat,
    ) -> Result<ImagePath, GetImageError> {
        let result = sqlx::query!(
            "SELECT computed, image_format FROM images WHERE image_identifier=$1",
            file_identifier,
        )
        .fetch_all(&self.pool)
        .await;

        match result {
            Ok(record) => {
                if record.is_empty() {
                    Err(GetImageError::NotFound)
                } else if let Some(right_format) = record.iter().find(|image| {
                    ImageFormat::from_str(&image.image_format).expect("Invalid format in database")
                        == image_format
                }) {
                    if right_format.computed {
                        Ok(ImagePath::new(
                            &self.image_location,
                            file_identifier,
                            image_format,
                        ))
                    } else {
                        Err(GetImageError::NotComputed)
                    }
                } else {
                    Err(GetImageError::FoundButNotInFormat(ImagePath::new(
                        &self.image_location,
                        file_identifier,
                        ImageFormat::from_str(&record.first().unwrap().image_format)
                            .expect("invalid thing in database"),
                    )))
                }
            }
            Err(e) => Err(GetImageError::InternalServerError(e)),
        }
    }

    async fn file_exists(&self, image_identifier: &Uuid) -> Result<bool, sqlx::Error> {
        Ok(sqlx::query!(
            "SELECT * FROM images WHERE image_identifier=$1",
            image_identifier
        )
        .fetch_optional(&self.pool)
        .await?
        .is_some())
    }
}

struct DatabaseReceiver();

impl DatabaseReceiver {
    async fn compute_message(mut rx: Receiver<DatabaseMessage>, pool: PgPool) {
        while let Some(message) = rx.recv().await {
            match message {
                DatabaseMessage::Computed(image, image_format) => {
                    tokio::spawn(Self::image_computed(image, image_format, pool.clone()));
                }
            }
        }
    }

    async fn image_computed(image_id: Uuid, file_format: ImageFormat, pool: PgPool) {
        let _ = sqlx::query!(
            "UPDATE images SET computed=true WHERE image_identifier=$1 AND image_format=$2",
            image_id,
            file_format.to_str()
        )
        .execute(&pool)
        .await
        .expect("Thread could not send query to sqlx");
    }
}

#[derive(Debug)]
pub struct ImagePath(PathBuf);

impl ImagePath {
    pub fn new(
        image_folder: &Path,
        image_identifier: &Uuid,
        image_format: ImageFormat,
    ) -> ImagePath {
        let mut location = String::with_capacity(32);
        for byte in image_identifier.as_bytes().iter() {
            write!(location, "{:02x}", byte).unwrap()
        }
        ImagePath(
            image_folder
                .join(location)
                .with_extension(image_format.extension()),
        )
    }
}

impl AsRef<Path> for ImagePath {
    fn as_ref(&self) -> &Path {
        self.0.as_ref()
    }
}
