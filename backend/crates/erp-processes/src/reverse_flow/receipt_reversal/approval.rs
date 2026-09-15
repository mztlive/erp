//! 回款冲正提交与撤回审批；保留原版本、绑定、回执及运行事实的先后顺序。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_read_models::returns_center::dto::ReceiptReversalView;
use erp_returns::dto::{CancelReceiptReversalApprovalRequest, SubmitReceiptReversalRequest};
use erp_returns::entity::returns::ReceiptReversal;
use erp_returns::service::receipt_reversal::{
    ensure_receipt_reversal_version, prepare_receipt_reversal_submit,
};
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::{prepare_cancel, prepare_start};
use erp_workflow::service::document_registry::find_approval_binding;
use persistence_core::NoTransaction;
use validator::Validate;

use super::super::ReturnsProcess;
use super::super::adapter::{
    build_receipt_reversal_snapshot, execute_receipt_reversal_domain_action, receipt_reversal_adapter,
    receipt_reversal_object_readable, receipt_reversal_start_command, receipt_reversal_start_command_kind,
    receipt_reversal_subject_ref, require_receipt_reversal_binding,
};
use super::super::cancel_approval::{
    ReceiptReversalCancelPersistInput, build_receipt_reversal_cancel_input, load_cancel_runtime,
    persist_receipt_reversal_cancel,
};
use super::super::start_approval::{
    ReceiptReversalStartInput, ReceiptReversalStartPersistInput, build_receipt_reversal_start_input,
    load_bound_definition_graph, load_receipt_reversal_start_receipt, persist_receipt_reversal_start,
};
use super::context::load_receipt_reversal_context;
use crate::Result;

impl ReturnsProcess {
    /// 提交回款冲正并调用统一 `start_approval`。
    ///
    /// 按合同 §4.4.1 冻结 `approval_subject_version` 与 `subject_snapshot`，
    /// 单据进入 `IN_APPROVAL`。定义与审批人取自已绑定事实，不接受客户端选择。
    ///
    /// # 参数
    /// * `id` - 冲正单主键
    /// * `req` - 提交请求（版本与幂等键）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回提交后的冲正单视图。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单或原回款不存在
    /// * `ConflictError` - 非草稿、无绑定或并发冲突
    pub async fn submit_receipt_reversal(
        &self,
        id: &str,
        req: SubmitReceiptReversalRequest,
        actor: &AuditActor,
    ) -> Result<ReceiptReversalView> {
        req.validate()?;
        let adapter = receipt_reversal_adapter()?;
        let mut reversal = self.domain().load_receipt_reversal(id).await?;
        prepare_receipt_reversal_submit(&mut reversal, req.expected_version)?;
        self.dispatch_receipt_reversal_start(id, reversal, req.idempotency_key, actor, adapter).await
    }

    /// 撤回回款冲正审批，成功后回到草稿且 `subject_version` 不回退。
    ///
    /// 作为合同 `cancel_action`，供业务撤回与管理员受阻取消共用。
    ///
    /// # 参数
    /// * `id` - 冲正单主键
    /// * `req` - 撤回请求（原因必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回撤回后的冲正单视图。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单不存在
    /// * `ConflictError` - 非审批中、已最终通过或并发冲突
    pub async fn cancel_receipt_reversal_approval(
        &self,
        id: &str,
        req: CancelReceiptReversalApprovalRequest,
        actor: &AuditActor,
    ) -> Result<ReceiptReversalView> {
        req.validate()?;
        let mut reversal = self.domain().load_receipt_reversal(id).await?;
        ensure_receipt_reversal_version(&reversal, req.expected_version)?;
        self.persist_cancelled_receipt_reversal(id, &mut reversal, &req, actor).await?;
        self.reads().receipt_reversal_detail(id).await.map_err(crate::Error::from)
    }

