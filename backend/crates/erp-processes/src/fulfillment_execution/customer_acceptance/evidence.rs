//! 由签收用例在同一 Executor 校验凭证存在、治理状态和转授资格。
use application_core::AuditActor;
use erp_core::ids::FileAssetId;
use erp_fulfillment::entity::fulfillment::{AcceptanceEvidenceMetadata, AcceptanceEvidencePolicy};
use erp_identity::SharedRbacService;
use erp_read_models::workbench::authorize_material_transfer;
use erp_support::{FileAssetExt, SecurityScanStatus};
use mongodb::Database;
use persistence_core::Executor;

use crate::adapters::workflow::workflow_auth;
use crate::fulfillment_execution::service_crypto::evidence_metadata;
use crate::{Error, Result};

/// 在签收事务内校验凭证，不读取对象存储。
///
/// # 参数
/// * `db` - 业务数据库。
/// * `rbac` - 权限服务。
/// * `asset_id` - 凭证文件身份。
/// * `actor` - 操作人。
/// * `executor` - 同一签收事务的执行器。
///
/// # 返回
/// 现有文件可作为当前签收凭证时成功。
///
/// # 错误
/// 文件缺失、格式不支持、治理状态无效或不可转授时拒绝。
pub(super) async fn ensure_evidence(
    db: &Database,
    rbac: &SharedRbacService,
    asset_id: &FileAssetId,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let asset = db
        .file_assets()
        .find_by_id(asset_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("签收单凭证不存在，请重新上传".into()))?;
    let (sensitivity, retention) = evidence_metadata(asset.sensitivity_class, asset.retention_class);
    AcceptanceEvidencePolicy::validate(&AcceptanceEvidenceMetadata {
        file_name: &asset.file_name,
        content_type: &asset.content_type,
        byte_size: asset.byte_size,
        sensitivity,
        retention,
        destroyed: asset.destroyed_at.is_some(),
        security_blocked: matches!(
            asset.security_scan_status,
            SecurityScanStatus::Rejected | SecurityScanStatus::Quarantined
        ),
    })
    .map_err(|error| Error::ValidationError(error.to_string()))?;
    let authorization = workflow_auth(db.clone(), rbac.clone());
    authorize_material_transfer(&authorization, actor, &[asset], executor).await?;
    Ok(())
}
