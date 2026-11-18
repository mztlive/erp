use erp_identity::AccessTarget;
use erp_permissions::returns;

use crate::ReverseFlowPorts;
mod commit;

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_core::ids::{SupplierAccountId, SupplierPaymentId};
use erp_finance::entity::payable::SupplierPaymentStatus;
use erp_finance::repository::PayableExt;
use erp_read_models::returns_center::dto::PaymentReversalView;
use erp_returns::dto::{
    CancelPaymentReversalApprovalRequest, CommitPaymentReversalRequest, CreatePaymentReversalRequest,
    SubmitPaymentReversalRequest,
};
use erp_returns::entity::returns::PaymentReversal;
use erp_returns::service::ReturnsService;
use erp_returns::service::approval::start_payment_reversal_approval;
use erp_returns::service::payment_reversal::{
    PaymentReversalSourceFact, new_payment_reversal, new_payment_reversal_commit,
};
use erp_returns::service::shared::ensure_posted_source;
use erp_returns::service::version_conflict::conflict_if_stale_version;
use erp_supplier::SupplierExt;
use erp_workflow::DocumentRegistryExt;
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use erp_workflow::service::approval::binding::{BindPublishedDefinitionCommand, attach_published_binding};
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::{prepare_cancel, prepare_start};
use erp_workflow::service::document_registry::{find_approval_binding, new_registered_document};
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::ReturnsProcess;
use super::adapter::{
    build_payment_reversal_snapshot, execute_payment_reversal_domain_action, payment_reversal_adapter,
    payment_reversal_object_readable, payment_reversal_responsible_org_id, payment_reversal_subject_ref,
    require_payment_reversal_binding,
};
use super::cancel_approval::{
    PaymentReversalCancelPersistInput, build_payment_reversal_cancel_input, load_cancel_runtime,
    persist_payment_reversal_cancel,
};
use super::start_approval::{
    PaymentReversalStartPersistInput, ReversalSubmitReplayInput, ReverseStartInput,
    build_payment_reversal_start_input, load_payment_reversal_start_receipt, load_start_graph,
    persist_payment_reversal_start, reversal_result_read_error,
};
use crate::{Error, Result};

impl ReturnsProcess {
    // -----------------------------------------------------------------------
    // 付款冲正
    // -----------------------------------------------------------------------

