//! 回款冲正提交与撤回审批；保留原版本、绑定、回执及运行事实的先后顺序。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_identity::AccessTarget;
use erp_permissions::returns;
use erp_read_models::returns_center::dto::ReceiptReversalView;
use erp_returns::dto::{CancelReceiptReversalApprovalRequest, SubmitReceiptReversalRequest};
use erp_returns::entity::returns::ReceiptReversal;
use erp_returns::service::receipt_reversal::{
    ensure_receipt_reversal_version, prepare_receipt_reversal_submit,
};
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshotPayload;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::{prepare_cancel, prepare_start};
use erp_workflow::service::document_registry::find_approval_binding;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::super::ReturnsProcess;
use super::super::adapter::{
    build_receipt_reversal_snapshot, execute_receipt_reversal_domain_action, receipt_reversal_adapter,
    receipt_reversal_object_readable, receipt_reversal_subject_ref, require_receipt_reversal_binding,
};
use super::super::cancel_approval::{
    ReceiptReversalCancelPersistInput, build_receipt_reversal_cancel_input, load_cancel_runtime,
    persist_receipt_reversal_cancel,
};
use super::super::start_approval::{
    ReceiptReversalStartPersistInput, ReversalSubmitReplayInput, ReverseStartInput,
    build_receipt_reversal_start_input, load_receipt_reversal_start_receipt, load_start_graph,
    persist_receipt_reversal_start, reversal_result_read_error,
};
use super::context::load_receipt_reversal_context;
use crate::Result;

