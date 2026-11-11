//! 业务材料转授只接受当前有效上传者或通用文件预览资格。

use application_core::AuditActor;
use erp_identity::{AuthorizationRegistry, AuthorizationService, Permission, SharedRbacService};
use erp_support::entity::file_asset::FileAsset;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 同一事务证明调用人可以把指定资产关联到单据或交付给审批参与者。
///
/// # 参数
/// * `identity_service` - 同一应用注入的共享身份与权限服务。
/// * `db` - 与原事务一致的身份数据库。
/// * `actor` - 当前已认证调用人或冻结快照的原提交人。
/// * `files` - 从资产仓储读取的真实资产集合。
/// * `executor` - 当前写入事务。
///
/// 本人上传分支保留直接转授规则，尚未接入 identity 统一对象授权。
///
/// # 返回
/// 每个资产均为本人上传或被当前通用预览权限覆盖时成功。
/// # 错误
/// 账号停用、资产来源未授权或策略版本变化时拒绝。
pub async fn authorize_material_transfer(
    identity_service: &SharedRbacService,
    db: &Database,
    actor: &AuditActor,
    files: &[FileAsset],
    executor: &mut dyn Executor,
) -> Result<()> {
    let qualified =
        AuthorizationService::new(db.clone(), identity_service.clone(), AuthorizationRegistry::default())
            .permissions(actor, &[Permission::parse("file_asset:preview")?], executor)
            .await?
            .ok_or_else(|| Error::Forbidden("文件材料提交账号已失效".into()))?;
    let preview = !qualified.permissions().is_empty();
    if files.iter().any(|file| !file.can_transfer_to_document(actor.id(), true, preview)) {
        return Err(Error::Forbidden("无权将他人文件关联到单据或作为审批材料".into()));
    }
    Ok(())
}
