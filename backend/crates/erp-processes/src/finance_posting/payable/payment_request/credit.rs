//! 出纳在已批准付款申请内使用同主体、同供应商留抵。
use entity_core::BaseModel;
use erp_audit::AuditActorLogs;
use erp_core::ids::WorkItemId;
use erp_finance::dto::payable::credit::UseSupplierCreditRequest;
use erp_finance::entity::payable::supplier_credit::{SupplierCredit, SupplierCreditUse};
use erp_finance::entity::payable::{PaymentRequestAllocation, PendingPaymentAllocation};
use erp_finance::repository::supplier_credit::{available_credits, supplier_credit_uses, supplier_credits};
use erp_finance::service::payable::{finish_credit_allocation, settle_supplier_payment};
use erp_returns::repository::{ReturnsExt, SupplierRefundPostedTotalsExt};
use erp_workflow::WorkItemExt;

use super::*;
use crate::adapters::workflow::work_item_service;
use crate::audit::persist_log;

impl PurchasePaymentRequestProcess {
    /// 查询当前付款任务可选择的供应商留抵。
    /// # 参数
    /// * `id` - 付款申请。
    /// * `actor` - 指定出纳。
    /// # 返回
    /// 同主体、同供应商的可用余额。
    /// # 错误
    /// 任务不开放、责任或权限不符时拒绝。
    pub async fn supplier_credits(&self, id: &str, actor: &AuditActor) -> Result<Vec<SupplierCredit>> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let request = load(&db, &id, executor).await?;
                    let task_id = request
                        .work_item_id
                        .as_ref()
                        .ok_or_else(|| Error::ConflictError("没有开放付款任务".into()))?;
                    let task = db
                        .work_items()
                        .find_by_id(task_id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("付款任务不存在".into()))?;
                    execution::authorize_task(
                        &db,
                        &rbac,
                        &request,
                        &WorkItemId::new(task_id),
                        task.base.version,
                        &actor,
                        executor,
                    )
                    .await?;
                    Ok(available_credits(&db, &request.supplier_id, &request.payer_party_id, executor)
                        .await?)
                })
            })
            .await
    }
    /// 在批准额度内抵扣留抵，并按同一请求键返回原结果。
    /// # 参数
    /// * `id` - 付款申请。
    /// * `req` - 本次抵扣金额、来源、任务版本及请求键。
    /// * `actor` - 指定出纳。
    /// # 返回
    /// 抵扣记录身份，不产生新的银行付款。
    /// # 错误
    /// 非本人任务、版本变化、主体不符、超额或异载荷重放时拒绝。
    pub async fn use_supplier_credit(
        &self,
        id: &str,
        req: UseSupplierCreditRequest,
        actor: &AuditActor,
    ) -> Result<String> {
        req.validate()?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { apply(&db, &rbac, &id, &req, &actor, executor).await })
            })
            .await
    }
}

