//! 电子交付与服务履约的 multipart 凭证协议校验。

use crate::core::errors::Error;
use crate::core::handler::file_asset::PendingAssetFile;

/// 校验现场凭证的临时引用、文件数量及图片类型。
///
/// # 参数
/// * `reference` - 确认命令中的凭证引用
/// * `files` - 本次解析出的具名文件
///
/// # 返回
/// 凭证引用与上传图片一一对应时成功。
///
/// # 错误
/// 文件数量、临时引用或图片 MIME 不匹配时返回 400。
pub(super) fn validate_evidence_upload(
    reference: &str,
    files: &[PendingAssetFile],
) -> std::result::Result<(), Error> {
    let file = match (reference.starts_with("pending-file:"), files) {
        (false, []) => return Ok(()),
        (true, [file]) => file,
        _ => return Err(Error::BadRequest("现场图片凭证与确认命令不匹配".to_string())),
    };
    if file.reference != reference {
        return Err(Error::BadRequest("现场图片凭证临时引用无效".to_string()));
    }
    if !matches!(file.file.content_type.as_str(), "image/jpeg" | "image/png" | "image/webp") {
        return Err(Error::BadRequest("现场凭证仅支持 JPG、PNG 或 WebP 图片".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_fulfillment::dto::ConfirmElectronicDeliveryRequest;

    use super::{Error, validate_evidence_upload};
    use crate::core::handler::file_asset::{AssetFile, PendingAssetFile};

    fn evidence(reference: &str, content_type: &str) -> PendingAssetFile {
        PendingAssetFile {
            reference: reference.into(),
            file: AssetFile {
                file_name: "delivery.png".into(),
                content_type: content_type.into(),
                content: vec![1],
            },
        }
    }

    #[test]
    fn electronic_confirmation_requires_facts_and_matching_evidence() {
        let fields = serde_json::json!({ "version": 1, "recipient_snapshot": "客户企业邮箱", "result": "SUCCESS", "quantity": "2", "occurred_at": 1_700_000_000, "evidence_attachment_id": "pending-file:electronic-evidence" });
        let request: ConfirmElectronicDeliveryRequest = serde_json::from_value(fields.clone()).unwrap();
        let image = PendingAssetFile {
            reference: "pending-file:electronic-evidence".into(),
            file: AssetFile {
                file_name: "delivery.png".into(),
                content_type: "image/png".into(),
                content: vec![1],
            },
        };
        assert!(
            super::validate_evidence_upload(
                request.evidence_attachment_id.as_ref(),
                std::slice::from_ref(&image)
            )
            .is_ok()
        );
        assert!(super::validate_evidence_upload(request.evidence_attachment_id.as_ref(), &[]).is_err());
        assert!(super::validate_evidence_upload("pending-file:unrelated", &[image]).is_err());
        let mut missing = fields;
        missing.as_object_mut().unwrap().remove("recipient_snapshot");
        assert!(serde_json::from_value::<ConfirmElectronicDeliveryRequest>(missing).is_err());
    }

    #[test]
    fn evidence_accepts_stored_reference_or_one_matching_image() {
        assert!(validate_evidence_upload("asset-1", &[]).is_ok());
        for content_type in ["image/jpeg", "image/png", "image/webp"] {
            let file = evidence("pending-file:evidence", content_type);
            validate_evidence_upload("pending-file:evidence", &[file]).unwrap();
        }
    }

    #[test]
    fn evidence_preserves_count_reference_and_type_error_order() {
        for (reference, files) in [
            ("pending-file:evidence", vec![]),
            ("asset-1", vec![evidence("wrong", "application/pdf")]),
            (
                "pending-file:evidence",
                vec![evidence("wrong-1", "application/pdf"), evidence("wrong-2", "application/pdf")],
            ),
        ] {
            assert!(matches!(
                validate_evidence_upload(reference, &files),
                Err(Error::BadRequest(message)) if message == "现场图片凭证与确认命令不匹配"
            ));
        }
        assert!(matches!(
            validate_evidence_upload("pending-file:evidence", &[evidence("wrong", "application/pdf")]),
            Err(Error::BadRequest(message)) if message == "现场图片凭证临时引用无效"
        ));
        assert!(matches!(
            validate_evidence_upload(
                "pending-file:evidence",
                &[evidence("pending-file:evidence", "application/pdf")]
            ),
            Err(Error::BadRequest(message)) if message == "现场凭证仅支持 JPG、PNG 或 WebP 图片"
        ));
    }
}
