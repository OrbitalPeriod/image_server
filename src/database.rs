use std::{
    error::Error,
    fmt::Write, // Add this line to bring the Write trait into scope
    io::{BufRead, Read, Seek},
    path::{Path, PathBuf},
    str::FromStr,
};

use image::{ImageError, ImageReader};
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::sync::mpsc::{Receiver, Sender};
use tracing::debug;
use uuid::Uuid;

use crate::Config;

pub struct Database {
    pool: PgPool,
    image_location: PathBuf,
    transmitter: Sender<DatabaseMessage>,
}

enum DatabaseMessage {
    Computed(Uuid),
}

#[derive(Debug)]
pub enum DatabaseError {
    NotComputed,
    NotFound,
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

    pub async fn save_image<R>(&self, imagereader: ImageReader<R>) -> Result<Uuid, ImageError>
    where
        R: Read + Seek + Send + BufRead + 'static,
    {
        let file_identifier = loop {
            let uid = uuid::Uuid::new_v4();

            if !self.file_exists(&uid).await {
                break uid;
            }
        };

        let file_path = ImagePath::new(&self.image_location, &file_identifier);

        let result = sqlx::query!(
            "INSERT INTO images (image_identifier) VALUES ($1)",
            file_identifier
        )
        .execute(&self.pool)
        .await
        .unwrap();

        let transmitter = self.transmitter.clone();
        tokio::task::spawn_blocking(move || {
            let image = imagereader.decode().unwrap();
            image.save(file_path).unwrap();
            tokio::runtime::Handle::current().block_on(async {
                transmitter
                    .send(DatabaseMessage::Computed(file_identifier))
                    .await
                    .unwrap()
            });
        });

        Ok(file_identifier)
    }
    pub async fn get_image_location(
        &self,
        file_identifier: &Uuid,
    ) -> Result<ImagePath, DatabaseError> {
        let result = sqlx::query!(
            "SELECT computed FROM images WHERE image_identifier=$1",
            file_identifier
        )
        .fetch_one(&self.pool)
        .await;

        match result {
            Ok(record) => {
                if record.computed {
                    Ok(ImagePath::new(&self.image_location, file_identifier))
                } else {
                    Err(DatabaseError::NotComputed)
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
                DatabaseMessage::Computed(image) => {
                    tokio::spawn(Self::image_computed(image, pool.clone()));
                }
            }
        }
    }

    async fn image_computed(image_id: Uuid, pool: PgPool) {
        let _ = sqlx::query!("UPDATE images SET computed=true WHERE image_identifier=$1", image_id)
            .execute(&pool)
            .await
            .unwrap();
    }
}

pub struct ImagePath(PathBuf);

impl ImagePath {
    pub fn new(image_folder: &Path, image_identifier: &Uuid) -> ImagePath {
        let mut location = String::with_capacity(32);
        for byte in image_identifier.as_bytes().iter() {
            write!(location, "{:02x}", byte).unwrap()
        }
        ImagePath(image_folder.join(location).with_extension("png"))
    }
}

impl AsRef<Path> for ImagePath {
    fn as_ref(&self) -> &Path {
        self.0.as_ref()
    }
}