async fn apply(
    db: &Database,
    rbac: &SharedRbacService,
    id: &str,
    req: &UseSupplierCreditRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<String> {
    let result_id = format!("SCU-{}", req.idempotency_key);
    if let Some(previous) = supplier_credit_uses(db).find_by_id(&result_id, executor).await? {
        authorize_replay(db, rbac, id, req, actor, &previous, executor).await?;
        return Ok(result_id);
    }
    let mut request = load(db, id, executor).await?;
    if request.base.version != req.expected_version {
        return Err(Error::ConflictError("付款申请已变化，请刷新后重新核对".into()));
    }
    execution::authorize_task(
        db,
        rbac,
        &request,
        &req.work_item_id,
        req.task_version.parse().map_err(|_| Error::ValidationError("任务版本无效".into()))?,
        actor,
        executor,
    )
    .await?;
    source::revalidate(db, &request, executor).await?;
    let entry = db
        .payable_entries()
        .find_by_id(&req.payable_entry_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("应付分录不存在".into()))?;
    if entry.payable_account_id.as_ref() != req.payable_account_id {
        return Err(Error::ConflictError("分录与所选采购应付不一致".into()));
    }
    let allocation =
        PaymentRequestAllocation { payable_account_id: req.payable_account_id.clone(), amount: req.amount };
    request.record_payment(std::slice::from_ref(&allocation))?;
    let mut credit = supplier_credits(db)
        .find_by_id(&req.credit_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("留抵余额不存在".into()))?;
    credit.consume(&request.supplier_id, &request.payer_party_id, req.amount)?;
    post_allocation(db, &request, &credit, req, actor, executor).await?;
    supplier_credits(db).update(&mut credit, executor).await?;
    db.purchase_payment_requests().update(&mut request, executor).await?;
    persist_use(db, id, &result_id, req, actor, executor).await?;
    execution::sync_task(db, id, executor).await?;
    let audit =
        actor.clone().resource_log("supplier_credit.use", "purchase_payment_request", id.to_owned())?;
    persist_log(db, &audit, executor).await?;
    Ok(result_id)
}

async fn post_allocation(
    db: &Database,
    request: &PurchasePaymentRequest,
    credit: &SupplierCredit,
    req: &UseSupplierCreditRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut payment = db
        .supplier_payments()
        .find_by_id(&credit.original_payment_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("留抵原付款不存在".into()))?;
    if payment.payer_party_id.as_deref() != Some(request.payer_party_id.as_str())
        || payment.supplier_id.as_ref() != request.supplier_id
    {
        return Err(Error::ConflictError("留抵原付款主体与本申请不一致".into()));
    }
    if !db
        .payable_accounts()
        .consume_payment_request(&req.payable_account_id, &req.amount, actor.id(), executor)
        .await?
    {
        return Err(Error::ConflictError("批准未付额度已变化".into()));
    }
    payment.cash_refunded_amount = db
        .supplier_refunds()
        .posted_refund_total_by_payment(&payment.base.id.clone().into(), "", executor)
        .await?;
    let pending = PendingPaymentAllocation {
        payable_entry_id: req.payable_entry_id.clone().into(),
        allocated_amount: req.amount,
    };
    let settlement = settle_supplier_payment(db, &payment, &[pending], actor.id(), executor).await?;
    finish_credit_allocation(db, &settlement, executor).await?;
    // 原付款 CAS 与退款、留抵消费共同仲裁可用资金。
    db.supplier_payments().update(&mut payment, executor).await?;
    Ok(())
}

async fn authorize_replay(
    db: &Database,
    rbac: &SharedRbacService,
    id: &str,
    req: &UseSupplierCreditRequest,
    actor: &AuditActor,
    previous: &SupplierCreditUse,
    executor: &mut dyn Executor,
) -> Result<()> {
    if previous.actor_id != actor.id()
        || previous.payment_request_id != id
        || previous.work_item_id != req.work_item_id.as_ref()
        || previous.task_version != req.task_version
        || previous.request_version != req.expected_version
        || previous.credit_id != req.credit_id
        || previous.payable_account_id != req.payable_account_id
        || previous.payable_entry_id != req.payable_entry_id
        || previous.amount != req.amount
    {
        return Err(Error::ConflictError("抵扣提交标识已用于其他内容".into()));
    }
    let task = db
        .work_items()
        .find_by_id(&req.work_item_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原任务不存在".into()))?;
    if task.owner_user_id.as_deref() != Some(actor.id()) || task.business_object_id != id {
        return Err(Error::Forbidden("不能读取其他任务的抵扣结果".into()));
    }
    work_item_service(db.clone(), rbac.clone()).ensure_domain_decision_access(actor, &task, executor).await?;
    Ok(())
}

async fn persist_use(
    db: &Database,
    id: &str,
    result_id: &str,
    req: &UseSupplierCreditRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    supplier_credit_uses(db)
        .create(
            &SupplierCreditUse {
                base: BaseModel::new(result_id.to_owned()),
                credit_id: req.credit_id.clone(),
                payment_request_id: id.into(),
                work_item_id: req.work_item_id.to_string(),
                task_version: req.task_version.clone(),
                request_version: req.expected_version,
                payable_account_id: req.payable_account_id.clone(),
                payable_entry_id: req.payable_entry_id.clone(),
                amount: req.amount,
                actor_id: actor.id().into(),
                idempotency_key: req.idempotency_key.clone(),
            },
            executor,
        )
        .await?;
    Ok(())
}
