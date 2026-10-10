//! 嵌入演示商品图片；文件不依赖运行目录或外部图片站。
use image::ImageFormat;

use crate::{Error, Result};

/// 读取固定清单对应的已生成 JPEG，并检查大小与文件类型。
/// # 参数
/// `product_no` 为演示商品种子身份。
/// # 返回
/// 可上传的 JPEG 字节。
/// # 错误
/// 未配置图片、类型或大小错误时拒绝生成。
pub(super) fn for_product(product_no: &str) -> Result<&'static [u8]> {
    let bytes: &[u8] = match product_no {
        "DEMO-MD-P-01" => include_bytes!("images/01.jpg"),
        "DEMO-MD-P-02" => include_bytes!("images/02.jpg"),
        "DEMO-MD-P-03" => include_bytes!("images/03.jpg"),
        "DEMO-MD-P-04" => include_bytes!("images/04.jpg"),
        "DEMO-MD-P-05" => include_bytes!("images/05.jpg"),
        "DEMO-MD-P-06" => include_bytes!("images/06.jpg"),
        "DEMO-MD-P-07" => include_bytes!("images/07.jpg"),
        "DEMO-MD-P-08" => include_bytes!("images/08.jpg"),
        "DEMO-MD-P-09" => include_bytes!("images/09.jpg"),
        "DEMO-MD-P-10" => include_bytes!("images/10.jpg"),
        "DEMO-MD-P-11" => include_bytes!("images/11.jpg"),
        "DEMO-MD-P-12" => include_bytes!("images/12.jpg"),
        "DEMO-MD-P-13" => include_bytes!("images/13.jpg"),
        "DEMO-MD-P-14" => include_bytes!("images/14.jpg"),
        "DEMO-MD-P-15" => include_bytes!("images/15.jpg"),
        "DEMO-MD-P-16" => include_bytes!("images/16.jpg"),
        "DEMO-MD-P-17" => include_bytes!("images/17.jpg"),
        "DEMO-MD-P-18" => include_bytes!("images/18.jpg"),
        "DEMO-MD-P-19" => include_bytes!("images/19.jpg"),
        "DEMO-MD-P-20" => include_bytes!("images/20.jpg"),
        "DEMO-MD-P-21" => include_bytes!("images/21.jpg"),
        "DEMO-MD-P-22" => include_bytes!("images/22.jpg"),
        "DEMO-MD-P-23" => include_bytes!("images/23.jpg"),
        "DEMO-MD-P-24" => include_bytes!("images/24.jpg"),
        "DEMO-MD-P-25" => include_bytes!("images/25.jpg"),
        "DEMO-MD-P-26" => include_bytes!("images/26.jpg"),
        "DEMO-MD-P-27" => include_bytes!("images/27.jpg"),
        "DEMO-MD-P-28" => include_bytes!("images/28.jpg"),
        _ => return Err(Error::ValidationError(format!("演示商品未配置图片：{product_no}"))),
    };
    if bytes.is_empty()
        || bytes.len() > 5 * 1024 * 1024
        || image::guess_format(bytes).ok() != Some(ImageFormat::Jpeg)
    {
        return Err(Error::ValidationError(format!("演示商品图片无效：{product_no}")));
    }
    Ok(bytes)
}
#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use zune_jpeg::JpegDecoder;

    use super::*;
    use crate::demo_master_data::plan;
    use crate::demo_master_data::seed::SeedRequest;
    #[test]
    fn every_demo_product_has_a_decodable_image() {
        for step in plan::demo_steps().unwrap() {
            if matches!(step.request, SeedRequest::Product(_)) {
                let mut decoder = JpegDecoder::new(Cursor::new(for_product(&step.key).unwrap()));
                let decoded = decoder.decode().unwrap();
                assert!(!decoded.is_empty());
                let info = decoder.info().unwrap();
                assert!(info.width >= 512 && info.height >= 512);
            }
        }
        assert!(for_product("unknown").is_err());
    }
}
