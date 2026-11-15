//! 撤回审批由共享授权服务读取真实根和完整来源。
use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::inventory;
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshot;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

/// 按冻结的审批实例身份检查撤回资格。
/// # 参数
/// db为原数据库；identity为共享服务；actor、snapshot、executor为原事务。
/// # 返回
/// 根资格及完整来源资格成立时成功。
/// # 错误
/// 账号、来源或权限失效时拒绝。
pub(super) async fn require_cancel(
    _db: &Database,
    identity: &Arc<AuthorizationService>,
    actor: &AuditActor,
    snapshot: &ApprovalSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<()> {
    identity
        .require(
            actor,
            &inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL,
            AccessTarget::record(snapshot.approval_process_instance_id.as_ref())?,
            executor,
        )
        .await?;
    Ok(())
}
