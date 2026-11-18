use erp_identity::AccessTarget;
use erp_permissions::returns;

use crate::ReverseFlowPorts;
mod commit;

use erp_audit::execution::persist_log;

mod commit_source;
mod posting;
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_core::ids::{SupplierAccountId, SupplierPaymentId};
use erp_finance::entity::payable::SupplierPaymentStatus;
use erp_finance::repository::PayableExt;
use erp_read_models::returns_center::dto::SupplierRefundView;
use erp_returns::dto::{
    CancelSupplierRefundApprovalRequest, CreateSupplierRefundRequest, SubmitSupplierRefundRequest,
};
use erp_returns::entity::returns::SupplierRefund;
use erp_returns::service::ReturnsService;
use erp_returns::service::approval::start_supplier_refund_approval;
use erp_returns::service::shared::ensure_posted_source;
use erp_returns::service::supplier_refund::{
    SupplierRefundSourceFact, new_supplier_refund, new_supplier_refund_commit,
};
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
pub(super) use posting::apply_supplier_refund_final_post;
use validator::Validate;

use super::ReturnsProcess;
use super::adapter::{
    build_supplier_refund_snapshot, execute_supplier_refund_domain_action, require_supplier_refund_binding,
    supplier_refund_adapter, supplier_refund_object_readable, supplier_refund_responsible_org_id,
    supplier_refund_subject_ref,
};
use super::cancel_approval::{
    SupplierRefundCancelPersistInput, build_supplier_refund_cancel_input, load_cancel_runtime,
    persist_supplier_refund_cancel,
};
use super::start_approval::{
    ReversalSubmitReplayInput, ReverseStartInput, SupplierRefundStartPersistInput,
    build_supplier_refund_start_input, ensure_submit_permission, load_start_graph,
    load_supplier_refund_start_receipt, persist_supplier_refund_start, reversal_result_read_error,
};
use crate::{Error, Result};

impl ReturnsProcess {
    // -----------------------------------------------------------------------
    // 供应商退款
    // -----------------------------------------------------------------------

