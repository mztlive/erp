//! 客户回款驳回原单的草稿编辑与审计事务。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_finance::dto::receivable::UpdateCustomerReceiptRequest;
use erp_finance::repository::ReceivableExt;
use erp_finance::service::receivable::mapping::ensure_expected_version;
use erp_identity::SharedRbacService;
use erp_read_models::finance::dto::CustomerReceiptView;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::ports::WorkflowAuthorizationPort;
use erp_workflow::service::approval::{
    approval_action_roles_with_executor, approval_actor_is_active_with_executor,
};
use mongodb::Database;
use persistence_core::Executor;
use validator::Validate;

use super::ReceivableProcess;
use crate::adapters::workflow::workflow_auth;
use crate::audit::run_audited;
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
                ensure_receipt_edit_authorized(db, &rbac, &actor, &receipt_id, executor).await?;
                persist_receipt_draft(db, &receipt_id, req, &actor, executor).await
            })
        })
        .await?;
        Ok(self.read.customer_receipt_detail(id).await?)
    }
}

/// 先重验账号与提交操作资格，再读取完整回款来源；全程使用同一执行器。
///
/// # 参数
/// * `db` - 数据库。
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
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let authorization = workflow_auth(db.clone(), rbac.clone());
    if !approval_actor_is_active_with_executor(&authorization, actor, executor).await? {
        return Err(Error::Forbidden("当前账号不可编辑客户回款单".into()));
    }
    if approval_action_roles_with_executor(&authorization, actor, "customer_receipt:submit", executor)
        .await?
        .is_empty()
    {
        return Err(Error::Forbidden("当前账号缺少回款提交权限".into()));
    }
    if !authorization.approval_source_readable(actor, DocumentType::CustomerReceipt, id, executor).await? {
        return Err(Error::Forbidden("无权读取该回款的完整资金来源".into()));
    }
    Ok(())
}

/// 在当前事务重读原单，执行资格、版本和字段校验，再保存原单 CAS。
async fn persist_receipt_draft(
    db: &Database,
    id: &str,
    req: UpdateCustomerReceiptRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut receipt = db
        .customer_receipts()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("客户回款单不存在".into()))?;
    receipt.ensure_draft_editor(actor.id())?;
    ensure_expected_version(receipt.base.version, req.version)?;
    receipt.update(req.into()).map_err(|error| Error::ValidationError(error.to_string()))?;
    db.customer_receipts().update(&mut receipt, executor).await?;
    Ok(())
}
