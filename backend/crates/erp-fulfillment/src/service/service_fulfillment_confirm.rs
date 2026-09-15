//! 服务确认的本域地点规范化、密文事实构造和事务内状态写入。

use erp_core::common::time::Instant;
use erp_core::ids::{FileAssetId, ServiceFulfillmentId};
use persistence_core::Executor;

use super::FulfillmentService;
use crate::dto::ConfirmServiceFulfillmentRequest;
use crate::entity::fulfillment::{
    ActualServiceLocation, ServiceFulfillment, ServiceFulfillmentConfirmation,
    ServiceFulfillmentConfirmationParams,
};
use crate::ports::service_crypto::ServiceLocationCryptoPort;
use crate::repository::FulfillmentExt;
use crate::{Error, Result};

/// 把确认命令写成已规范化的现场事实。
///
/// 地点占位值与空白规则由领域值对象 [`ActualServiceLocation`] 独占；本函数
/// 只负责把已校验明文交给 Service 的加密编解码器与查询指纹函数。
///
/// # 参数
/// * `req` - 已通过校验的确认命令
/// * `evidence_attachment_id` - 已解析的正式凭证主键
/// * `fingerprint_key` - 服务地点查询指纹密钥
/// * `sensitive_data` - 服务地点加密编解码器
///
/// # 返回
/// 返回可写入草稿的确认事实。
///
/// # 错误
/// 实体规范化失败时返回校验错误。
pub fn service_confirmation_from_request<C: ServiceLocationCryptoPort>(
    req: &ConfirmServiceFulfillmentRequest,
    evidence_attachment_id: FileAssetId,
    fingerprint_key: &[u8],
    sensitive_data: &C,
) -> std::result::Result<ServiceFulfillmentConfirmation, C::Error> {
    let service_location = ActualServiceLocation::parse(&req.service_location)
        .map_err(|error| Error::ValidationError(error.to_string()))?;
    Ok(ServiceFulfillmentConfirmation::new(ServiceFulfillmentConfirmationParams {
        result: req.result,
        completion_note: req.completion_note.clone(),
        evidence_attachment_id,
        service_location_encrypted: sensitive_data.encrypt(service_location.as_str())?,
        service_location_fingerprint: ServiceFulfillment::service_location_fingerprint(
            service_location.as_str(),
            fingerprint_key,
        ),
        service_started_at: Instant::from_unix_secs(req.service_started_at),
        service_ended_at: Instant::from_unix_secs(req.service_ended_at),
        quantity: req.quantity,
    })
    .map_err(Error::from)?)
}

impl FulfillmentService {
    /// 在调用方事务中读取并锁定服务草稿版本，保留状态与版本首错。
    pub async fn prepare_service_confirmation(
        &self,
        record_id: &ServiceFulfillmentId,
        expected_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<ServiceFulfillment> {
        let record = self
            .db
            .service_fulfillments()
            .find_by_id(record_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("服务履约记录不存在".to_string()))?;
        record
            .ensure_draft_version(expected_version)
            .map_err(|error| Error::ConflictError(error.to_string()))?;
        Ok(record)
    }

    /// 在附件资格与登记完成之后应用现场事实、确认并写入同一服务记录。
    pub async fn persist_service_confirmation(
        &self,
        record: &mut ServiceFulfillment,
        confirmation: ServiceFulfillmentConfirmation,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        record.apply_confirmation(confirmation)?;
        record.confirm()?;
        self.db.service_fulfillments().update(record, executor).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::str::FromStr;

    use erp_core::money::Quantity;

    use super::*;
    use crate::entity::fulfillment::FulfillmentResult;

    struct RecordingCrypto {
        plain: RefCell<Vec<String>>,
        fail: bool,
    }
    impl ServiceLocationCryptoPort for RecordingCrypto {
        type Error = Error;
        fn encrypt(&self, plaintext: &str) -> Result<String> {
            self.plain.borrow_mut().push(plaintext.to_string());
            if self.fail {
                return Err(Error::Internal("encryption failure".into()));
            }
            Ok("ciphertext".into())
        }
    }
    fn request(location: &str) -> ConfirmServiceFulfillmentRequest {
        ConfirmServiceFulfillmentRequest {
            version: 1,
            result: FulfillmentResult::Success,
            completion_note: "上门安装完成".into(),
            service_location: location.into(),
            service_started_at: 1_700_000_000,
            service_ended_at: 1_700_003_600,
            quantity: Quantity::from_str("1").unwrap(),
            evidence_attachment_id: FileAssetId::new("file-1"),
        }
    }

    #[test]
    fn invalid_location_fails_before_encryption() {
        let crypto = RecordingCrypto { plain: RefCell::new(Vec::new()), fail: true };
        let error =
            service_confirmation_from_request(&request("  "), FileAssetId::new("file-1"), b"key", &crypto)
                .unwrap_err();
        assert!(matches!(error, Error::ValidationError(_)));
        assert!(crypto.plain.borrow().is_empty());
    }

    #[test]
    fn encryption_failure_keeps_original_error_before_confirmation_rules() {
        let crypto = RecordingCrypto { plain: RefCell::new(Vec::new()), fail: true };
        let mut request = request("  客户现场  ");
        request.service_ended_at = request.service_started_at - 1;
        let error = service_confirmation_from_request(&request, FileAssetId::new("file-1"), b"key", &crypto)
            .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "encryption failure"));
        assert_eq!(*crypto.plain.borrow(), ["客户现场"]);
    }
}
