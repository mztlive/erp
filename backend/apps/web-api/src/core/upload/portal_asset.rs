//! 门户图片与 PDF 的上传协议白名单，真实内容检查由专用处理器完成。

use super::{FormFile, MAX_UPLOAD_FILE_BYTES, normalized_extension};
use crate::core::errors::Error;

/// 扩展名、MIME、内容头与输入大小一致的门户文件；尚未宣称内容检查完成。
pub(crate) struct ValidatedPortalAsset {
    content: Vec<u8>,
    extension: String,
    content_type: &'static str,
}

impl FormFile {
    /// 校验门户文件的协议类型，PDF 仅允许作为资料。
    /// # 参数
    /// 使用累计不超过 5 MiB 的原始 multipart 文件。
    /// # 返回
    /// 返回需进一步完成完整图像解码或严格 PDF 检查的文件。
    /// # 错误
    /// 空内容、扩展名、MIME 或内容头不匹配时拒绝。
    pub(crate) fn validate_portal_asset(self) -> Result<ValidatedPortalAsset, Error> {
        if self.content.is_empty() || self.content.len() > MAX_UPLOAD_FILE_BYTES {
            return Err(Error::BadRequest("文件须为 5 MiB 内的有效图片或 PDF".into()));
        }
        if normalized_extension(&self.filename).as_deref() == Some("pdf") {
            if !self.content_type.as_deref().is_some_and(|mime| mime.eq_ignore_ascii_case("application/pdf"))
                || !self.content.starts_with(b"%PDF-")
            {
                return Err(Error::BadRequest("PDF 扩展名、MIME 与真实内容须一致".into()));
            }
            return Ok(ValidatedPortalAsset {
                content: self.content,
                extension: "pdf".into(),
                content_type: "application/pdf",
            });
        }
        let image = self.validate_image()?;
        Ok(ValidatedPortalAsset {
            content: image.content,
            extension: image.extension,
            content_type: image.content_type,
        })
    }
}

impl ValidatedPortalAsset {
    /// 返回经过大小和协议类型校验的原始字节。
    pub(crate) fn content(&self) -> &[u8] {
        &self.content
    }

    /// 返回与已验证扩展名一致的内容类型。
    pub(crate) fn content_type(&self) -> &'static str {
        self.content_type
    }

    /// 服务端按安全扩展名生成独立对象名。
    pub(crate) fn unique_name(&self) -> String {
        format!("{}.{}", id_generator::next_id(), self.extension)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, mime: &str, bytes: &[u8]) -> FormFile {
        FormFile { filename: name.into(), content_type: Some(mime.into()), content: bytes.to_vec() }
    }

    #[test]
    fn pdf_protocol_requires_matching_extension_type_and_nonempty_bounded_content() {
        let pdf =
            file("specification.PDF", "application/pdf", b"%PDF-1.7\n").validate_portal_asset().unwrap();
        assert_eq!(pdf.content_type(), "application/pdf");
        assert!(pdf.unique_name().ends_with(".pdf"));
        assert!(file("file.png", "application/pdf", b"%PDF-1.7").validate_portal_asset().is_err());
        assert!(file("file.pdf", "image/png", b"%PDF-1.7").validate_portal_asset().is_err());
        assert!(file("file.pdf", "application/pdf", b"damaged").validate_portal_asset().is_err());
        assert!(file("file.pdf", "application/pdf", b"").validate_portal_asset().is_err());
        assert!(file("file.svg", "image/svg+xml", b"<svg/>").validate_portal_asset().is_err());
        assert!(
            file("file.pdf", "application/pdf", &vec![0; MAX_UPLOAD_FILE_BYTES + 1])
                .validate_portal_asset()
                .is_err()
        );
    }
}