    /// 从绑定读取定义并持久化启动事实。
    ///
    /// # 错误
    /// 无绑定、定义缺失或写入失败时返回错误。
    async fn dispatch_receipt_reversal_start(
        &self,
        id: &str,
        reversal: ReceiptReversal,
        idempotency_key: String,
        actor: &AuditActor,
        adapter: super::super::adapter::ReceiptReversalAdapter,
    ) -> Result<ReceiptReversalView> {
        let subject = receipt_reversal_subject_ref(id)?;
        let binding =
            find_approval_binding(&self.db, id, &mut NoTransaction).await.map_err(crate::Error::from)?;
        let binding = require_receipt_reversal_binding(binding.as_ref())?.clone();
        let now = Instant::now();
        let (organization_id, customer_id) =
            load_receipt_reversal_context(&self.db, &reversal.original_customer_receipt_id).await?;
        let snapshot = build_receipt_reversal_snapshot(
            &reversal,
            &organization_id,
            customer_id.as_ref(),
            actor.id(),
            now,
        )?;
        let start = receipt_reversal_start_command(
            id,
            reversal.approval_subject_version,
            actor.id(),
            &idempotency_key,
        );
        let _ = receipt_reversal_start_command_kind(&start);
        let _ = receipt_reversal_object_readable(&organization_id, actor.id())?;
        let graph = load_bound_definition_graph(&self.db, &binding).await?;
        let existing_receipt = load_receipt_reversal_start_receipt(
            &self.db,
            &subject,
            reversal.approval_subject_version,
            &idempotency_key,
        )
        .await?;
        let start_input = build_receipt_reversal_start_input(ReceiptReversalStartInput {
            graph,
            binding: &binding,
            subject,
            subject_version: reversal.approval_subject_version,
            actor_id: actor.id(),
            organization_id: &organization_id,
            idempotency_key: &idempotency_key,
            receipt: existing_receipt,
            now,
        })?;
        let prepared = prepare_start(start_input)?;
        persist_receipt_reversal_start(
            &self.db,
            ReceiptReversalStartPersistInput {
                reversal,
                actor: actor.clone(),
                id: id.to_string(),
                snapshot_payload: snapshot,
                prepared,
                owner_role: adapter.owner_role,
                organization_id,
                now,
            },
        )
        .await?;
        self.reads().receipt_reversal_detail(id).await.map_err(crate::Error::from)
    }

    /// 加载撤回运行事实并写回草稿。
    ///
    /// # 错误
    /// 无绑定、实例终态或写入失败时返回错误。
    async fn persist_cancelled_receipt_reversal(
        &self,
        id: &str,
        reversal: &mut ReceiptReversal,
        req: &CancelReceiptReversalApprovalRequest,
        actor: &AuditActor,
    ) -> Result<()> {
        let adapter = receipt_reversal_adapter()?;
        let binding =
            find_approval_binding(&self.db, id, &mut NoTransaction).await.map_err(crate::Error::from)?;
        let binding = require_receipt_reversal_binding(binding.as_ref())?.clone();
        let subject = receipt_reversal_subject_ref(id)?;
        let runtime =
            load_cancel_runtime(&self.db, &binding, &subject, reversal.approval_subject_version).await?;
        let now = Instant::now();
        let idempotency_key = normalize_idempotency_key(&req.idempotency_key)?;
        let input = build_receipt_reversal_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &idempotency_key,
            None,
            now,
        )?;
        let prepared = prepare_cancel(input)?;
        execute_receipt_reversal_domain_action(reversal, adapter.cancel_action)?;
        let audit = actor.clone().resource_log(
            "receipt_reversal.cancel_approval",
            "receipt_reversal",
            id.to_string(),
        )?;
        persist_receipt_reversal_cancel(
            &self.db,
            ReceiptReversalCancelPersistInput {
                reversal: reversal.clone(),
                prepared,
                open_tasks: runtime.open_tasks,
                actor_id: actor.id().to_string(),
                reason: req.reason.clone(),
                now,
                audit,
            },
        )
        .await
    }
}
