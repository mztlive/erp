//! 客户签收单凭证的纯元数据规则。
use erp_core::{Error, Result};

use crate::entity::facts::{EvidenceRetention, EvidenceSensitivity};

/// 从文件资产读取的签收凭证元数据，不携带存储对象键。
pub struct AcceptanceEvidenceMetadata<'a> {
    /// 文件展示名称，扩展名必须与 MIME 一致。
    pub file_name: &'a str,
    /// 声明的内容类型。
    pub content_type: &'a str,
    /// 文件大小，必须在 1 字节到 5 MiB 之间。
    pub byte_size: u64,
    /// 敏感级别。
    pub sensitivity: EvidenceSensitivity,
    /// 保留策略。
    pub retention: EvidenceRetention,
    /// 是否已经销毁。
    pub destroyed: bool,
    /// 是否被安全检查拒绝或隔离。
    pub security_blocked: bool,
}

/// 签收单必须为可读取的敏感图片或 PDF，并长期保留。
pub struct AcceptanceEvidencePolicy;

impl AcceptanceEvidencePolicy {
    /// 校验待签收凭证元数据。
    ///
    /// # 参数
    /// * `metadata` - 已登记资产的真实元数据
    /// # 返回
    /// 全部条件满足时成功。
    /// # 错误
    /// 类型、大小、扩展名、敏感级别、保留策略或治理状态不满足时拒绝。
    pub fn validate(metadata: &AcceptanceEvidenceMetadata<'_>) -> Result<()> {
        let extension = metadata.file_name.rsplit_once('.').map(|(_, value)| value.to_ascii_lowercase());
        let expected_mime = match extension.as_deref() {
            Some("pdf") => "application/pdf",
            Some("jpg" | "jpeg") => "image/jpeg",
            Some("png") => "image/png",
            Some("webp") => "image/webp",
            _ => return Err(Error::from("签收单凭证仅支持 PDF、JPG、PNG 或 WebP")),
        };
        if metadata.content_type != expected_mime {
            return Err(Error::from("签收单凭证扩展名与文件类型不一致"));
        }
        if metadata.byte_size == 0 || metadata.byte_size > 5 * 1024 * 1024 {
            return Err(Error::from("签收单凭证必须为不超过 5 MB 的非空文件"));
        }
        if metadata.sensitivity == EvidenceSensitivity::General {
            return Err(Error::from("签收单凭证必须按敏感文件保存"));
        }
        if metadata.retention != EvidenceRetention::LongTerm {
            return Err(Error::from("签收单凭证必须长期保留"));
        }
        if metadata.destroyed || metadata.security_blocked {
            return Err(Error::from("签收单凭证不可用，请重新上传"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AcceptanceEvidenceMetadata, AcceptanceEvidencePolicy, EvidenceRetention, EvidenceSensitivity,
    };

    fn metadata() -> AcceptanceEvidenceMetadata<'static> {
        AcceptanceEvidenceMetadata {
            file_name: "signature.pdf",
            content_type: "application/pdf",
            byte_size: 10,
            sensitivity: EvidenceSensitivity::Sensitive,
            retention: EvidenceRetention::LongTerm,
            destroyed: false,
            security_blocked: false,
        }
    }

    #[test]
    fn accepts_pdf_and_supported_images_at_upload_size_boundary() {
        for (name, mime) in [
            ("proof.pdf", "application/pdf"),
            ("proof.JPG", "image/jpeg"),
            ("proof.png", "image/png"),
            ("proof.webp", "image/webp"),
        ] {
            let valid = AcceptanceEvidenceMetadata {
                file_name: name,
                content_type: mime,
                byte_size: 5 * 1024 * 1024,
                ..metadata()
            };
            assert!(AcceptanceEvidencePolicy::validate(&valid).is_ok());
        }
    }

    #[test]
    fn rejects_invalid_type_size_and_governance() {
        let invalid = [
            AcceptanceEvidenceMetadata { content_type: "text/plain", ..metadata() },
            AcceptanceEvidenceMetadata { content_type: "image/svg+xml", ..metadata() },
            AcceptanceEvidenceMetadata { file_name: "proof.exe", ..metadata() },
            AcceptanceEvidenceMetadata { file_name: "proof.png", ..metadata() },
            AcceptanceEvidenceMetadata { byte_size: 0, ..metadata() },
            AcceptanceEvidenceMetadata { byte_size: 5 * 1024 * 1024 + 1, ..metadata() },
            AcceptanceEvidenceMetadata { sensitivity: EvidenceSensitivity::General, ..metadata() },
            AcceptanceEvidenceMetadata { retention: EvidenceRetention::Other, ..metadata() },
            AcceptanceEvidenceMetadata { destroyed: true, ..metadata() },
            AcceptanceEvidenceMetadata { security_blocked: true, ..metadata() },
        ];
        for value in invalid {
            assert!(AcceptanceEvidencePolicy::validate(&value).is_err());
        }
    }
}
