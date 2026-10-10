//! 旧采购应付直付生命周期已经停止；应付形成不再授予付款资格。
use application_core::AuditActor;
use erp_core::ids::{PayableAccountId, WorkItemId};
use erp_finance::entity::payable::PayableAccount;
use erp_identity::SharedRbacService;
use erp_workflow::entity::work_item::WorkItem;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 应付变化不再重建旧直付任务。
/// # 参数
/// * `db` / `account_id` / `executor` - 原核销事务事实。
/// # 返回
/// 无旧任务写入，成功返回。
/// # 错误
/// 不返回错误。
pub(crate) async fn sync_purchase_payment_task(
    _db: &Database,
    _account_id: &PayableAccountId,
    _executor: &mut dyn Executor,
) -> Result<()> {
    Ok(())
}
/// 旧应付任务永远不能揭示账户或执行新付款。
/// # 参数
/// * 所有参数 - 旧入口任务资格，保留内部调用签名。
/// # 返回
/// 不返回授权事实。
/// # 错误
/// 返回固定冲突，要求走付款申请。
pub(crate) async fn authorize_payment_execution(
    _db: &Database,
    _rbac: &SharedRbacService,
    _work_item_id: &WorkItemId,
    _expected_task_version: u64,
    _expected_account_id: Option<&PayableAccountId>,
    _actor: &AuditActor,
    _executor: &mut dyn Executor,
) -> Result<(WorkItem, PayableAccount)> {
    Err(Error::ConflictError("旧采购应付直付入口已停止，请先提交并批准采购付款申请".into()))
}
