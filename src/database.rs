use std::{
    error::Error,
    fmt::Write, // Add this line to bring the Write trait into scope
    io::{BufRead, Read, Seek},
    path::{Path, PathBuf}, sync::Arc,
};

use sqlx::prelude::*;

use crate::image_format::ImageFormat;
use image::{ImageError, ImageReader};
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::sync::mpsc::{Receiver, Sender};
use uuid::Uuid;

use crate::Config;

pub struct Database {
    pool: PgPool,
    image_location: PathBuf,
    transmitter: Sender<DatabaseMessage>,
}

enum DatabaseMessage {
    Computed(Uuid, ImageFormat),
}

#[derive(Debug)]
pub enum DatabaseError {
    NotComputed,
    NotFound,
    FoundButNotInFormat(ImagePath),
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
    ) -> Result<Uuid, ImageError>
    where
        R: Read + Seek + Send + BufRead + 'static,
    {
        let file_identifier = loop {
            let uid = uuid::Uuid::new_v4();

            if !self.file_exists(&uid).await {
                break uid;
            }
        };

        let file_path = ImagePath::new(&self.image_location, &file_identifier, image_format);

        let result = sqlx::query!(
            "INSERT INTO images (image_identifier, image_format) VALUES ($1, $2)",
            file_identifier,
            image_format.to_str()
        )
        .execute(&self.pool)
        .await
        .unwrap();

        let transmitter = self.transmitter.clone();
        tokio::task::spawn_blocking(move || {
            let image = imagereader.decode().unwrap();
            image.save_with_format(file_path, *image_format).unwrap();
            tokio::runtime::Handle::current().block_on(async {
                transmitter
                    .send(DatabaseMessage::Computed(file_identifier, image_format))
                    .await
                    .unwrap()
            });
        });

        Ok(file_identifier)
    }

    pub async fn save_raw_image(&self, data : Vec<u8>, image_identifier: Uuid, image_format: ImageFormat){
        let file_path = ImagePath::new(&self.image_location, &image_identifier, image_format);
        let _result = sqlx::query!(
            "INSERT INTO images (image_identifier, image_format) VALUES ($1, $2)",
            image_identifier,
            image_format.to_str()
        )
        .execute(&self.pool)
        .await
        .unwrap();

        let transmitter = self.transmitter.clone();
        tokio::spawn(async move {
            tokio::fs::write(file_path, data.as_slice()).await.unwrap();
            transmitter.send(DatabaseMessage::Computed(image_identifier, image_format)).await.unwrap()
        });
    }

    pub async fn get_image_location(
        &self,
        file_identifier: &Uuid,
        image_format: ImageFormat,
    ) -> Result<ImagePath, DatabaseError> {
        let result = sqlx::query!(
            "SELECT computed, image_format FROM images WHERE image_identifier=$1",
            file_identifier,
        )
        .fetch_all(&self.pool)
        .await;

        match result {
            Ok(record) => {
                let matching_record = record
                    .iter()
                    .filter(|image| {
                        ImageFormat::from_str(&image.image_format)
                            .expect("invalid format in database")
                            == image_format
                    })
                    .take(1)
                    .last();
                match matching_record {
                    Some(matching) => {
                        if matching.computed {
                            Ok(ImagePath::new(
                                &self.image_location,
                                file_identifier,
                                image_format,
                            ))
                        } else {
                            Err(DatabaseError::NotComputed)
                        }
                    }
                    None => Err(DatabaseError::FoundButNotInFormat(ImagePath::new(
                        &self.image_location,
                        file_identifier,
                        ImageFormat::from_str(&record.first().unwrap().image_format)
                            .expect("invalid thing in database"),
                    ))),
                }
            }
            Err(sqlx::Error::RowNotFound) => Err(DatabaseError::NotFound),
            Err(e) => panic!("{e:?}"),
        }
    }
    async fn file_exists(&self, image_identifier: &Uuid) -> bool {
        sqlx::query!(
            "SELECT * FROM images WHERE image_identifier=$1",
            image_identifier
        )
        .fetch_optional(&self.pool)
        .await
        .unwrap()
        .is_some()
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

    async fn image_computed(image_id: Uuid, file_format : ImageFormat, pool: PgPool) {
        let _ = sqlx::query!(
            "UPDATE images SET computed=true WHERE image_identifier=$1 AND image_format=$2",
            image_id,
            file_format.to_str()
        )
        .execute(&pool)
        .await
        .unwrap();
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
