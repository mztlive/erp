//! 原回款登记人的固定提交资格由共享授权服务检查。
use application_core::AuditActor;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::receivable;
use persistence_core::Executor;

use crate::Result;

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
