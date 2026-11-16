//! 客户退款审批启动、精确授权回放、8次fresh恢复与撤回编排。
use erp_returns::service::ReturnsService;
use persistence_core::Executor;

use super::*;
use crate::ReverseFlowPorts;
impl ReturnsProcess {
    /// 从绑定读取定义并持久化启动事实。
    ///
    /// # 参数
    /// * `refund` - 已进入提交状态的退款单。
    /// * `idempotency_key` - 启动幂等键。
    /// * `resubmission_reason` - 本次重提说明。
    /// * `submission` - 绑定原单、已授权操作人与精确请求收据的提交上下文。
    /// * `adapter` - 客户退款审批适配器。
    ///
    /// # 返回
    /// 返回提交后的退款单视图。
    ///
    /// # 错误
    /// 无绑定、定义缺失、写入失败或视图读取失败时返回错误。提交结果未知且有界回读未确认时返回原错误。
    pub(super) async fn dispatch_customer_refund_start(
        &self,
        refund: CustomerRefund,
        idempotency_key: String,
        resubmission_reason: Option<String>,
        adapter: super::super::adapter::CustomerRefundAdapter,
        submission: ReversalSubmitReplayInput,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let id = submission.id.as_str();
        let actor = &submission.actor;
        let subject = customer_refund_subject_ref(id)?;
        let binding = find_approval_binding(&self.db, id, executor).await.map_err(crate::Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let now = Instant::now();
        let organization_id =
            load_customer_responsible_org_id(&self.db, &refund.customer_id, executor).await?;
        let snapshot = build_customer_refund_snapshot(&refund, &organization_id, actor.id(), now)?;
        let existing_receipt = load_start_receipt(
            &self.db,
            &subject,
            refund.approval_subject_version,
            &idempotency_key,
            executor,
        )
        .await?;
        let graph =
            load_start_graph(&self.db, &self.rbac, &binding, actor.id(), existing_receipt.as_ref(), executor)
                .await?;
        let prepared = prepare_start(build_customer_refund_start_input(ReverseStartInput {
            resubmission_reason: resubmission_reason.clone(),
            graph,
            binding: &binding,
            subject,
            subject_version: refund.approval_subject_version,
            actor_id: actor.id(),
            organization_id: &organization_id,
            idempotency_key: &idempotency_key,
            receipt: existing_receipt,
            now,
        })?)?;
        persist_customer_refund_start(
            &self.db,
            &self.rbac,
            CustomerRefundStartPersistInput {
                command_receipt: submission.command_receipt.clone(),
                resubmission_reason: resubmission_reason.clone(),
                refund,
                actor: actor.clone(),
                id: id.to_string(),
                snapshot_payload: snapshot,
                prepared,
                owner_role: adapter.owner_role,
                organization_id,
                now,
            },
            executor,
        )
        .await
    }

    /// 加载撤回运行事实并写回草稿。
    ///
    /// # 参数
    /// * `id` - 退款单主键。
    /// * `refund` - 待撤回的退款单；领域动作成功后会先改成草稿。
    /// * `req` - 撤回请求。
    /// * `actor` - 撤回人。
    ///
    /// # 返回
    /// 成功时无返回值。
    ///
    /// # 错误
    /// 无绑定、实例终态或写入失败时返回错误。
    pub(super) async fn persist_cancelled_customer_refund(
        &self,
        id: &str,
        refund: &mut CustomerRefund,
        req: &CancelCustomerRefundApprovalRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let adapter = customer_refund_adapter()?;
        let binding = find_approval_binding(&self.db, id, executor).await?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let subject = customer_refund_subject_ref(id)?;
        let runtime =
            load_cancel_runtime(&self.db, &binding, &subject, refund.approval_subject_version, executor)
                .await?;
        let now = Instant::now();
        let idempotency_key = normalize_idempotency_key(&req.idempotency_key)?;
        let input = build_customer_refund_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &idempotency_key,
            None,
            now,
        )?;
        let prepared = prepare_cancel(input)?;
        execute_customer_refund_domain_action(refund, adapter.cancel_action)?;
        let audit = actor.clone().resource_log(
            "customer_refund.cancel_approval",
            "customer_refund",
            id.to_string(),
        )?;
        persist_customer_refund_cancel(
            &self.db,
            &self.rbac,
            CustomerRefundCancelPersistInput {
                refund: refund.clone(),
                prepared,
                open_tasks: runtime.open_tasks,
                actor_id: actor.id().to_string(),
                reason: req.reason.clone(),
                now,
                audit,
            },
            executor,
        )
        .await
    }
}

/// 先执行真实单据的完整提交规则，再读取退款业务状态，沿用调用方事务。
/// # 参数
/// * `db` / `rbac` - 数据库与审批授权端口。
/// * `actor` / `refund_id` - 当前操作人与退款单主键。
/// * `executor` - 同一事务执行器。
/// # 返回
/// 返回完成原来源授权重验的退款单。
/// # 错误
/// 原单或往来主体不存在、来源无权限或提交资格失效时返回错误。
pub(in crate::reverse_flow) async fn authorized_customer_refund(
    db: &Database,
    rbac: &ReverseFlowPorts,
    actor: &AuditActor,
    refund_id: &str,
    executor: &mut dyn Executor,
) -> Result<CustomerRefund> {
    ensure_submit_permission(rbac, actor, DocumentType::CustomerRefund, refund_id, executor).await?;
    let refund = ReturnsService::new(db.clone()).load_customer_refund(refund_id, executor).await?;
    refund.ensure_separation()?;
    let customer = db
        .customer_accounts()
        .find_by_id(&refund.customer_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("客户不存在".to_string()))?;
    customer_refund_responsible_org_id(customer.party_id.as_ref())?;
    Ok(refund)
}