    /// 登记供应商退款草稿，并在同一事务绑定已发布审批定义。
    ///
    /// 退款单号全局唯一（唯一索引）构成幂等去重。经办人与复核人必须不同。绑定失败必须回滚业务实体，不得把绑定推迟到提交。返回新建退款单视图。
    ///
    /// `ConflictError`：退款单号重复或流程未配置。
    pub async fn create_supplier_refund(
        &self,
        req: CreateSupplierRefundRequest,
        actor: &AuditActor,
    ) -> Result<SupplierRefundView> {
        let process = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.create_supplier_refund_stage(req, &actor, executor).await })
            })
            .await
    }

    // 单独创建供应商退款草稿时，在原事务先重验授权，再读取真实往来主体、绑定发布定义并登记。
    async fn create_supplier_refund_stage(
        &self,
        req: CreateSupplierRefundRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SupplierRefundView> {
        self.rbac
            .identity
            .require(
                actor,
                &returns::SUPPLIER_REFUND_SUPPLIER_REFUND_CREATE,
                AccessTarget::Operation,
                executor,
            )
            .await?;

        req.validate()?;
        let refund = new_supplier_refund(req, actor.id())?;
        persist_created_supplier_refund(&self.db, &self.rbac, refund.clone(), actor.clone(), executor)
            .await?;

        self.supplier_refund_result_read(
            &refund.base.id,
            actor,
            &returns::SUPPLIER_REFUND_SUPPLIER_REFUND_CREATE,
            AccessTarget::Operation,
            executor,
        )
        .await
    }

    /// 提交供应商退款并调用统一 `start_approval`。
    ///
    /// 按合同 §4.4.1 冻结 `approval_subject_version` 与 `subject_snapshot`，单据进入 `IN_APPROVAL`。定义与审批人取自已绑定事实，不接受客户端选择。
    ///
    /// `id` 为退款单主键；`req` 为提交请求（版本与幂等键）。返回提交后的退款单视图。`NotFound`：退款单或供应商不存在。
    ///
    /// `ConflictError`：非草稿、无绑定或并发冲突。
    pub async fn submit_supplier_refund(
        &self,
        id: &str,
        mut req: SubmitSupplierRefundRequest,
        actor: &AuditActor,
    ) -> Result<SupplierRefundView> {
        req.validate()?;
        req.idempotency_key = normalize_idempotency_key(&req.idempotency_key)?.as_str().to_string();
        let submission = ReversalSubmitReplayInput {
            id: id.to_owned(),
            document_type: DocumentType::SupplierRefund,
            command_receipt: req.command_receipt(id, actor.id())?,
            actor: actor.clone(),
        };

        let process = self.clone();
        let command = submission.clone();
        let persisted = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.submit_supplier_refund_stage(req, &command, executor).await })
            })
            .await;

        self.finish_reversal_submit(&submission, persisted).await?;
        self.supplier_refund_result(
            &submission.id,
            actor,
            &returns::SUPPLIER_REFUND_SUPPLIER_REFUND_SUBMIT,
            AccessTarget::record(&submission.id)?,
        )
        .await
        .map_err(reversal_result_read_error)
    }

    // 提交已有供应商退款时，在原事务先授权并匹配收据，未重放时才校验版本、来源、定义与人员并写入。
    async fn submit_supplier_refund_stage(
        &self,
        req: SubmitSupplierRefundRequest,
        submission: &ReversalSubmitReplayInput,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if self.replay_reversal_submit_stage(submission, executor).await?.is_some() {
            return Ok(());
        }

        let id = submission.id.as_str();
        let adapter = supplier_refund_adapter()?;
        let mut refund = self.domain().load_supplier_refund(id, executor).await?;
        conflict_if_stale_version(refund.matches_version(req.expected_version))?;
        start_supplier_refund_approval(&mut refund)?;
        self.dispatch_supplier_refund_start(
            refund,
            req.idempotency_key,
            req.resubmission_reason,
            adapter,
            submission.clone(),
            executor,
        )
        .await
    }

    /// 撤回供应商退款审批，成功后回到草稿且 `subject_version` 不回退。
    ///
    /// 作为合同 `cancel_action`，供业务撤回与管理员受阻取消共用。
    ///
    /// `id` 为退款单主键；`req` 为撤回请求（原因必填）。返回撤回后的退款单视图。`NotFound`：退款单不存在。
    ///
    /// `ConflictError`：非审批中、已最终通过或并发冲突。
    pub async fn cancel_supplier_refund_approval(
        &self,
        id: &str,
        req: CancelSupplierRefundApprovalRequest,
        actor: &AuditActor,
    ) -> Result<SupplierRefundView> {
        let process = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process.cancel_supplier_refund_approval_stage(&id, req, &actor, executor).await
                })
            })
            .await
    }

    // 撤回供应商退款审批时，在原事务先重验授权，再检查精确版本和审批运行事实后写入撤回。
    async fn cancel_supplier_refund_approval_stage(
        &self,
        id: &str,
        req: CancelSupplierRefundApprovalRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SupplierRefundView> {
        self.rbac
            .identity
            .require(
                actor,
                &returns::SUPPLIER_REFUND_SUPPLIER_REFUND_CANCEL_APPROVAL,
                AccessTarget::Operation,
                executor,
            )
            .await?;

        req.validate()?;
        let mut refund = self.domain().load_supplier_refund(id, executor).await?;
        conflict_if_stale_version(refund.matches_version(req.expected_version))?;
        self.persist_cancelled_supplier_refund(id, &mut refund, &req, actor, executor).await?;

        self.supplier_refund_result_read(
            id,
            actor,
            &returns::SUPPLIER_REFUND_SUPPLIER_REFUND_CANCEL_APPROVAL,
            AccessTarget::Operation,
            executor,
        )
        .await
    }

    // 从绑定读取定义并持久化启动事实。无绑定、定义缺失或写入失败时返回错误。
    async fn dispatch_supplier_refund_start(
        &self,
        refund: SupplierRefund,
        idempotency_key: String,
        resubmission_reason: Option<String>,
        adapter: super::adapter::SupplierRefundAdapter,
        submission: ReversalSubmitReplayInput,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let id = submission.id.as_str();
        let actor = &submission.actor;
        let subject = supplier_refund_subject_ref(id)?;
        let binding = find_approval_binding(&self.db, id, executor).await.map_err(crate::Error::from)?;
        let binding = require_supplier_refund_binding(binding.as_ref())?.clone();

        let now = Instant::now();
        let organization_id = load_supplier_refund_org_id(&self.db, &refund.supplier_id, executor).await?;
        let snapshot = build_supplier_refund_snapshot(&refund, &organization_id, actor.id(), now)?;

        let existing_receipt = load_supplier_refund_start_receipt(
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
        let prepared = prepare_start(build_supplier_refund_start_input(ReverseStartInput {
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

        persist_supplier_refund_start(
            &self.db,
            &self.rbac,
            SupplierRefundStartPersistInput {
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

    // 加载撤回运行事实并写回草稿。无绑定、实例终态或写入失败时返回错误。
    async fn persist_cancelled_supplier_refund(
        &self,
        id: &str,
        refund: &mut SupplierRefund,
        req: &CancelSupplierRefundApprovalRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let adapter = supplier_refund_adapter()?;
        let binding = find_approval_binding(&self.db, id, executor).await?;
        let binding = require_supplier_refund_binding(binding.as_ref())?.clone();
        let subject = supplier_refund_subject_ref(id)?;
        let runtime =
            load_cancel_runtime(&self.db, &binding, &subject, refund.approval_subject_version, executor)
                .await?;

        let now = Instant::now();
        let idempotency_key = normalize_idempotency_key(&req.idempotency_key)?;
        let input = build_supplier_refund_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &idempotency_key,
            None,
            now,
        )?;
        let prepared = prepare_cancel(input)?;

        execute_supplier_refund_domain_action(refund, adapter.cancel_action)?;
        let audit = actor.clone().resource_log(
            "supplier_refund.cancel_approval",
            "supplier_refund",
            id.to_string(),
        )?;

        persist_supplier_refund_cancel(
            &self.db,
            &self.rbac,
            SupplierRefundCancelPersistInput {
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

// 在创建事务内写入退款单、绑定发布定义并登记单据。
//
// 绑定失败必须回滚业务实体，不得留下以后补流程的单据。无发布定义、人员重验失败或写入失败时返回错误。
async fn persist_created_supplier_refund(
    db: &Database,
    rbac: &ReverseFlowPorts,
    refund: SupplierRefund,
    actor: AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    rbac.identity
        .require(&actor, &returns::SUPPLIER_REFUND_SUPPLIER_REFUND_CREATE, AccessTarget::Operation, executor)
        .await?;

    let organization_id = load_supplier_refund_org_id(db, &refund.supplier_id, executor).await?;
    let bind_command = BindPublishedDefinitionCommand {
        document_type: DocumentType::SupplierRefund,
        business_object_id: refund.base.id.clone(),
        business_object_version: refund.base.version,
        context: BindingRevalidationContext::new(organization_id, actor.id().to_string()),
    };
    let document =
        new_registered_document(&refund.base.id, DocumentType::SupplierRefund, refund.refund_no.clone())
            .map_err(crate::Error::from)?;
    let audit =
        actor.clone().resource_log("supplier_refund.create", "supplier_refund", refund.base.id.clone())?;

    persist_bound_supplier_refund_document(db, rbac, document, &bind_command, &actor, executor).await?;
    ReturnsService::new(db.clone()).create_supplier_refund(&refund, executor).await?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}

// 查询供应商往来主体作为责任组织。供应商不存在或往来主体为空时返回错误。
async fn load_supplier_refund_org_id(
    db: &Database,
    supplier_id: &SupplierAccountId,
    executor: &mut dyn Executor,
) -> Result<String> {
    let supplier = db
        .supplier_accounts()
        .find_by_id(supplier_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
    supplier_refund_responsible_org_id(supplier.party_id.as_ref())
}

// 在创建退款的同一事务中重读并校验原供应商付款事实。
//
// 原事实必须仍为调用前读取的版本且已经过账，避免从草稿、审批中或已冲正付款
// 派生无法最终执行的退款审批。
async fn validate_supplier_refund_source(
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
        "只有已过账的供应商付款才能发起退款",
    )?)
}

// 查询发布定义、写入绑定并持久化注册行。无发布定义或绑定失败时返回错误。
async fn persist_bound_supplier_refund_document(
    db: &Database,
    rbac: &ReverseFlowPorts,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding> {
    let _ = supplier_refund_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let binding = rbac.runtime.bind_document(db, bind_command, actor, executor).await?;
    let binding = binding.ok_or_else(|| Error::Internal("供应商退款单必须绑定已发布定义".to_string()))?;
    attach_published_binding(&mut document, binding.clone())?;
    db.business_documents().create(&document, executor).await?;
    Ok(binding)
}

// 先证明账号和提交资格，再读取退款并核验资金来源，沿用调用方事务。
//
// `db` / `rbac` 为数据库与审批授权端口；`actor` / `refund_id` 为当前操作人与退款单主键。返回完成原来源授权重验的退款单。
//
// 原单或往来主体不存在、来源无权限或提交资格失效时返回错误。
pub(super) async fn authorized_supplier_refund(
    db: &Database,
    rbac: &ReverseFlowPorts,
    actor: &AuditActor,
    refund_id: &str,
    executor: &mut dyn Executor,
) -> Result<SupplierRefund> {
    ensure_submit_permission(rbac, actor, DocumentType::SupplierRefund, refund_id, executor).await?;
    let refund = ReturnsService::new(db.clone()).load_supplier_refund(refund_id, executor).await?;
    refund.ensure_separation()?;
    let supplier = db
        .supplier_accounts()
        .find_by_id(&refund.supplier_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
    supplier_refund_responsible_org_id(supplier.party_id.as_ref())?;
    Ok(refund)
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
                    .cancel_supplier_refund_approval_stage(
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
            process.create_supplier_refund_stage(request, &actor(), &mut SnapshotExecutor::default()).await,
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
            let req: SubmitSupplierRefundRequest = serde_json::from_value(serde_json::json!({
                "expected_version": expected_version,
                "idempotency_key": "submit-command"
            }))
            .unwrap();
            let command = ReversalSubmitReplayInput {
                id: "unread-record".into(),
                document_type: DocumentType::SupplierRefund,
                command_receipt: req.command_receipt("unread-record", actor.id()).unwrap(),
                actor: actor.clone(),
            };
            denied(
                process.submit_supplier_refund_stage(req, &command, &mut SnapshotExecutor::default()).await,
            );
        }
    }
}
