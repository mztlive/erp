//! COS 图片读取必须取上传原图，避免自动压缩改变凭证的字节与内容指纹。

use url::Url;

use crate::{Error, Result};

/// 仅为 COS 图片对象添加原图参数；由 SDK 在请求签名前调用。
///
/// # 参数
/// * `uri` - SDK 已完成端点与对象键编码的读取地址。
/// # 返回
/// COS 图片返回带原图参数的地址；其他对象返回 `None`，保留原请求。
/// # 错误
/// 地址无法解析时返回存储错误，不发送未经处理的请求。
pub(crate) fn original_image_uri(uri: &str) -> Result<Option<String>> {
    let mut url = Url::parse(uri).map_err(|_| Error::S3("对象读取地址无效".into()))?;
    let host = url.host_str().unwrap_or_default();
    let is_cos = host.ends_with(".myqcloud.com") && (host.starts_with("cos.") || host.contains(".cos."));
    let extension = url.path().rsplit('.').next().unwrap_or_default().to_ascii_lowercase();
    if !is_cos || !matches!(extension.as_str(), "jpg" | "jpeg" | "png" | "gif" | "webp") {
        return Ok(None);
    }
    url.query_pairs_mut().append_pair("ci-process", "originImage");
    Ok(Some(url.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cos_images_preserve_encoded_keys_and_existing_query() {
        for host in ["cos.ap-guangzhou.myqcloud.com", "assets.cos.ap-guangzhou.myqcloud.com"] {
            let uri = format!("https://{host}/folder/receipt%20copy.PNG?x-id=GetObject");
            assert_eq!(original_image_uri(&uri).unwrap(), Some(format!("{uri}&ci-process=originImage")));
        }
    }

    #[test]
    fn other_storage_and_non_images_keep_standard_get() {
        for uri in [
            "https://s3.example.com/receipt.png?x-id=GetObject",
            "https://assets.s3.amazonaws.com/receipt.png",
            "https://cos.ap-guangzhou.myqcloud.com.example.org/receipt.png",
            "https://cdn.myqcloud.com/receipt.png",
            "https://assets.cos.ap-guangzhou.myqcloud.com/contract.pdf",
            "https://assets.cos.ap-guangzhou.myqcloud.com/import.xlsx",
            "https://assets.cos.ap-guangzhou.myqcloud.com/no-extension",
        ] {
            assert_eq!(original_image_uri(uri).unwrap(), None, "{uri}");
        }
    }
}
