use std::{
    error::Error,
    fmt::Write, // Add this line to bring the Write trait into scope
    io::{BufRead, Read, Seek},
    path::PathBuf,
};

use image::{ImageError, ImageReader};
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::sync::mpsc::{Receiver, Sender};

use crate::Config;

pub struct Database {
    pool: PgPool,
    image_location: PathBuf,
    transmitter: Sender<DatabaseMessage>,
}

enum DatabaseMessage {
    Computed(i32),
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

    pub async fn save_image<R>(&self, imagereader: ImageReader<R>) -> Result<(), ImageError>
    where
        R: Read + Seek + Send + BufRead + 'static,
    {
        let file_name = loop {
            let uid = uuid::Uuid::new_v4();

            let mut file_name = String::with_capacity(20);
            for byte in uid.as_bytes().iter() {
                write!(file_name, "{:02x}", byte).unwrap();
            }
            file_name.push_str(".png");

            let image_folder = self.image_location.join(file_name);

            if !image_folder.exists() {
                break image_folder;
            }
        };

        let result = sqlx::query!(
            "INSERT INTO images (file_name) VALUES ($1) RETURNING id",
            file_name.to_str()
        )
        .fetch_one(&self.pool)
        .await
        .unwrap();

        let transmitter = self.transmitter.clone();
        tokio::task::spawn_blocking(move || {
            let image = imagereader.decode().unwrap();
            image.save(file_name).unwrap();
            tokio::runtime::Handle::current().block_on(async {
                transmitter
                    .send(DatabaseMessage::Computed(result.id))
                    .await
                    .unwrap()
            });
        });

        Ok(())
    }
}

struct DatabaseReceiver();

impl DatabaseReceiver {
    async fn compute_message(mut rx: Receiver<DatabaseMessage>, pool: PgPool) {
        while let Some(message) = rx.recv().await {
            match message {
                DatabaseMessage::Computed(image) => {tokio::spawn(Self::image_computed(image, pool.clone()));},
            }
        }
    }

    async fn image_computed(image_id: i32, pool: PgPool) {
        let _ = sqlx::query!("UPDATE images SET computed=true WHERE id=$1", image_id)
            .execute(&pool)
            .await
            .unwrap();
    }
}