impl ReturnsProcess {
    /// 提交回款冲正并调用统一 `start_approval`。
    ///
    /// 按合同 §4.4.1 冻结 `approval_subject_version` 与 `subject_snapshot`，单据进入 `IN_APPROVAL`。定义与审批人取自已绑定事实，不接受客户端选择。
    ///
    /// `id` 为冲正单主键；`req` 为提交请求（版本与幂等键）。返回提交后的冲正单视图。`NotFound`：冲正单或原回款不存在。
    ///
    /// `ConflictError`：非草稿、无绑定或并发冲突。
    pub async fn submit_receipt_reversal(
        &self,
        id: &str,
        req: SubmitReceiptReversalRequest,
        actor: &AuditActor,
    ) -> Result<ReceiptReversalView> {
        req.validate()?;
        let submission = ReversalSubmitReplayInput {
            id: id.to_string(),
            document_type: DocumentType::ReceiptReversal,
            command_receipt: req.command_receipt(id, actor.id())?,
            actor: actor.clone(),
        };

        let process = self.clone();
        let command = submission.clone();
        let persisted = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.submit_receipt_reversal_stage(req, &command, executor).await })
            })
            .await;

        self.finish_reversal_submit(&submission, persisted).await?;
        self.receipt_reversal_result(
            &submission.id,
            actor,
            &returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_SUBMIT,
            AccessTarget::record(&submission.id)?,
        )
        .await
        .map_err(reversal_result_read_error)
    }

    // 原提交事务先授权与匹配收据，再读取版本、来源、定义和人员并写入。
    async fn submit_receipt_reversal_stage(
        &self,
        req: SubmitReceiptReversalRequest,
        submission: &ReversalSubmitReplayInput,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if self.replay_reversal_submit_stage(submission, executor).await?.is_some() {
            return Ok(());
        }

        let id = submission.id.as_str();
        let adapter = receipt_reversal_adapter()?;
        let mut reversal = self.domain().load_receipt_reversal(id, executor).await?;
        prepare_receipt_reversal_submit(&mut reversal, req.expected_version)?;
        self.dispatch_receipt_reversal_start(
            reversal,
            req.idempotency_key,
            req.resubmission_reason,
            adapter,
            submission.clone(),
            executor,
        )
        .await
    }

    /// 撤回回款冲正审批，成功后回到草稿且 `subject_version` 不回退。
    ///
    /// 作为合同 `cancel_action`，供业务撤回与管理员受阻取消共用。
    ///
    /// `id` 为冲正单主键；`req` 为撤回请求（原因必填）。返回撤回后的冲正单视图。`NotFound`：冲正单不存在。
    ///
    /// `ConflictError`：非审批中、已最终通过或并发冲突。
    pub async fn cancel_receipt_reversal_approval(
        &self,
        id: &str,
        req: CancelReceiptReversalApprovalRequest,
        actor: &AuditActor,
    ) -> Result<ReceiptReversalView> {
        let process = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process.cancel_receipt_reversal_approval_stage(&id, req, &actor, executor).await
                })
            })
            .await
    }

    // 原撤回事务内先授权，再读取精确版本及审批事实并写入。
    async fn cancel_receipt_reversal_approval_stage(
        &self,
        id: &str,
        req: CancelReceiptReversalApprovalRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ReceiptReversalView> {
        self.rbac
            .identity
            .require(
                actor,
                &returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_CANCEL_APPROVAL,
                AccessTarget::Operation,
                executor,
            )
            .await?;

        req.validate()?;
        let mut reversal = self.domain().load_receipt_reversal(id, executor).await?;
        ensure_receipt_reversal_version(&reversal, req.expected_version)?;
        self.persist_cancelled_receipt_reversal(id, &mut reversal, &req, actor, executor).await?;

        self.receipt_reversal_result_read(
            id,
            actor,
            &returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_CANCEL_APPROVAL,
            AccessTarget::Operation,
            executor,
        )
        .await
    }

    // 从绑定读取定义并持久化启动事实。无绑定、定义缺失或写入失败时返回错误。
    async fn dispatch_receipt_reversal_start(
        &self,
        reversal: ReceiptReversal,
        idempotency_key: String,
        resubmission_reason: Option<String>,
        adapter: super::super::adapter::ReceiptReversalAdapter,
        submission: ReversalSubmitReplayInput,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let id = submission.id.as_str();
        let actor = &submission.actor;
        let subject = receipt_reversal_subject_ref(id)?;
        let binding = find_approval_binding(&self.db, id, executor).await.map_err(crate::Error::from)?;
        let binding = require_receipt_reversal_binding(binding.as_ref())?.clone();

        let now = Instant::now();
        let (organization_id, snapshot) =
            self.receipt_reversal_start_snapshot(&reversal, actor, now, executor).await?;
        let _ = receipt_reversal_object_readable(&organization_id, actor.id())?;

        let version = reversal.approval_subject_version;
        let existing_receipt =
            load_receipt_reversal_start_receipt(&self.db, &subject, version, &idempotency_key, executor)
                .await?;
        let graph =
            load_start_graph(&self.db, &self.rbac, &binding, actor.id(), existing_receipt.as_ref(), executor)
                .await?;
        let prepared = prepare_start(build_receipt_reversal_start_input(ReverseStartInput {
            resubmission_reason: resubmission_reason.clone(),
            graph,
            binding: &binding,
            subject,
            subject_version: reversal.approval_subject_version,
            actor_id: actor.id(),
            organization_id: &organization_id,
            idempotency_key: &idempotency_key,
            receipt: existing_receipt,
            now,
        })?)?;

        persist_receipt_reversal_start(
            &self.db,
            &self.rbac,
            ReceiptReversalStartPersistInput {
                resubmission_reason,
                reversal,
                command_receipt: submission.command_receipt.clone(),
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

    // 沿原回款冻结提交组织、客户和单据快照。
    async fn receipt_reversal_start_snapshot(
        &self,
        reversal: &ReceiptReversal,
        actor: &AuditActor,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<(String, ApprovalSubjectSnapshotPayload)> {
        let (organization_id, customer_id) =
            load_receipt_reversal_context(&self.db, &reversal.original_customer_receipt_id, executor).await?;
        let snapshot = build_receipt_reversal_snapshot(
            reversal,
            &organization_id,
            customer_id.as_ref(),
            actor.id(),
            now,
        )?;
        Ok((organization_id, snapshot))
    }

    // 加载撤回运行事实并写回草稿。无绑定、实例终态或写入失败时返回错误。
    async fn persist_cancelled_receipt_reversal(
        &self,
        id: &str,
        reversal: &mut ReceiptReversal,
        req: &CancelReceiptReversalApprovalRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let adapter = receipt_reversal_adapter()?;
        let binding = find_approval_binding(&self.db, id, executor).await?;
        let binding = require_receipt_reversal_binding(binding.as_ref())?.clone();
        let subject = receipt_reversal_subject_ref(id)?;
        let runtime =
            load_cancel_runtime(&self.db, &binding, &subject, reversal.approval_subject_version, executor)
                .await?;

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
            &self.rbac,
            ReceiptReversalCancelPersistInput {
                reversal: reversal.clone(),
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

#[cfg(test)]
mod authorization_tests {
    use persistence_core::testing::SnapshotExecutor;

    use super::super::super::testing::{actor, denied, process};

    #[tokio::test]
    async fn cancellation_authorizes_before_original_versions_bindings_or_runtime_reads() {
        let process = process().await;
        for version in [1, 999] {
            let request = serde_json::from_value(
                serde_json::json!({"expected_version":version,"reason":"cancel","idempotency_key":"key"}),
            )
            .unwrap();
            denied(
                process
                    .cancel_receipt_reversal_approval_stage(
                        "unread-record",
                        request,
                        &actor(),
                        &mut SnapshotExecutor::default(),
                    )
                    .await,
            );
        }
    }
}

#[cfg(test)]
mod submission_authorization_tests {
    use persistence_core::testing::SnapshotExecutor;

    use super::super::super::testing::{actor, denied, process};
    use super::*;

    #[tokio::test]
    async fn submission_authorizes_before_receipt_versions_sources_definitions_or_people() {
        let process = process().await;
        let actor = actor();
        for expected_version in [1, 999] {
            let req: SubmitReceiptReversalRequest = serde_json::from_value(serde_json::json!({
                "expected_version": expected_version,
                "idempotency_key": "submit-command"
            }))
            .unwrap();
            let command = ReversalSubmitReplayInput {
                id: "unread-record".into(),
                document_type: DocumentType::ReceiptReversal,
                command_receipt: req.command_receipt("unread-record", actor.id()).unwrap(),
                actor: actor.clone(),
            };
            denied(
                process.submit_receipt_reversal_stage(req, &command, &mut SnapshotExecutor::default()).await,
            );
        }
    }
}
