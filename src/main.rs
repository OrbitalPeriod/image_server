use std::error::Error;

use image_server::Config;

#[tokio::main]
async fn main() {
    if let Ok(_) = dotenv::dotenv() {
        //dotenv loaded
    }

    let config = get_config();

    if let Err(e) = image_server::run(config).await {
        eprintln!("Some error occured running the server: {e:?}")
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
