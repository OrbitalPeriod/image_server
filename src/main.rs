use image_server::Config;
use tracing::warn;
use tracing_subscriber::FmtSubscriber;

mod api;

#[tokio::main]
async fn main() {
    let subscriber = if cfg!(debug_assertions){
        FmtSubscriber::builder().with_max_level(tracing::Level::DEBUG).finish()
    }else{
        FmtSubscriber::builder().with_max_level(tracing::Level::WARN).finish()
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

    Config {
        max_image_width,
        max_image_height,
        max_image_size,
        max_memory_usage,
        backend_port,
    }
}
