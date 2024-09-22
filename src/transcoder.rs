use std::io::{BufRead, Read, Seek};

use image::ImageReader;
use tracing::{info, warn};

pub async fn transcode<R>(mut rx: tokio::sync::mpsc::Receiver<ImageReader<R>>)
where
    R: Read + Seek + Send + BufRead + 'static,
{
    while let Some(image) = rx.recv().await {
        tokio::task::spawn_blocking(move || match image.decode() {
            Ok(image) => image.save("test.png").unwrap(),
            Err(e) => {
                warn!("Image could not be decoded: {e:?}")
            }
        });
    }
}
