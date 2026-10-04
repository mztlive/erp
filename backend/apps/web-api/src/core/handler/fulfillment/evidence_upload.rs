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
    let mut expected = Vec::new();
    let reference = reference.to_string();
    if reference.starts_with("pending-file:") {
        expected.push(reference);
    }
    if expected.len() != files.len() {
        return Err(Error::BadRequest("现场图片凭证与确认命令不匹配".to_string()));
    }
    expected.sort();
    let mut actual = files.iter().map(|pending| pending.reference.clone()).collect::<Vec<_>>();
    actual.sort();
    if actual != expected {
        return Err(Error::BadRequest("现场图片凭证临时引用无效".to_string()));
    }
    if files.iter().any(|pending| {
        !matches!(pending.file.content_type.as_str(), "image/jpeg" | "image/png" | "image/webp")
    }) {
        return Err(Error::BadRequest("现场凭证仅支持 JPG、PNG 或 WebP 图片".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_fulfillment::dto::ConfirmElectronicDeliveryRequest;

    use crate::core::handler::file_asset::{AssetFile, PendingAssetFile};

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
}
