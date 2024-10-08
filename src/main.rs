use std::{path::PathBuf, str::FromStr};

use chrono::Duration;
use image_server::Config;
use tracing::warn;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() {
    let subscriber = if cfg!(debug_assertions){
        FmtSubscriber::builder().with_max_level(tracing::Level::DEBUG).finish()
    }else{
        FmtSubscriber::builder().with_max_level(tracing::Level::INFO).finish()
    };

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let _ = dotenv::dotenv();

    let config = get_config();

    if let Err(e) = image_server::run(config).await {
        warn!("Some error occured running the server: {e:?}");
    }
}

fn get_config() -> Config {
    use std::env;

    let max_image_width = env::var("MAX_IMAGE_WIDTH")
        .map(|string| {
            string
                .parse::<u32>()
                .expect("invalid format of 'max_image_width, please provide u32'")
        })
        .ok();
    let max_image_height = env::var("MAX_IMAGE_HEIGHT")
        .map(|string| {
            string
                .parse::<u32>()
                .expect("invalid format of 'max_image_height, please provide u32'")
        })
        .ok();
    let max_image_size = env::var("MAX_IMAGE_SIZE")
        .map(|string| {
            string
                .parse::<usize>()
                .expect("invalid format of 'max_image_size, please provide u32'")
        })
        .ok();
    let max_memory_usage = env::var("MAX_MEMORY_USAGE")
        .map(|string| {
            string
                .parse::<u32>()
                .expect("invalid format of 'max_memory_usage, please provide u32'")
        })
        .ok();

    let backend_port = env::var("BACKEND_PORT")
        .map(|string| {
            string
                .parse::<u16>()
                .expect("invalid format of 'BACKEND_PORT, please provide u16'")
        })
        .unwrap_or(8080u16);

    if backend_port > 25565{
        panic!("backend port to large, assign value below 25565");
    }

    let database_url = env::var("DATABASE_URL").expect("'DATABASE_URL' must be set.");

    let image_path = PathBuf::from_str(&env::var("IMAGE_PATH").unwrap_or("images".to_string())).expect("image_path does not exist");

    let image_ttl = env::var("IMAGE_TTL_SECS").map(|string|
    {
        let seconds = string.parse::<i64>().expect("Invalid format of 'IMAGE_TTL_SECS', please provide u64");
        Duration::seconds(seconds)
    }).ok();

    Config {
        max_image_width,
        max_image_height,
        max_image_size,
        max_memory_usage,
        backend_port,
        database_url,
        image_path,
        image_ttl,
    }
}
