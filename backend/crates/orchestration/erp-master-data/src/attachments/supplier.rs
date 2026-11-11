//! 在同一事务中登记资质文件的供应商资料命令。

use application_core::AuditActor;
use erp_file_assets::PendingFileAssets;
use erp_supplier::SaveSupplierProfileRequest;
use erp_support::PendingFileAssetRequest;

use crate::Result;
use crate::supplier_profile::{SupplierProfileProcess, SupplierProfileWithAssetsResult};

/// 创建供应商资料，并在同一事务中持久化已上传的资质文件。
///
/// # 参数
/// * `service` - 由应用组合根注入授权源及敏感字段编解码器的供应商资料服务。
/// * `req` - 供应商资料保存请求。
/// * `asset_requests` - 已写入对象存储的资质文件。
/// * `actor` - 已认证的审计操作人。
///
/// # 返回
/// 返回创建结果及附件提交状态。
///
/// # 错误
/// 临时文件校验失败，或资料创建与附件事务失败时返回错误。
pub async fn supplier_profile_create_with_assets(
    service: &SupplierProfileProcess,
    req: SaveSupplierProfileRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<SupplierProfileWithAssetsResult> {
    let pending = PendingFileAssets::prepare(asset_requests, &actor)?.shared();
    service.create_with_assets(req, pending, &actor).await
}

/// 更新供应商资料，并在同一事务中持久化已上传的资质文件。
///
/// # 参数
/// * `service` - 由应用组合根注入授权源及敏感字段编解码器的供应商资料服务。
/// * `id` - 供应商资料主键。
/// * `req` - 供应商资料保存请求。
/// * `asset_requests` - 已写入对象存储的资质文件。
/// * `actor` - 已认证的审计操作人。
///
/// # 返回
/// 返回更新结果及附件提交状态。
///
/// # 错误
/// 临时文件校验失败，或资料更新与附件事务失败时返回错误。
pub async fn supplier_profile_update_with_assets(
    service: &SupplierProfileProcess,
    id: String,
    req: SaveSupplierProfileRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<SupplierProfileWithAssetsResult> {
    let pending = PendingFileAssets::prepare(asset_requests, &actor)?.shared();
    service.update_with_assets(&id, req, pending, &actor).await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use erp_core::AccountKind;
    use erp_identity::ports::{IdentityAuditPort, PreparedResourceAudit};
    use erp_party::SensitiveDataCodec;
    use erp_supplier::SupplierAccess;
    use erp_support::{RegisterFileAssetRequest, RetentionClass, SensitivityClass};
    use mongodb::Client;
    use persistence_core::Executor;
    use serde_json::json;

    use super::*;
    use crate::Error;
    struct UnusedIdentityAudit;
    #[async_trait]
    impl IdentityAuditPort for UnusedIdentityAudit {
        fn resource_log(
            &self,
            _: AuditActor,
            _: &str,
            _: &str,
            _: String,
        ) -> erp_identity::Result<PreparedResourceAudit> {
            Err(erp_identity::Error::Internal("unit test audit must not execute".into()))
        }
        async fn persist(&self, _: &PreparedResourceAudit, _: &mut dyn Executor) -> erp_identity::Result<()> {
            panic!("input validation must stop before identity audit persistence")
        }
    }

    /// 只构造惰性客户端；输入校验必须在任何数据库或对象存储读取前返回。
    async fn service() -> SupplierProfileProcess {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("unit_test");
        let access = SupplierAccess::unwired(db.clone());
        let rbac = erp_identity::shared_rbac_service(db.clone(), Arc::new(UnusedIdentityAudit));
        SupplierProfileProcess::new(
            db,
            Arc::new(SensitiveDataCodec::from_secret(b"supplier-assets-unit-test")),
            rbac,
            access,
        )
    }

    fn request() -> SaveSupplierProfileRequest {
        serde_json::from_value(json!({
            "idempotency_key": "supplier-assets-unit-test",
            "party_no": "PARTY-1", "supplier_no": "SUP-1",
            "legal_name": "测试供应商",
            "contact": {"contact_name": "测试联系人", "mobile": "13800000000"},
            "settlement_mode": "monthly", "reconciliation_cycle": "none",
            "payment_term_snapshot": "PERIOD_MONTH_3",
            "invoice_type": "vat_special", "invoice_tax_rates": ["0.13"],
            "signing_entity_party_id": "party-1", "payment_entity_party_id": "party-1",
            "maintainer_user_id": "buyer-1", "capability_codes": ["physical"],
            "qualifications": [{
                "qualification_type": "certificate", "certificate_no": "TEST-1",
                "attachment_id": "pending-file:supplier:certificate:0",
                "capability_codes": ["physical"]
            }],
            "effective_from": "2026-10-20", "change_reason": "测试"
        }))
        .unwrap()
    }

    fn assets() -> Vec<PendingFileAssetRequest> {
        vec![PendingFileAssetRequest {
            reference: "pending-file:supplier:certificate:0".into(),
            registration: RegisterFileAssetRequest {
                storage_object_key: "unit-test/certificate.png".into(),
                file_name: "certificate.png".into(),
                content_type: "image/png".into(),
                byte_size: 12,
                content_hmac: "ab".repeat(32),
                sensitivity_class: SensitivityClass::Sensitive,
                retention_class: RetentionClass::LongTerm,
                expires_at: None,
            },
        }]
    }

    async fn save(
        service: &SupplierProfileProcess,
        req: SaveSupplierProfileRequest,
        update: bool,
    ) -> Result<SupplierProfileWithAssetsResult> {
        let actor = AuditActor::new("buyer-1".into(), "buyer".into(), AccountKind::Admin);
        if update {
            supplier_profile_update_with_assets(service, "supplier-1".into(), req, assets(), actor).await
        } else {
            supplier_profile_create_with_assets(service, req, assets(), actor).await
        }
    }

    #[tokio::test]
    async fn asset_commands_preserve_injected_authorization_and_reach_input_validation() {
        let service = service().await;
        for update in [false, true] {
            let mut req = request();
            req.clear_contact = true;
            let result = save(&service, req, update).await;
            assert!(matches!(
                result,
                Err(Error::ValidationError(message)) if message == "联系人不能同时替换和清空"
            ));
        }
    }
}