    /// 登记付款冲正草稿，并在同一事务绑定已发布审批定义。
    ///
    /// 冲正单号全局唯一（唯一索引）构成幂等去重。经办人与复核人必须不同。
    /// 绑定失败必须回滚业务实体，不得把绑定推迟到提交。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建冲正单视图。
    ///
    /// # 错误
    /// * `ConflictError` - 冲正单号重复或流程未配置
    /// * `NotFound` - 原付款或供应商不存在
    pub async fn create_payment_reversal(
        &self,
        req: CreatePaymentReversalRequest,
        actor: &AuditActor,
    ) -> Result<PaymentReversalView> {
        let process = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.create_payment_reversal_stage(req, &actor, executor).await })
            })
            .await
    }

    /// 原创建事务内先授权，再读取真实往来主体、绑定定义并原子登记。
    async fn create_payment_reversal_stage(
        &self,
        req: CreatePaymentReversalRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PaymentReversalView> {
        self.rbac
            .identity
            .require(
                actor,
                &returns::PAYMENT_REVERSAL_PAYMENT_REVERSAL_CREATE,
                AccessTarget::Operation,
                executor,
            )
            .await?;

        req.validate()?;
        let reversal = new_payment_reversal(req, actor.id())?;
        persist_created_payment_reversal(&self.db, &self.rbac, reversal.clone(), actor.clone(), executor)
            .await?;

        self.payment_reversal_result_read(
            &reversal.base.id,
            actor,
            &returns::PAYMENT_REVERSAL_PAYMENT_REVERSAL_CREATE,
            AccessTarget::Operation,
            executor,
        )
        .await
    }

    /// 按原顺序读取原付款及版本，准备一次创建提交所需的冲正事实。
    async fn payment_reversal_commit_source(
        &self,
        req: &CommitPaymentReversalRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(SupplierPaymentId, u64, PaymentReversal)> {
        let payment = self
            .db
            .supplier_payments()
            .find_by_id(&req.source_fact_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原供应商付款不存在".to_string()))?;
        let source_fact_id = SupplierPaymentId::new(payment.base.id.clone());
        let source_version = payment.base.version;
        let reversal = new_payment_reversal_commit(
            req,
            PaymentReversalSourceFact { payment_id: source_fact_id.clone(), amount: payment.amount },
            actor.id(),
        )?;
        Ok((source_fact_id, source_version, reversal))
    }

    /// 提交付款冲正并调用统一 `start_approval`。
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
    /// * `NotFound` - 冲正单或原付款不存在
    /// * `ConflictError` - 非草稿、无绑定或并发冲突
    pub async fn submit_payment_reversal(
        &self,
        id: &str,
        req: SubmitPaymentReversalRequest,
        actor: &AuditActor,
    ) -> Result<PaymentReversalView> {
        req.validate()?;
        let submission = ReversalSubmitReplayInput {
            id: id.to_string(),
            document_type: DocumentType::PaymentReversal,
            command_receipt: req.command_receipt(id, actor.id())?,
            actor: actor.clone(),
        };

        let process = self.clone();
        let command = submission.clone();
        let persisted = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.submit_payment_reversal_stage(req, &command, executor).await })
            })
            .await;

        self.finish_reversal_submit(&submission, persisted).await?;
        self.payment_reversal_result(
            &submission.id,
            actor,
            &returns::PAYMENT_REVERSAL_PAYMENT_REVERSAL_SUBMIT,
            AccessTarget::record(&submission.id)?,
        )
        .await
        .map_err(reversal_result_read_error)
    }

    /// 原提交事务先授权与匹配收据，再读取版本、来源、定义和人员并写入。
    async fn submit_payment_reversal_stage(
        &self,
        req: SubmitPaymentReversalRequest,
        submission: &ReversalSubmitReplayInput,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if self.replay_reversal_submit_stage(submission, executor).await?.is_some() {
            return Ok(());
        }

        let id = submission.id.as_str();
        let adapter = payment_reversal_adapter()?;
        let mut reversal = self.domain().load_payment_reversal(id, executor).await?;
        conflict_if_stale_version(reversal.matches_version(req.expected_version))?;
        start_payment_reversal_approval(&mut reversal)?;
        self.dispatch_payment_reversal_start(
            reversal,
            req.idempotency_key,
            req.resubmission_reason,
            adapter,
            submission.clone(),
            executor,
        )
        .await
    }

    /// 撤回付款冲正审批，成功后回到草稿且 `subject_version` 不回退。
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
    pub async fn cancel_payment_reversal_approval(
        &self,
        id: &str,
        req: CancelPaymentReversalApprovalRequest,
        actor: &AuditActor,
    ) -> Result<PaymentReversalView> {
        let process = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process.cancel_payment_reversal_approval_stage(&id, req, &actor, executor).await
                })
            })
            .await
    }

    /// 原撤回事务内先授权，再读取精确版本及审批事实并写入。
    async fn cancel_payment_reversal_approval_stage(
        &self,
        id: &str,
        req: CancelPaymentReversalApprovalRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PaymentReversalView> {
        self.rbac
            .identity
            .require(
                actor,
                &returns::PAYMENT_REVERSAL_PAYMENT_REVERSAL_CANCEL_APPROVAL,
                AccessTarget::Operation,
                executor,
            )
            .await?;

        req.validate()?;
        let mut reversal = self.domain().load_payment_reversal(id, executor).await?;
        conflict_if_stale_version(reversal.matches_version(req.expected_version))?;
        self.persist_cancelled_payment_reversal(id, &mut reversal, &req, actor, executor).await?;

        self.payment_reversal_result_read(
            id,
            actor,
            &returns::PAYMENT_REVERSAL_PAYMENT_REVERSAL_CANCEL_APPROVAL,
            AccessTarget::Operation,
            executor,
        )
        .await
    }

    /// 从绑定读取定义并持久化启动事实。
    ///
    /// # 错误
    /// 无绑定、定义缺失或写入失败时返回错误。
    async fn dispatch_payment_reversal_start(
        &self,
        reversal: PaymentReversal,
        idempotency_key: String,
        resubmission_reason: Option<String>,
        adapter: super::adapter::PaymentReversalAdapter,
        submission: ReversalSubmitReplayInput,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let id = submission.id.as_str();
        let actor = &submission.actor;
        let subject = payment_reversal_subject_ref(id)?;
        let binding = find_approval_binding(&self.db, id, executor).await.map_err(crate::Error::from)?;
        let binding = require_payment_reversal_binding(binding.as_ref())?.clone();

        let now = Instant::now();
        let (organization_id, supplier_id) =
            load_payment_reversal_context(&self.db, &reversal.original_supplier_payment_id, executor).await?;
        let snapshot =
            build_payment_reversal_snapshot(&reversal, &organization_id, &supplier_id, actor.id(), now)?;
        let _ = payment_reversal_object_readable(&organization_id, actor.id())?;

        let version = reversal.approval_subject_version;
        let existing_receipt =
            load_payment_reversal_start_receipt(&self.db, &subject, version, &idempotency_key, executor)
                .await?;
        let graph =
            load_start_graph(&self.db, &self.rbac, &binding, actor.id(), existing_receipt.as_ref(), executor)
                .await?;
        let prepared = prepare_start(build_payment_reversal_start_input(ReverseStartInput {
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

        persist_payment_reversal_start(
            &self.db,
            &self.rbac,
            PaymentReversalStartPersistInput {
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

    /// 加载撤回运行事实并写回草稿。
    ///
    /// # 错误
    /// 无绑定、实例终态或写入失败时返回错误。
    async fn persist_cancelled_payment_reversal(
        &self,
        id: &str,
        reversal: &mut PaymentReversal,
        req: &CancelPaymentReversalApprovalRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let adapter = payment_reversal_adapter()?;
        let binding = find_approval_binding(&self.db, id, executor).await?;
        let binding = require_payment_reversal_binding(binding.as_ref())?.clone();
        let subject = payment_reversal_subject_ref(id)?;
        let runtime =
            load_cancel_runtime(&self.db, &binding, &subject, reversal.approval_subject_version, executor)
                .await?;

        let now = Instant::now();
        let idempotency_key = normalize_idempotency_key(&req.idempotency_key)?;
        let input = build_payment_reversal_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &idempotency_key,
            None,
            now,
        )?;
        let prepared = prepare_cancel(input)?;

        execute_payment_reversal_domain_action(reversal, adapter.cancel_action)?;
        let audit = actor.clone().resource_log(
            "payment_reversal.cancel_approval",
            "payment_reversal",
            id.to_string(),
        )?;

        persist_payment_reversal_cancel(
            &self.db,
            &self.rbac,
            PaymentReversalCancelPersistInput {
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

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------
}

/// 在创建事务内写入冲正单、绑定发布定义并登记单据。
///
/// 绑定失败必须回滚业务实体，不得留下以后补流程的单据。
///
/// # 错误
/// 无发布定义、人员重验失败或写入失败时返回错误。
async fn persist_created_payment_reversal(
    db: &Database,
    rbac: &ReverseFlowPorts,
    reversal: PaymentReversal,
    actor: AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    rbac.identity
        .require(
            &actor,
            &returns::PAYMENT_REVERSAL_PAYMENT_REVERSAL_CREATE,
            AccessTarget::Operation,
            executor,
        )
        .await?;

    let (organization_id, _) =
        load_payment_reversal_context(db, &reversal.original_supplier_payment_id, executor).await?;
    let bind_command = BindPublishedDefinitionCommand {
        document_type: DocumentType::PaymentReversal,
        business_object_id: reversal.base.id.clone(),
        business_object_version: reversal.base.version,
        context: BindingRevalidationContext::new(organization_id, actor.id().to_string()),
    };
    let document = new_registered_document(
        &reversal.base.id,
        DocumentType::PaymentReversal,
        reversal.reversal_no.clone(),
    )
    .map_err(crate::Error::from)?;
    let audit = actor.clone().resource_log(
        "payment_reversal.create",
        "payment_reversal",
        reversal.base.id.clone(),
    )?;

    persist_bound_payment_reversal_document(db, rbac, document, &bind_command, &actor, executor).await?;
    ReturnsService::new(db.clone()).create_payment_reversal(&reversal, executor).await?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}

/// 查询原付款供应商往来主体作为责任组织，并带回供应商。
///
/// # 错误
/// 原付款或供应商不存在、往来主体为空时返回错误。
async fn load_payment_reversal_context(
    db: &Database,
    original_payment_id: &SupplierPaymentId,
    executor: &mut dyn Executor,
) -> Result<(String, SupplierAccountId)> {
    let payment = db
        .supplier_payments()
        .find_by_id(original_payment_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原付款不存在".to_string()))?;
    let supplier = db
        .supplier_accounts()
        .find_by_id(&payment.supplier_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
    let organization_id = payment_reversal_responsible_org_id(supplier.party_id.as_ref())?;
    Ok((organization_id, payment.supplier_id))
}

/// 在创建冲正单的同一事务中重读并校验原供应商付款事实。
///
/// 原付款必须仍为调用前读取的版本且已经过账，避免为非正式事实创建审批任务。
async fn validate_payment_reversal_source(
    db: &Database,
    source_fact_id: &SupplierPaymentId,
    expected_version: u64,
    executor: &mut dyn Executor,
) -> Result<()> {
    let payment = db
        .supplier_payments()
        .find_by_id(source_fact_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原供应商付款不存在".to_string()))?;
    Ok(ensure_posted_source(
        payment.base.version,
        expected_version,
        payment.status == SupplierPaymentStatus::Posted,
        "只有已过账的供应商付款才能发起冲正",
    )?)
}

/// 查询发布定义、写入绑定并持久化注册行。
///
/// # 错误
/// 无发布定义或绑定失败时返回错误。
async fn persist_bound_payment_reversal_document(
    db: &Database,
    rbac: &ReverseFlowPorts,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding> {
    let _ = payment_reversal_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let binding = rbac.runtime.bind_document(db, bind_command, actor, executor).await?;
    let binding = binding.ok_or_else(|| Error::Internal("付款冲正单必须绑定已发布定义".to_string()))?;
    attach_published_binding(&mut document, binding.clone())?;
    db.business_documents().create(&document, executor).await?;
    Ok(binding)
}

#[cfg(test)]
mod authorization_tests {
    use persistence_core::testing::SnapshotExecutor;

    use super::super::testing::{actor, denied, process};

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
                    .cancel_payment_reversal_approval_stage(
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
mod creation_authorization_tests {
    use erp_core::common::time::Instant;
    use persistence_core::testing::SnapshotExecutor;

    use super::super::testing::{actor, denied, process};

    #[tokio::test]
    async fn creation_authorizes_before_counterparty_source_or_definition_reads() {
        let process = process().await;
        let request=serde_json::from_value(serde_json::json!({
"refund_no":"RF-1","reversal_no":"RV-1","customer_id":"unread-customer","supplier_id":"unread-supplier",
"original_customer_receipt_id":"unread-receipt","original_supplier_payment_id":"unread-payment",
"reason_text":"reason","amount":"1","handled_by":"actor","reviewed_by":"reviewer","occurred_at":Instant::from_unix_secs(100)
})).unwrap();
        denied(
            process.create_payment_reversal_stage(request, &actor(), &mut SnapshotExecutor::default()).await,
        );
    }
}

#[cfg(test)]
mod submission_authorization_tests {
    use persistence_core::testing::SnapshotExecutor;

    use super::super::testing::{actor, denied, process};
    use super::*;

    #[tokio::test]
    async fn submission_authorizes_before_receipt_versions_sources_definitions_or_people() {
        let process = process().await;
        let actor = actor();
        for expected_version in [1, 999] {
            let req: SubmitPaymentReversalRequest = serde_json::from_value(serde_json::json!({
                "expected_version": expected_version,
                "idempotency_key": "submit-command"
            }))
            .unwrap();
            let command = ReversalSubmitReplayInput {
                id: "unread-record".into(),
                document_type: DocumentType::PaymentReversal,
                command_receipt: req.command_receipt("unread-record", actor.id()).unwrap(),
                actor: actor.clone(),
            };
            denied(
                process.submit_payment_reversal_stage(req, &command, &mut SnapshotExecutor::default()).await,
            );
        }
    }
}
