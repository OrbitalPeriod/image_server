use std::{
    error::Error, fmt::Write, io::{BufRead, Read, Seek}, ops::Deref, path::{Path, PathBuf}, sync::Arc
};

use chrono::{DateTime, Duration, Utc};
use derive_more::derive::Display;
use tracing::{debug, instrument, span, trace_span, warn};

use crate::image_format::ImageFormat;
use image::ImageReader;
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

#[derive(Debug)]
pub struct Database {
    pool: PgPool,
    image_location: PathBuf,
    transmitter: Sender<DatabaseMessage>,
    image_ttl_allowed : Option<Duration>,
}

enum DatabaseMessage {
    Computed(Uuid, ImageFormat),
    CleanExpired,
}

impl Database {
    #[instrument]
    pub async fn new(config: &Config) -> Result<Database, Box<dyn Error>> {
        let (transmitter, rx) = tokio::sync::mpsc::channel(1024);
        let pool = PgPoolOptions::new().connect(&config.database_url).await?;
        let receiver_pool = pool.clone();
        let image_path = config.image_path.clone();
        tokio::spawn(DatabaseReceiver::compute_message(
            rx,
            receiver_pool,
            image_path,
        ));

        Ok(Database {
            pool,
            image_location: config.image_path.clone(),
            transmitter,
            image_ttl_allowed: config.image_ttl
        })
    }

    #[instrument(skip(imagereader))]
    pub async fn save_image<R>(
        &self,
        imagereader: ImageReader<R>,
        image_format: ImageFormat,
        api_ttl: Option<Duration>,
    ) -> Result<Uuid, SaveImageError>
    where
        R: Read + Seek + Send + BufRead + 'static,
    {
        let image_eol = Self::determine_eol(api_ttl, self.image_ttl_allowed);

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
            "INSERT INTO images (image_identifier, image_format, expires_at) VALUES ($1, $2, $3)",
            file_identifier,
            image_format.to_str(),
            image_eol.expect("TODO: OPTIONAL TTLS NOT YET IMPLEMENTED")
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

    #[instrument]
    pub async fn save_raw_image(
        &self,
        data: Box<[u8]>,
        image_identifier: Uuid,
        image_format: ImageFormat,
        api_ttl: Option<Duration>,
    ) -> Result<(), sqlx::Error> {
        let data = Arc::new(data);

        let image_eol = Self::determine_eol(api_ttl, self.image_ttl_allowed);

        let file_path = ImagePath::new(&self.image_location, &image_identifier, image_format);
        let _result = sqlx::query!(
            "INSERT INTO images (image_identifier, image_format, expires_at) VALUES ($1, $2, $3)",
            image_identifier,
            image_format.to_str(),
            image_eol.expect("TODO: OPTIONAL TTL NOT YET IMPLEMENTED")
        )
        .execute(&self.pool)
        .await?;

        let transmitter = self.transmitter.clone();
        let data = Arc::clone(&data);
        tokio::spawn(async move {
            if let Err(e) = tokio::fs::write(file_path, &*data).await {
                warn!("Could not save raw image: {image_identifier} because : {e:?}")
            }
            transmitter
                .send(DatabaseMessage::Computed(image_identifier, image_format))
                .await
                .expect("Could not send image on channel");
        });

        Ok(())
    }

    #[instrument]
    pub async fn get_image_location(
        &self,
        file_identifier: &Uuid,
        image_format: ImageFormat,
        max_time: &DateTime<Utc>,
    ) -> Result<ImagePath, GetImageError> {
        let result = sqlx::query!(
            "SELECT computed, image_format, expires_at FROM images WHERE image_identifier=$1",
            file_identifier,
        )
        .fetch_all(&self.pool)
        .await;

        match result {
            Ok(record) => {
                if record.iter().any(|image| &image.expires_at < max_time) {
                    println!("owo");
                    if let Err(e) = self.transmitter.send(DatabaseMessage::CleanExpired).await {
                        warn!("Could not send to transmitter: {e:?}");
                    }
                }

                let active: Vec<(bool, DateTime<Utc>, ImageFormat)> = record
                    .into_iter()
                    .map(|image| {
                        (
                            image.computed,
                            image.expires_at,
                            ImageFormat::from_str(&image.image_format)
                                .expect("invalid image format in db"),
                        )
                    })
                    .filter(|(_, expires_at, _)| expires_at > max_time)
                    .collect();
                if active.is_empty() {
                    Err(GetImageError::NotFound)
                } else if let Some((computed, _, _)) =
                    active.iter().find(|(_, _, format)| &image_format == format)
                {
                    if *computed {
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
                        active.first().unwrap().2,
                    )))
                }
            }
            Err(e) => Err(GetImageError::InternalServerError(e)),
        }
    }

    #[instrument]
    async fn file_exists(&self, image_identifier: &Uuid) -> Result<bool, sqlx::Error> {
        Ok(sqlx::query!(
            "SELECT * FROM images WHERE image_identifier=$1",
            image_identifier
        )
        .fetch_optional(&self.pool)
        .await?
        .is_some())
    }

    fn determine_eol(requested : Option<Duration>, max : Option<Duration>) -> Option<DateTime<Utc>>{
        if requested.is_none(){
            max.map(|x| Utc::now() + x)
        }else if let (Some(requested), Some(max)) = (requested, max){
            if requested > max{
                Some(Utc::now() + max)
            }else{
                Some(Utc::now() + requested)
            }
        }else{
            requested.map(|x| Utc::now() + x)
        }
    }
}

struct DatabaseReceiver();

impl DatabaseReceiver {
    #[instrument]
    async fn compute_message(
        mut rx: Receiver<DatabaseMessage>,
        pool: PgPool,
        image_folder: PathBuf,
    ) {
        while let Some(message) = rx.recv().await {
            match message {
                DatabaseMessage::Computed(image, image_format) => {
                    tokio::spawn(Self::image_computed(image, image_format, pool.clone()));
                }
                DatabaseMessage::CleanExpired => {
                    tokio::spawn(Self::clean_expired(pool.clone(), image_folder.clone()));
                }
            }
        }
    }

    #[instrument]
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

    #[instrument]
    async fn clean_expired(pool: PgPool, image_folder: PathBuf) {
        debug!("Deleting expired images");
        let expired = sqlx::query!(
            "DELETE FROM images WHERE expires_at < $1 AND computed = True RETURNING image_identifier, image_format",
            Utc::now()
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        for image in expired {
            let format = ImageFormat::from_str(&image.image_format)
                .expect("INVALID IMAGE FORMAT IN DATABASE");
            let file_path = ImagePath::new(&image_folder, &image.image_identifier, format);
            if let Err(e) = tokio::fs::remove_file(file_path).await {
                warn!("Something went wrong deleting expired image: {e:?}");
            }
        }
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
