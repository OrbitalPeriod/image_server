use image::ImageFormat as InnerImageFormat;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use sqlx::{Decode, Encode, PgPool, Postgres, Type};
use std::ops::Deref;

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub struct ImageFormat(pub InnerImageFormat);

impl ImageFormat {
    pub const PNG: ImageFormat = ImageFormat(image::ImageFormat::Png);
    pub const JPG: ImageFormat = ImageFormat(image::ImageFormat::Jpeg);
    pub const WEBP: ImageFormat = ImageFormat(image::ImageFormat::WebP);
    pub const HDR: ImageFormat = ImageFormat(image::ImageFormat::Hdr);
    pub const AVIF: ImageFormat = ImageFormat(image::ImageFormat::Avif);

    const PNG_EXT : &'static str = "png";
    const JPG_EXT : &'static str = "jpg";
    const JPEG_EXT : &'static str = "jpeg";
    const WEBP_EXT : &'static str = "webp";
    const HDR_EXT : &'static str = "hdr";
    const AVIF_EXT : &'static str = "avif";
    const UNWN_EXT : &'static str = "unkw";

    pub fn from_str(s: &str) -> Option<ImageFormat> {
        match s {
            Self::PNG_EXT => Some(ImageFormat(InnerImageFormat::Png)),
            Self::JPG_EXT | Self::JPEG_EXT => Some(ImageFormat(InnerImageFormat::Jpeg)),
            Self::WEBP_EXT => Some(ImageFormat(InnerImageFormat::WebP)),
            Self::HDR_EXT => Some(ImageFormat(InnerImageFormat::Hdr)),
            Self::AVIF_EXT => Some(ImageFormat(InnerImageFormat::Avif)),
            _ => None,
        }
    }

    pub fn to_str(self) -> &'static str {
        match self.0 {
            InnerImageFormat::Png => Self::PNG_EXT,
            InnerImageFormat::Jpeg => Self::JPG_EXT,
            InnerImageFormat::WebP => Self::WEBP_EXT,
            InnerImageFormat::Hdr => Self::HDR_EXT,
            InnerImageFormat::Avif => Self::AVIF_EXT,
            _ => Self::UNWN_EXT,
        }
    }

    pub fn extension(self) -> &'static str{
        self.to_str()
    }

    pub fn format(&self) -> image::ImageFormat {
        self.0
    }
}

impl Default for ImageFormat{
    fn default() -> Self {
        Self::PNG
    }
}

impl Deref for ImageFormat {
    type Target = InnerImageFormat;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ImageFormat {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let string = String::deserialize(de)?;
        Self::from_str(&string).ok_or(de::Error::custom("Invalid image format"))
    }
}

impl Serialize for ImageFormat {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_str())
    }
}

impl Type<Postgres> for ImageFormat {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <str as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for ImageFormat {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        let s = self.to_str();
        <&str as Encode<Postgres>>::encode(s, buf)
    }
}

impl<'r> Decode<'r, Postgres> for ImageFormat {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as Decode<Postgres>>::decode(value)?;
        ImageFormat::from_str(s).ok_or_else(|| "invalid image format".into())
    }
}

