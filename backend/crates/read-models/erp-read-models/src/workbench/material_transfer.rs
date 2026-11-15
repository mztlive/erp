//! 业务材料转授只接受当前有效的真实上传者。

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::AuthorizationService;
use erp_support::authorization::files::require_transfer;
use erp_support::entity::file_asset::FileAsset;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

/// 同一事务证明调用人可以把指定资产关联到单据或交付给审批参与者。
///
/// # 参数
/// * `identity_service` - 同一应用注入的共享身份与权限服务。
/// * `db` - 与原事务一致的身份数据库。
/// * `actor` - 当前已认证调用人或冻结快照的原提交人。
/// * `files` - 从资产仓储读取的真实资产集合。
/// * `executor` - 当前写入事务。
///
/// # 返回
/// 每个资产均由当前账号真实上传时成功。
/// # 错误
/// 账号停用、资产来源未授权或策略版本变化时拒绝。
pub async fn authorize_material_transfer(
    identity_service: &Arc<AuthorizationService>,
    _db: &Database,
    actor: &AuditActor,
    files: &[FileAsset],
    executor: &mut dyn Executor,
) -> Result<()> {
    let ids = files.iter().map(|file| file.base.id.as_str()).collect::<Vec<_>>();
    require_transfer(identity_service, actor, &ids, executor).await?;
    Ok(())
}
