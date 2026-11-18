//! 客户回款驳回原单的草稿编辑与审计事务。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::run_audited;
use erp_finance::dto::receivable::UpdateCustomerReceiptRequest;
use erp_finance::repository::ReceivableExt;
use erp_finance::service::receivable::mapping::ensure_expected_version;
use erp_identity::AccessTarget;
use erp_permissions::receivable;
use erp_read_models::finance::dto::CustomerReceiptView;
use mongodb::Database;
use persistence_core::Executor;
use validator::Validate;

use super::ReceivableProcess;
use super::customer_receipt::receipt_command_result;
use super::receipt_access::ensure_receipt_edit_authorized;
use crate::{Error, Result};

impl ReceivableProcess {
    /// 保存原回款草稿的到账字段，保留单号、来源与原登记人。
    ///
    /// # 参数
    /// * `id` - 原回款单主键
    /// * `req` - 原单当前版本与可编辑字段
    /// * `actor` - 当前已认证登记人
    ///
    /// # 返回
    /// 返回保存后的回款详情，含新乐观锁版本。
    ///
    /// # 错误
    /// 无提交或完整资金源读取资格、非原登记人、非草稿、版本变化或字段非法时拒绝。
    pub async fn update_customer_receipt(
        &self,
        id: &str,
        req: UpdateCustomerReceiptRequest,
        actor: &AuditActor,
    ) -> Result<CustomerReceiptView> {
        req.validate()?;
        let audit =
            actor.clone().resource_log("customer_receipt.update", "customer_receipt", id.to_owned())?;
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        let receipt_id = id.to_owned();
        run_audited(&self.db, audit, move |db, executor| {
            Box::pin(async move {
                ensure_receipt_edit_authorized(&rbac, &actor, &receipt_id, executor).await?;
                persist_receipt_draft(db, &receipt_id, req, executor).await?;
                receipt_command_result(
                    db,
                    &rbac.authorization,
                    &actor,
                    &receipt_id,
                    &receivable::CUSTOMER_RECEIPT_SUBMIT,
                    AccessTarget::record(&receipt_id)?,
                    executor,
                )
                .await
            })
        })
        .await
    }
}

/// 在当前事务重读原单，执行资格、版本和字段校验，再保存原单 CAS。
async fn persist_receipt_draft(
    db: &Database,
    id: &str,
    req: UpdateCustomerReceiptRequest,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut receipt = db
        .customer_receipts()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("客户回款单不存在".into()))?;
    receipt.ensure_draft_editable()?;
    ensure_expected_version(receipt.base.version, req.version)?;
    receipt.update(req.into()).map_err(|error| Error::ValidationError(error.to_string()))?;
    db.customer_receipts().update(&mut receipt, executor).await?;
    Ok(())
}
