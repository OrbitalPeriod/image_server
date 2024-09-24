use std::io::Cursor;

use image::{DynamicImage, ImageFormat};

pub struct TranscodeTarget{
    pub image_format : Option<ImageFormat>,
    pub image_width : Option<u32>,
    pub image_height : Option<u32>,
}

pub async fn transcode(image : DynamicImage, settings : TranscodeTarget) -> Vec<u8>{
    let bytes = tokio::task::spawn_blocking(move || {
        let image = if settings.image_width.is_some() || settings.image_height.is_some(){
            let width = settings.image_width.unwrap_or(image.width());
            let height = settings.image_height.unwrap_or(image.height());
            image.resize(width, height, image::imageops::FilterType::Lanczos3)
        }else{
            image
        };
    
        let mut bytes : Vec<u8> = Vec::new();
        let mut cursor = Cursor::new(&mut bytes);
    
        image.write_to(&mut cursor, settings.image_format.unwrap_or(ImageFormat::Png)).unwrap();

        bytes
    }).await.unwrap();

    
    bytes
}