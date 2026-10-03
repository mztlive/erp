//! 财务退款与冲正驳回原单的草稿编辑与成功审计事务。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_read_models::returns_center::dto::{
    CustomerRefundView, PaymentReversalView, ReceiptReversalView, SupplierRefundView,
};
use erp_returns::dto::UpdateFinancialReturnRequest;
use erp_returns::repository::ReturnsExt;
use erp_returns::service::ReturnsService;
use erp_returns::service::version_conflict::conflict_if_stale_version;
use erp_workflow::entity::document_registry::DocumentType;
use mongodb::Database;
use persistence_core::Executor;
use validator::Validate;

use super::ReturnsProcess;
use super::start_approval::ensure_return_start_replay_authorized;
use crate::audit::run_audited;
use crate::{Error, Result};

impl ReturnsProcess {
    /// 保存原单的可编辑财务字段，并保留原单号、资金来源和岗位。
    ///
    /// # 参数
    /// * `id` - 原单主键
    /// * `req` - 当前版本和可编辑字段
    /// * `actor` - 当前已认证的经办人
    ///
    /// # 返回
    /// 返回保存后的原单详情，含新乐观锁版本。
    ///
    /// # 错误
    /// 非原经办人、无提交或资金来源读取资格、非草稿、版本变化或字段非法时拒绝。
    pub async fn update_customer_refund(
        &self,
        id: &str,
        req: UpdateFinancialReturnRequest,
        actor: &AuditActor,
    ) -> Result<CustomerRefundView> {
        self.update_financial_draft(DocumentType::CustomerRefund, id, req, actor).await?;
        Ok(self.reads().customer_refund_detail(id).await?)
    }

    /// 保存原单的可编辑财务字段，并保留原单号、资金来源和岗位。
    ///
    /// # 参数
    /// * `id` - 原单主键
    /// * `req` - 当前版本和可编辑字段
    /// * `actor` - 当前已认证的经办人
    ///
    /// # 返回
    /// 返回保存后的原单详情，含新乐观锁版本。
    ///
    /// # 错误
    /// 非原经办人、无提交或资金来源读取资格、非草稿、版本变化或字段非法时拒绝。
    pub async fn update_supplier_refund(
        &self,
        id: &str,
        req: UpdateFinancialReturnRequest,
        actor: &AuditActor,
    ) -> Result<SupplierRefundView> {
        self.update_financial_draft(DocumentType::SupplierRefund, id, req, actor).await?;
        Ok(self.reads().supplier_refund_detail(id).await?)
    }

    /// 保存原单的可编辑财务字段，并保留原单号、资金来源和岗位。
    ///
    /// # 参数
    /// * `id` - 原单主键
    /// * `req` - 当前版本和可编辑字段
    /// * `actor` - 当前已认证的经办人
    ///
    /// # 返回
    /// 返回保存后的原单详情，含新乐观锁版本。
    ///
    /// # 错误
    /// 非原经办人、无提交或资金来源读取资格、非草稿、版本变化或字段非法时拒绝。
    pub async fn update_receipt_reversal(
        &self,
        id: &str,
        req: UpdateFinancialReturnRequest,
        actor: &AuditActor,
    ) -> Result<ReceiptReversalView> {
        self.update_financial_draft(DocumentType::ReceiptReversal, id, req, actor).await?;
        Ok(self.reads().receipt_reversal_detail(id).await?)
    }

    /// 保存原单的可编辑财务字段，并保留原单号、资金来源和岗位。
    ///
    /// # 参数
    /// * `id` - 原单主键
    /// * `req` - 当前版本和可编辑字段
    /// * `actor` - 当前已认证的经办人
    ///
    /// # 返回
    /// 返回保存后的原单详情，含新乐观锁版本。
    ///
    /// # 错误
    /// 非原经办人、无提交或资金来源读取资格、非草稿、版本变化或字段非法时拒绝。
    pub async fn update_payment_reversal(
        &self,
        id: &str,
        req: UpdateFinancialReturnRequest,
        actor: &AuditActor,
    ) -> Result<PaymentReversalView> {
        self.update_financial_draft(DocumentType::PaymentReversal, id, req, actor).await?;
        Ok(self.reads().payment_reversal_detail(id).await?)
    }

