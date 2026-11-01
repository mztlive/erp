//! 业务材料转授只接受当前有效上传者或通用文件预览资格。

use application_core::AuditActor;
use erp_support::entity::file_asset::FileAsset;
use erp_workflow::WorkflowAuthorizationPort;
use erp_workflow::service::approval::{
    approval_action_roles_with_executor, approval_actor_is_active_with_executor,
};
use persistence_core::Executor;

use crate::{Error, Result};

/// 同一事务证明调用人可以把指定资产关联到单据或交付给审批参与者。
///
/// # 参数
/// * `auth` - 组合层注入的账号与权限事实端口。
/// * `actor` - 当前已认证调用人或冻结快照的原提交人。
/// * `files` - 从资产仓储读取的真实资产集合。
/// * `executor` - 当前写入事务。
/// # 返回
/// 每个资产均为本人上传或被当前通用预览权限覆盖时成功。
/// # 错误
/// 账号停用、资产来源未授权或策略版本变化时拒绝。
pub async fn authorize_material_transfer(
    auth: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    files: &[FileAsset],
    executor: &mut dyn Executor,
) -> Result<()> {
    let active = approval_actor_is_active_with_executor(auth, actor, executor).await?;
    if !active {
        return Err(Error::Forbidden("文件材料提交账号已失效".into()));
    }
    let preview =
        !approval_action_roles_with_executor(auth, actor, "file_asset:preview", executor).await?.is_empty();
    if files.iter().any(|file| !file.can_transfer_to_document(actor.id(), active, preview)) {
        return Err(Error::Forbidden("无权将他人文件关联到单据或作为审批材料".into()));
    }
    Ok(())
}
