//! 原回款登记人的固定提交资格由共享授权服务检查。
use application_core::AuditActor;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::receivable;
use erp_workflow::entity::document_registry::DocumentType;
use persistence_core::Executor;

use crate::{Error, FinancePostingPorts, Result};

/// 原单读取、编辑及重提核验当前提交权限和真实原登记人。
/// # 参数
/// authorization 为共享服务；actor、id 和 executor 为原命令上下文。
/// # 返回
/// 当前提交权限和原登记人事实同时满足时成功。
/// # 错误
/// 账号、权限、原单或所有者资格不满足时拒绝；读取错误透传。
pub(super) async fn require_receipt_owner(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    Ok(authorization
        .require(actor, &receivable::CUSTOMER_RECEIPT_SUBMIT, AccessTarget::record(id)?, executor)
        .await?)
}

/// 先重验账号与提交操作资格，再读取完整回款来源；全程使用同一执行器。
///
/// # 参数
/// * `rbac` - 授权源。
/// * `actor` - 当前已认证操作人。
/// * `id` - 回款单 ID。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 账号、提交权限和完整资金来源资格都通过时无返回值。
///
/// # 错误
/// 账号不可用、缺少提交权限、无权读取完整资金来源，或授权读取失败时返回错误。
pub(super) async fn ensure_receipt_edit_authorized(
    rbac: &FinancePostingPorts,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    require_receipt_owner(&rbac.authorization, actor, id, executor).await?;
    if !rbac.runtime.source_readable(actor, DocumentType::CustomerReceipt, id, executor).await? {
        return Err(Error::Forbidden("无权读取该回款的完整资金来源".into()));
    }
    Ok(())
}
