//! 在同一事务中登记资质文件的供应商资料命令。

use std::sync::Arc;

use application_core::AuditActor;
use erp_party::SensitiveDataCodec;
use erp_supplier::SaveSupplierProfileRequest;
use erp_support::PendingFileAssetRequest;
use mongodb::Database;

use super::pending::PendingFileAssets;
use crate::Result;
use crate::supplier_profile::{SupplierProfileService, SupplierProfileWithAssetsResult};

/// 创建供应商资料，并在同一事务中持久化已上传的资质文件。
///
/// # 参数
/// * `db` - 供应商资料与文件资产所在数据库。
/// * `sensitive_data` - 供应商敏感字段编解码器。
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
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
    req: SaveSupplierProfileRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<SupplierProfileWithAssetsResult> {
    let pending = PendingFileAssets::prepare(asset_requests, &actor)?.shared();
    SupplierProfileService::new(db, sensitive_data).create_with_assets(req, pending, &actor).await
}

/// 更新供应商资料，并在同一事务中持久化已上传的资质文件。
///
/// # 参数
/// * `db` - 供应商资料与文件资产所在数据库。
/// * `sensitive_data` - 供应商敏感字段编解码器。
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
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
    id: String,
    req: SaveSupplierProfileRequest,
    asset_requests: Vec<PendingFileAssetRequest>,
    actor: AuditActor,
) -> Result<SupplierProfileWithAssetsResult> {
    let pending = PendingFileAssets::prepare(asset_requests, &actor)?.shared();
    SupplierProfileService::new(db, sensitive_data).update_with_assets(&id, req, pending, &actor).await
}
