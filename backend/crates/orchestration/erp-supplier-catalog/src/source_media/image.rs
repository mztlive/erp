//! 验证来源图片的实际格式、解码尺寸与内容摘要。

use std::io::Cursor;

use image::{ImageFormat, ImageReader};
use sha2::{Digest, Sha256};
use zune_jpeg::JpegDecoder;

use super::DownloadedSourceImage;
use super::transport::MAX_SOURCE_IMAGE_BYTES;
use crate::{Error, Result};

pub(super) struct VerifiedSourceImage {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
    pub extension: &'static str,
    pub digest: String,
}

pub(super) fn validate(downloaded: DownloadedSourceImage) -> Result<VerifiedSourceImage> {
    let bytes = downloaded.bytes;
    if bytes.is_empty() || bytes.len() > MAX_SOURCE_IMAGE_BYTES {
        return Err(invalid_image());
    }
    let format = image::guess_format(&bytes).map_err(|_| invalid_image())?;
    let (content_type, extension) = match format {
        ImageFormat::Png => ("image/png", "png"),
        ImageFormat::Jpeg => ("image/jpeg", "jpg"),
        ImageFormat::WebP => ("image/webp", "webp"),
        _ => return Err(invalid_image()),
    };
    if downloaded.content_type.is_some_and(|mime| {
        !mime.split(';').next().is_some_and(|value| value.trim().eq_ignore_ascii_case(content_type))
    }) {
        return Err(Error::ValidationError("供应商图片实际格式与内容类型不一致".into()));
    }
    decode(&bytes, format)?;
    let digest = hex::encode(Sha256::digest(&bytes));
    Ok(VerifiedSourceImage { bytes, content_type, extension, digest })
}

fn decode(bytes: &[u8], format: ImageFormat) -> Result<()> {
    if format == ImageFormat::Jpeg {
        let mut decoder = JpegDecoder::new(Cursor::new(bytes));
        decoder.decode_headers().map_err(|_| invalid_image())?;
        let info = decoder.info().ok_or_else(invalid_image)?;
        check_dimensions(u32::from(info.width), u32::from(info.height))?;
        decoder.decode().map_err(|_| invalid_image())?;
    } else {
        let dimensions = ImageReader::with_format(Cursor::new(bytes), format)
            .into_dimensions()
            .map_err(|_| invalid_image())?;
        check_dimensions(dimensions.0, dimensions.1)?;
        image::load_from_memory_with_format(bytes, format).map_err(|_| invalid_image())?;
    }
    Ok(())
}

fn check_dimensions(width: u32, height: u32) -> Result<()> {
    if width == 0
        || height == 0
        || width > 16384
        || height > 32768
        || u64::from(width) * u64::from(height) > 40_000_000
    {
        return Err(invalid_image());
    }
    Ok(())
}

fn invalid_image() -> Error {
    Error::ValidationError("供应商图片格式、大小或实际尺寸无效；仅支持 PNG、JPEG、WebP".into())
}
