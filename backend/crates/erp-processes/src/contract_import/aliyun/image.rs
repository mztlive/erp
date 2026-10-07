//! 仅将完整渲染后所有像素均为不透明白色的页面认定为空白。
use std::io::Cursor;

use erp_contract::entity::recognition::ImportFailure;
use image::{ImageFormat, ImageReader, Limits};

pub(super) struct RenderedPage {
    pub bytes: Vec<u8>,
    pub blank: bool,
}

pub(super) fn inspect(bytes: Vec<u8>) -> Result<RenderedPage, ImportFailure> {
    let mut reader = ImageReader::with_format(Cursor::new(&bytes), ImageFormat::Png);
    let mut limits = Limits::default();
    limits.max_image_width = Some(3200);
    limits.max_image_height = Some(3200);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| ImportFailure::new("OCR_RENDER_FAILED", "页面图片无法完整解码，请检查 PDF 后重试"))?;
    let blank = image.to_rgba8().pixels().all(|pixel| pixel.0 == [255, 255, 255, 255]);
    Ok(RenderedPage { bytes, blank })
}

#[cfg(test)]
mod tests {
    use image::{DynamicImage, Rgba, RgbaImage};

    use super::*;
    #[test]
    fn only_fully_white_pixels_are_blank_not_faint_text_or_transparency() {
        for (pixel, blank) in
            [([255, 255, 255, 255], true), ([254, 255, 255, 255], false), ([255, 255, 255, 0], false)]
        {
            let mut image = RgbaImage::from_pixel(4, 4, Rgba([255, 255, 255, 255]));
            image.put_pixel(1, 1, Rgba(pixel));
            let mut bytes = Cursor::new(Vec::new());
            DynamicImage::ImageRgba8(image).write_to(&mut bytes, ImageFormat::Png).unwrap();
            assert_eq!(inspect(bytes.into_inner()).unwrap().blank, blank);
        }
        assert!(inspect(b"broken PNG".to_vec()).is_err());
    }
}