    /// 在同一事务重验当前资格，保存草稿并追加成功审计。
    async fn update_financial_draft(
        &self,
        kind: DocumentType,
        id: &str,
        req: UpdateFinancialReturnRequest,
        actor: &AuditActor,
    ) -> Result<()> {
        req.validate()?;
        let permission = format!("{}:submit", kind.as_str());
        let audit =
            actor.clone().resource_log(&format!("{}.update", kind.as_str()), kind.as_str(), id.to_owned())?;
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        run_audited(&self.db, audit, move |db, executor| {
            Box::pin(async move {
                ensure_return_start_replay_authorized(db, &rbac, &actor, kind, &permission, &id, executor)
                    .await?;
                persist_financial_draft(db, kind, &id, req, &actor, executor).await
            })
        })
        .await
    }
}

/// 按拥有领域的实际单据类型分派草稿更新，不改变任何资金过账事实。
async fn persist_financial_draft(
    db: &Database,
    kind: DocumentType,
    id: &str,
    req: UpdateFinancialReturnRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    match kind {
        DocumentType::CustomerRefund => persist_customer_refund_draft(db, id, req, actor, executor).await,
        DocumentType::SupplierRefund => persist_supplier_refund_draft(db, id, req, actor, executor).await,
        DocumentType::ReceiptReversal => persist_receipt_reversal_draft(db, id, req, actor, executor).await,
        DocumentType::PaymentReversal => persist_payment_reversal_draft(db, id, req, actor, executor).await,
        _ => Err(Error::ValidationError("该单据不支持财务草稿编辑".into())),
    }
}

/// 在调用方事务内加载原单、执行实体资格与版本守卫，再调用本域 CAS 保存。
async fn persist_customer_refund_draft(
    db: &Database,
    id: &str,
    req: UpdateFinancialReturnRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut record = db
        .customer_refunds()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原财务单据不存在".into()))?;
    record.ensure_draft_editor(actor.id())?;
    conflict_if_stale_version(record.matches_version(req.version))?;
    record.update(req.into()).map_err(|error| Error::ValidationError(error.to_string()))?;
    ReturnsService::new(db.clone()).persist_customer_refund(&mut record, executor).await?;
    Ok(())
}

/// 在调用方事务内加载原单、执行实体资格与版本守卫，再调用本域 CAS 保存。
async fn persist_supplier_refund_draft(
    db: &Database,
    id: &str,
    req: UpdateFinancialReturnRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut record = db
        .supplier_refunds()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原财务单据不存在".into()))?;
    record.ensure_draft_editor(actor.id())?;
    conflict_if_stale_version(record.matches_version(req.version))?;
    record.update(req.into()).map_err(|error| Error::ValidationError(error.to_string()))?;
    ReturnsService::persist_supplier_refund(db, &mut record, executor).await?;
    Ok(())
}

/// 在调用方事务内加载原单、执行实体资格与版本守卫，再调用本域 CAS 保存。
async fn persist_receipt_reversal_draft(
    db: &Database,
    id: &str,
    req: UpdateFinancialReturnRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut record = db
        .receipt_reversals()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原财务单据不存在".into()))?;
    record.ensure_draft_editor(actor.id())?;
    conflict_if_stale_version(record.matches_version(req.version))?;
    record.update(req.into()).map_err(|error| Error::ValidationError(error.to_string()))?;
    ReturnsService::persist_receipt_reversal(db, &mut record, executor).await?;
    Ok(())
}

/// 在调用方事务内加载原单、执行实体资格与版本守卫，再调用本域 CAS 保存。
async fn persist_payment_reversal_draft(
    db: &Database,
    id: &str,
    req: UpdateFinancialReturnRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut record = db
        .payment_reversals()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原财务单据不存在".into()))?;
    record.ensure_draft_editor(actor.id())?;
    conflict_if_stale_version(record.matches_version(req.version))?;
    record.update(req.into()).map_err(|error| Error::ValidationError(error.to_string()))?;
    ReturnsService::persist_payment_reversal(db, &mut record, executor).await?;
    Ok(())
}
