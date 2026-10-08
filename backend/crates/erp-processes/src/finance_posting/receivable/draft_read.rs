//! 原回款完整编辑视图与未知提交确认；读取不改变单据状态。

use application_core::AuditActor;
use erp_finance::entity::receivable::CustomerReceipt;
use erp_finance::repository::ReceivableExt;
use erp_identity::SharedRbacService;
use erp_read_models::finance::dto::CustomerReceiptView;
use erp_read_models::finance::receivable::ReceivableReadService;
use erp_workflow::service::approval::approval_action_roles_with_executor;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::ReceivableProcess;
use super::draft_update::ensure_receipt_edit_authorized;
use crate::adapters::workflow::workflow_auth;
use crate::{Error, Result};

impl ReceivableProcess {
    /// 读取原登记人的完整回款字段，供草稿编辑与未知提交确认。
    ///
    /// # 参数
    /// * `id` - 原回款主键
    /// * `actor` - 当前已认证原登记人
    ///
    /// # 返回
    /// 返回真实状态、金额、版本、审批与原拟核销分配；只有 Draft 状态可编辑。
    ///
    /// # 错误
    /// 当前账号失效、无提交或完整来源资格、非原登记人或读取失败时拒绝。
    pub async fn customer_receipt_draft(&self, id: &str, actor: &AuditActor) -> Result<CustomerReceiptView> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    ensure_receipt_edit_authorized(&db, &rbac, &actor, &id, executor).await?;
                    let receipt = db
                        .customer_receipts()
                        .find_by_id(&id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("客户回款单不存在".into()))?;
                    ensure_receipt_owner(&receipt, actor.id())?;
                    receipt_view_with_actions(&db, &rbac, &actor, &id, executor).await
                })
            })
            .await
    }
}

/// 原单读资格已在调用方执行器证明；撤回动作仍须当前 cancel 静态资格。
///
/// # 参数
/// * `db` - 数据库。
/// * `rbac` - 授权源。
/// * `actor` - 当前已认证操作人。
/// * `id` - 回款单 ID。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 返回回款视图；当前账号没有撤回静态资格时去掉 `CANCEL`。
///
/// # 错误
/// 视图或撤回资格读取失败时返回错误。
pub(super) async fn receipt_view_with_actions(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<CustomerReceiptView> {
    let mut view =
        ReceivableReadService::new(db.clone()).customer_receipt_view_with_executor(id, executor).await?;
    if view.approval.allowed_actions.iter().any(|action| action == "CANCEL") {
        let auth = workflow_auth(db.clone(), rbac.clone());
        if approval_action_roles_with_executor(&auth, actor, "customer_receipt:cancel_approval", executor)
            .await?
            .is_empty()
        {
            view.approval.allowed_actions.retain(|action| action != "CANCEL");
        }
    }
    Ok(view)
}

/// 完整原单读取和提交回放仅允许非空的原登记人身份。
///
/// # 参数
/// * `receipt` - 已读取的回款单。
/// * `actor_id` - 当前操作人 ID。
///
/// # 返回
/// 身份与 `created_by` 一致时无返回值。
///
/// # 错误
/// 操作人身份为空或不是原登记人时返回 `Forbidden`。
pub(super) fn ensure_receipt_owner(receipt: &CustomerReceipt, actor_id: &str) -> Result<()> {
    if actor_id.is_empty() || receipt.created_by != actor_id {
        return Err(Error::Forbidden("仅原回款登记人可以读取或重提本单".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::{CustomerReceiptId, PartyId};
    use erp_core::money::Amount;
    use erp_finance::entity::receivable::{CustomerReceiptData, CustomerReceiptStatus};

    use super::*;

    /// 读取允许原人确认非草稿结果；编辑仍沿实体草稿门禁，历史空登记人拒绝。
    #[test]
    fn owner_read_and_draft_edit_have_distinct_state_requirements() {
        let mut receipt = CustomerReceipt::new(
            CustomerReceiptId::new("receipt"),
            CustomerReceiptData {
                receipt_no: "RC-1".into(),
                counterparty_party_id: PartyId::new("party"),
                customer_id: None,
                received_at: Instant::from_unix_secs(10),
                amount: "100.00".parse::<Amount>().unwrap(),
                bank_reference: None,
            },
            "creator",
        )
        .unwrap();
        assert!(ensure_receipt_owner(&receipt, "other").is_err());
        assert!(ensure_receipt_owner(&receipt, "").is_err());
        for status in [
            CustomerReceiptStatus::Draft,
            CustomerReceiptStatus::InApproval,
            CustomerReceiptStatus::Posted,
            CustomerReceiptStatus::Reversed,
        ] {
            receipt.status = status;
            ensure_receipt_owner(&receipt, "creator").unwrap();
            assert_eq!(
                receipt.ensure_draft_editor("creator").is_ok(),
                status == CustomerReceiptStatus::Draft
            );
        }
        receipt.created_by.clear();
        assert!(ensure_receipt_owner(&receipt, "creator").is_err());
    }
}
