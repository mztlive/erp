use erp_permissions::{Permission, internal};
pub(in crate::procure_to_pay) mod recovery;

use application_core::AuditActor;
use bpm::model::SubjectRef;
use erp_approval_runtime::approval_recovery::recover_approval_command;
use erp_approval_runtime::approver_directory::resolve_start_approvers;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_procurement::dto::purchase_order::{
    CancelPurchaseChangeApprovalRequest, PurchaseChangeSubmitResult, SavePurchaseOrderLine,
    StartPurchaseChangeRequest, StartPurchaseChangeResult, SubmitPurchaseChangeRequest,
};
use erp_procurement::entity::purchase_order::{
    PurchaseChangeOrder, PurchaseChangeSubmission, PurchaseCommandReceiptIdentity, PurchaseOrder,
    inherit_revision_sources,
};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::service::purchase_order::change::lock_draft_change;
use erp_procurement::service::purchase_order::change::mapping::content_fingerprint;
use erp_procurement::service::purchase_order::change::state::{
    apply_change_reason, start_purchase_change_approval,
};
use erp_procurement::service::purchase_order::line_input::build_change_submission_lines;
use erp_read_models::purchase_center::PurchaseOrderReadService;
use erp_read_models::purchase_center::dto::PurchaseChangeOrderView;
use erp_sales::repository::SalesOrderExt;
use erp_workflow::DocumentRegistryExt;
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshotPayload;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use erp_workflow::ports::OrderTaskSource;
use erp_workflow::service::approval::binding::{BindPublishedDefinitionCommand, attach_published_binding};
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::approval::execution::{PreparedExecution, prepare_cancel, prepare_start};
use erp_workflow::service::document_registry::{find_approval_binding, new_registered_document};
use persistence_core::{Executor, Transactional};
use recovery::replay_change_command;
use validator::Validate;

use super::super::PurchaseOrderProcess;
use super::super::change_adapter::{
    build_purchase_change_snapshot, execute_purchase_change_domain_action, purchase_change_order_adapter,
    purchase_change_order_object_readable, purchase_change_order_subject_ref,
    purchase_change_responsible_org_id, require_frozen_binding,
};
use super::super::change_cancel::{
    PurchaseChangeCancelPersistInput, build_purchase_change_cancel_input, load_cancel_runtime,
    persist_purchase_change_cancel,
};
use super::super::change_start::{
    PurchaseChangeStartInput, PurchaseChangeStartPersistInput, build_purchase_change_start_input,
    load_bound_definition_graph, load_start_receipt, persist_purchase_change_start,
};
use super::guard::check_change_inputs;
use crate::procure_to_pay::PurchaseOrderPorts;
use crate::{Error, Result};

impl PurchaseOrderProcess {
    /// 发起采购变更（基于当前生效版本创建变更单）。
    ///
    /// 新变更单独立绑定已发布定义，不继承原采购单定义。客户端不得选择定义或审批人。
    ///
    /// # 参数
    /// * `id` - 采购单 ID
    /// * `req` - 发起请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更单结果。
    ///
    /// # 错误
    /// * `NotFound` - 采购单不存在或无权操作
    /// * `ConflictError` - 版本不一致、已存在进行中变更或未配置审批流程
    /// * `BusinessLogicError` - 采购单未生效
    ///
    /// # 关键业务约束
    /// 必须先证明采购变更创建资格及来源采购单更新范围；不可见对象一律 NotFound，
    /// 不得先返回进行中、版本或未生效错误。
    pub async fn start_change(
        &self,
        id: &str,
        req: StartPurchaseChangeRequest,
        actor: &AuditActor,
    ) -> Result<StartPurchaseChangeResult> {
        req.validate()?;
        let service = self.clone();
        let id = id.to_string();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let order = service
                        .ports
                        .access(service.db.clone())
                        .require_entry(&actor, &internal::PURCHASE_CHANGE_CREATE_SOURCE, &id, executor)
                        .await?;
                    let (order, base_revision) = service
                        .domain()
                        .load_changeable_order(order, req.expected_lock_version, executor)
                        .await?;
                    service.domain().ensure_no_in_progress_change(&id, executor).await?;
                    let change = erp_procurement::service::purchase_order::change::new_change(
                        &order,
                        &base_revision,
                        &req.reason,
                        actor.id(),
                    )?;
                    service.persist_started_change(&order, &change, &actor, executor).await?;
                    Ok(StartPurchaseChangeResult {
                        change_id: change.base.id.clone(),
                        base_revision_id: base_revision.base.id.clone(),
                        base_revision_no: base_revision.revision.revision_no,
                        lock_version: order.base.version,
                        reference: format!("CHANGE-V{}", base_revision.revision.revision_no),
                    })
                })
            })
            .await
    }

    /// 提交采购变更并调用统一 `start_approval`。
    ///
    /// 同一事务内：锁定单据、递增 `approval_subject_version`、冻结 `subject_snapshot`、
    /// 从 `BusinessDocument` 读取绑定并启动审批。客户端不得选择定义或审批人。
    ///
    /// # 参数
    /// * `change_id` - 变更单 ID
    /// * `req` - 提交请求（目标完整头、行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更提交结果。
    ///
    /// # 错误
    /// * `NotFound` - 变更单不存在，或来源采购单不可见
    /// * `ConflictError` - 版本不一致、无绑定或重复提交
    ///
    /// # 关键业务约束
    /// 读取变更单后必须立即证明变更提交资格及来源采购单提交范围，再做草稿与版本校验。
    pub async fn submit_change(
        &self,
        change_id: &str,
        req: SubmitPurchaseChangeRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseChangeSubmitResult> {
        req.validate()?;
        let identity = req.command_identity(change_id, actor.id())?;
        let fingerprint = req.request_fingerprint(change_id)?;
        let service = self.clone();
        let change_id = change_id.to_string();
        let recovery_actor = actor.clone();
        let actor = actor.clone();
        let transaction_identity = identity.clone();
        let transaction_fingerprint = fingerprint.clone();
        let result: Result<PurchaseChangeSubmitResult> = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let adapter = purchase_change_order_adapter()?;
                    let change = service.domain().load_change(&change_id, executor).await?;
                    service
                        .ports
                        .access(service.db.clone())
                        .require_entry(
                            &actor,
                            &internal::PURCHASE_CHANGE_SUBMIT_SOURCE,
                            change.purchase_order_id.as_ref(),
                            executor,
                        )
                        .await?;
                    if let Some(result) = replay_change_command(
                        &service.db,
                        &transaction_identity,
                        &transaction_fingerprint,
                        executor,
                    )
                    .await?
                    {
                        return Ok(result);
                    }
                    let change = lock_draft_change(change, req.expected_lock_version)?;
                    service.start_change_approval(&change_id, change, req, &actor, adapter, executor).await
                })
            })
            .await;
        match result {
            Err(error) if error.command_may_have_committed() => {
                self.recover_purchase_change_start(&identity, &fingerprint, &recovery_actor, error).await
            },
            result => result,
        }
    }

    /// 提交采购变更并返回提交后的完整变更单视图。
    ///
    /// # 参数
    /// * `change_id` - 变更单 ID
    /// * `req` - 期望版本、可选目标内容与幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回写事务完成后重读的完整变更单视图。
    ///
    /// # 错误
    /// 提交事务或提交后权威视图读取失败时返回错误。
    pub async fn submit_change_view(
        &self,
        change_id: &str,
        req: SubmitPurchaseChangeRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseChangeOrderView> {
        self.submit_change(change_id, req, actor).await?;
        self.change_view(change_id, actor, &internal::PURCHASE_CHANGE_SUBMITTED_RESULT_READ).await
    }

    /// 写事务结束后使用新快照和原命令资格读取完整结果。
    /// # 参数
    /// change_id、actor 为原命令身份，permission 为固定提交或作废结果入口。
    /// # 返回
    /// 新快照内重新授权的完整变更结果。
    /// # 错误
    /// 当前资格失效、来源无权、事务或读取失败时拒绝。
    pub(super) async fn change_view(
        &self,
        change_id: &str,
        actor: &AuditActor,
        permission: &Permission,
    ) -> Result<PurchaseChangeOrderView> {
        let reads = PurchaseOrderReadService::new(
            self.db.clone(),
            self.require_rbac()?.clone(),
            self.ports.scope.clone(),
        );
        let id = change_id.to_owned();
        let actor = actor.clone();
        let permission = permission.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    Ok(reads.command_change_order_view(&id, &actor, &permission, executor).await?)
                })
            })
            .await
    }

    /// 撤回审批中的采购变更单，回到可修正草稿且 `subject_version` 不回退。
    ///
    /// 作为合同 `cancel_action`，供业务撤回与管理员受阻取消共用。
    ///
    /// # 参数
    /// * `id` - 变更单主键
    /// * `req` - 撤回请求（原因必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 撤回成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 来源采购单不可见、非审批中、已最终通过、原因缺失或并发冲突时返回错误。
    ///
    /// # 关键业务约束
    /// 必须先证明变更撤回资格及来源采购单撤回范围，再校验期望版本。
    pub async fn cancel_change_approval(
        &self,
        id: &str,
        req: CancelPurchaseChangeApprovalRequest,
        actor: &AuditActor,
    ) -> Result<()> {
        req.validate()?;
        let service = self.clone();
        let id = id.to_string();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut change = service.domain().load_change(&id, executor).await?;
                    service
                        .ports
                        .access(service.db.clone())
                        .require_entry(
                            &actor,
                            &internal::PURCHASE_CHANGE_CANCEL_SOURCE,
                            change.purchase_order_id.as_ref(),
                            executor,
                        )
                        .await?;
                    change
                        .ensure_expected_version(req.expected_lock_version)
                        .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".into()))?;
                    service.persist_cancelled_change(&id, &mut change, &req, &actor, executor).await
                })
            })
            .await
    }

    /// 为新变更单独立绑定已发布定义并写入事务。
    ///
    /// # 错误
    /// 无发布定义、人员重验失败或写入失败时返回错误。
    async fn persist_started_change(
        &self,
        order: &PurchaseOrder,
        change: &PurchaseChangeOrder,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ports = self.ports.clone();
        let sales_order = self
            .db
            .sales_orders()
            .find_by_id(&order.sales_order_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
        let bind_command = BindPublishedDefinitionCommand {
            document_type: DocumentType::PurchaseChangeOrder,
            business_object_id: change.base.id.clone(),
            business_object_version: change.base.version,
            context: BindingRevalidationContext::new(
                purchase_change_responsible_org_id(&sales_order)?,
                actor.id().to_string(),
            )
            .with_order_source(Some(OrderTaskSource::Purchase(order.base.id.clone())))
            .with_business_org_unit_id(Some(order.business_org_unit_id.clone()))
            .with_scope_owner_user_id(Some(order.current_owner_user_id()?.to_string())),
        };
        let document = new_registered_document(&change.base.id, DocumentType::PurchaseChangeOrder, "")
            .map_err(crate::Error::from)?;
        let audit = actor.clone().resource_log(
            "purchase_change_order.create",
            "purchase_change_order",
            change.base.id.clone(),
        )?;
        self.require_rbac()?;
        persist_created_change_order(
            &self.db,
            ports.as_ref(),
            change.clone(),
            document,
            bind_command,
            audit,
            actor.clone(),
            executor,
        )
        .await
    }

    /// 冻结提交并启动统一审批。
    ///
    /// # 错误
    /// 无绑定、定义缺失、状态不允许或写入失败时返回错误。
    async fn start_change_approval(
        &self,
        id: &str,
        mut change: PurchaseChangeOrder,
        req: SubmitPurchaseChangeRequest,
        actor: &AuditActor,
        adapter: super::change_adapter::PurchaseChangeOrderAdapter,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseChangeSubmitResult> {
        let order = self
            .db
            .purchase_orders()
            .find_by_id(&change.purchase_order_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原采购单不存在".to_string()))?;
        let sales_order = self
            .db
            .sales_orders()
            .find_by_id(&order.sales_order_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
        apply_change_reason(&mut change, req.reason.as_deref(), actor.id())?;
        let mut prepared = self.prepare_change_target(&change, &order, &req, actor, executor).await?;
        prepared.submission.submit(Instant::now(), actor.id())?;
        start_purchase_change_approval(
            &mut change,
            prepared.submission.base.id.clone().into(),
            prepared.content_hash.clone(),
            actor.id(),
        )?;
        self.dispatch_change_start(
            ChangeStartDispatch { id, change, sales_order, prepared, adapter },
            actor,
            executor,
        )
        .await
    }

    /// 构造冻结提交与明细。
    ///
    /// # 错误
    /// 基准版本缺失或行非法时返回错误。
    pub(super) async fn prepare_change_target(
        &self,
        change: &PurchaseChangeOrder,
        order: &PurchaseOrder,
        req: &SubmitPurchaseChangeRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<FrozenChangeSubmission> {
        let base_revision = self
            .db
            .purchase_order_revisions()
            .find_by_id(&change.base_revision_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("基准版本不存在".to_string()))?;
        let mut normalized_request = req.clone();
        if normalized_request.lines.is_empty() {
            let previous = self.domain().change_draft(change, order, executor).await?;
            normalized_request.lines = previous.lines;
            if normalized_request.payment_term_code.is_none() {
                normalized_request.payment_term_code = Some(previous.payment_term_code);
            }
        }
        let supplier_name = self.change_supplier_name(order, executor).await?;
        let submission = self
            .build_change_submission(
                change,
                order,
                &base_revision,
                &supplier_name,
                &normalized_request,
                executor,
            )
            .await?;
        let enriched_lines = self
            .enrich_change_lines_with_current_sales_revision(order, &normalized_request.lines, executor)
            .await?;
        let mut inputs = SavePurchaseOrderLine::to_line_inputs(&enriched_lines)?;
        let source_lines =
            self.db.purchase_order().list_revision_lines(&change.base_revision_id, executor).await?;
        inherit_revision_sources(&mut inputs, &source_lines)?;
        check_change_inputs(&self.db, order, &inputs, submission.gross_amount, executor).await?;
        let lines = build_change_submission_lines(&submission.base.id.clone(), &inputs)?;
        Ok(FrozenChangeSubmission {
            submission,
            lines,
            content_hash: content_fingerprint(&normalized_request.lines),
            idempotency_key: req.idempotency_key.clone(),
            command_identity: req.command_identity(&change.base.id, actor.id())?,
            request_fingerprint: req.request_fingerprint(&change.base.id)?,
            resubmission_reason: req.resubmission_reason.clone(),
        })
    }

    /// 使用原事务读取本次变更冻结的供应商名称。
    async fn change_supplier_name(
        &self,
        order: &PurchaseOrder,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let names =
            erp_read_models::purchase_center::repository::supplier_names::current_legal_names_by_account_ids(
                &self.db,
                std::slice::from_ref(&order.supplier_id),
                executor,
            )
            .await?;
        Ok(names
            .get(&order.supplier_id.to_string())
            .cloned()
            .unwrap_or_else(|| order.supplier_id.to_string()))
    }

    /// 从绑定读取定义并持久化启动事实。
    ///
    /// # 用途
    /// 加载冻结绑定并写入采购变更启动事实。
    ///
    /// # 参数
    /// * `dispatch` - 变更单、原单、提交与适配器
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回提交结果。
    ///
    /// # 错误
    /// 无绑定、定义缺失或写入失败时返回错误。
    ///
    /// # 关键业务约束
    /// 必须使用单据创建时冻结的发布定义。
    async fn dispatch_change_start(
        &self,
        dispatch: ChangeStartDispatch<'_>,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseChangeSubmitResult> {
        let context = self.prepare_change_start_context(&dispatch, actor, executor).await?;
        let prepared_exec = self.prepare_change_execution(&dispatch, &context, actor, executor).await?;
        self.persist_change_dispatch(dispatch, context, prepared_exec, actor, executor).await
    }

    /// 按既有顺序读取冻结绑定、构造快照并校验启动组织。
    async fn prepare_change_start_context(
        &self,
        dispatch: &ChangeStartDispatch<'_>,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseChangeApprovalContext> {
        let subject = purchase_change_order_subject_ref(dispatch.id)?;
        let binding =
            find_approval_binding(&self.db, dispatch.id, executor).await.map_err(crate::Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let now = Instant::now();
        let snapshot = build_purchase_change_snapshot(
            &dispatch.change,
            &dispatch.sales_order,
            &dispatch.prepared.submission,
            &dispatch.prepared.lines,
            actor.id(),
            now,
        )?;
        let organization_id = purchase_change_responsible_org_id(&dispatch.sales_order)?;
        let _ = purchase_change_order_object_readable(&organization_id, actor.id())?;
        Ok(PurchaseChangeApprovalContext { subject, binding, now, snapshot, organization_id })
    }

    /// 从绑定图和候选收据准备启动，不提前执行持久化。
    async fn prepare_change_execution(
        &self,
        dispatch: &ChangeStartDispatch<'_>,
        context: &PurchaseChangeApprovalContext,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PreparedExecution> {
        let graph = load_bound_definition_graph(&self.db, &context.binding, executor).await?;
        let existing_receipt = load_start_receipt(
            &self.db,
            &context.subject,
            dispatch.change.approval_subject_version,
            &dispatch.prepared.idempotency_key,
            executor,
        )
        .await?;
        let graph = resolve_start_approvers(
            &self.db,
            self.require_rbac()?,
            graph,
            actor.id(),
            existing_receipt.as_ref(),
            executor,
        )
        .await?;
        let start_input = build_purchase_change_start_input(PurchaseChangeStartInput {
            resubmission_reason: dispatch.prepared.resubmission_reason.clone(),
            graph,
            binding: &context.binding,
            subject: context.subject.clone(),
            subject_version: dispatch.change.approval_subject_version,
            actor_id: actor.id(),
            organization_id: &context.organization_id,
            idempotency_key: &dispatch.prepared.idempotency_key,
            receipt: existing_receipt,
            now: context.now,
        })?;
        Ok(prepare_start(start_input)?)
    }

    /// 准备审计后提交原写入计划，只有原持久化错误进入有界恢复。
    async fn persist_change_dispatch(
        &self,
        dispatch: ChangeStartDispatch<'_>,
        context: PurchaseChangeApprovalContext,
        prepared_exec: PreparedExecution,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseChangeSubmitResult> {
        let ChangeStartDispatch { change, prepared, adapter, .. } = dispatch;
        let PurchaseChangeApprovalContext { snapshot, organization_id, now, .. } = context;
        let audit = actor.clone().resource_log(
            "purchase_change_order.submit",
            "purchase_change_order",
            change.base.id.clone(),
        )?;
        let identity = prepared.command_identity.clone();
        let fingerprint = prepared.request_fingerprint.clone();
        persist_purchase_change_start(
            &self.db,
            PurchaseChangeStartPersistInput {
                command_identity: identity,
                request_fingerprint: fingerprint,
                resubmission_reason: prepared.resubmission_reason.clone(),
                ports: self.ports.clone(),
                change_order: change.clone(),
                submission: prepared.submission.clone(),
                submission_lines: prepared.lines,
                snapshot_payload: snapshot,
                prepared: prepared_exec,
                owner_role: adapter.owner_role,
                organization_id,
                now,
                audit,
                actor: actor.clone(),
            },
            executor,
        )
        .await
    }

    /// 原事务提交未知或并发撞键后，仅按完整原请求回读首次命令结果。
    async fn recover_purchase_change_start(
        &self,
        identity: &PurchaseCommandReceiptIdentity,
        fingerprint: &str,
        actor: &AuditActor,
        original_error: Error,
    ) -> Result<PurchaseChangeSubmitResult> {
        recover_approval_command(original_error, || {
            let db = self.db.clone();
            let identity = identity.clone();
            let fingerprint = fingerprint.to_string();
            let access = self.ports.access(db.clone());
            let actor = actor.clone();
            self.db.client().with_transaction(move |executor| {
                Box::pin(async move {
                    let change_id = identity
                        .scope_id
                        .as_deref()
                        .ok_or_else(|| Error::Internal("采购变更回执缺少业务对象".into()))?;
                    let change =
                        erp_procurement::service::purchase_order::PurchaseOrderService::new(db.clone())
                            .load_change(change_id, executor)
                            .await?;
                    access
                        .require_entry(
                            &actor,
                            &internal::PURCHASE_CHANGE_SUBMIT_SOURCE,
                            change.purchase_order_id.as_ref(),
                            executor,
                        )
                        .await?;
                    replay_change_command(&db, &identity, &fingerprint, executor).await
                })
            })
        })
        .await
    }

    /// 加载撤回运行事实并写回草稿。
    ///
    /// # 错误
    /// 无绑定、实例终态或写入失败时返回错误。
    async fn persist_cancelled_change(
        &self,
        id: &str,
        change: &mut PurchaseChangeOrder,
        req: &CancelPurchaseChangeApprovalRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let adapter = purchase_change_order_adapter()?;
        let binding = find_approval_binding(&self.db, id, executor).await.map_err(crate::Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let subject = purchase_change_order_subject_ref(id)?;
        let runtime =
            load_cancel_runtime(&self.db, &binding, &subject, change.approval_subject_version, executor)
                .await?;
        let now = Instant::now();
        let input = build_purchase_change_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &req.idempotency_key,
            None,
            now,
        )?;
        let prepared = prepare_cancel(input)?;
        execute_purchase_change_domain_action(change, adapter.cancel_action, actor.id())?;
        let audit = actor.clone().resource_log(
            "purchase_change_order.cancel_approval",
            "purchase_change_order",
            id.to_string(),
        )?;
        persist_purchase_change_cancel(
            &self.db,
            PurchaseChangeCancelPersistInput {
                ports: self.ports.clone(),
                change_order: change.clone(),
                prepared,
                open_tasks: runtime.open_tasks,
                actor_id: actor.id().to_string(),
                reason: req.reason.clone(),
                now,
                audit,
                actor: actor.clone(),
            },
            executor,
        )
        .await
    }
}

/// 采购变更启动所需的单据、提交与适配器。
///
/// # 用途
/// 将变更单、原采购单、来源销售单、冻结提交与适配器打包。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 变更提交必须已冻结；原采购单仅用于来源复验。
struct ChangeStartDispatch<'a> {
    /// 变更单主键。
    id: &'a str,
    /// 已进入审批中的变更单。
    change: PurchaseChangeOrder,
    /// 来源销售单。
    sales_order: erp_sales::entity::sales_order::SalesOrder,
    /// 冻结提交。
    prepared: FrozenChangeSubmission,
    /// 采购变更审批适配器。
    adapter: super::change_adapter::PurchaseChangeOrderAdapter,
}

/// 已冻结的变更提交与指纹。
pub(super) struct FrozenChangeSubmission {
    command_identity: PurchaseCommandReceiptIdentity,
    request_fingerprint: String,
    /// 已提交的不可变头。
    pub(super) submission: PurchaseChangeSubmission,
    /// 提交明细。
    pub(super) lines: Vec<erp_procurement::entity::purchase_order::PurchaseChangeSubmissionLine>,
    /// 目标内容指纹。
    pub(super) content_hash: String,
    /// 启动幂等键。
    idempotency_key: String,
    resubmission_reason: Option<String>,
}

/// 在创建事务内写入变更单、绑定发布定义并登记单据。
///
/// # 错误
/// 无发布定义、人员重验失败或写入失败时返回错误，调用方必须回滚。
#[allow(clippy::too_many_arguments)]
async fn persist_created_change_order(
    db: &mongodb::Database,
    ports: &PurchaseOrderPorts,
    change_order: PurchaseChangeOrder,
    mut document: BusinessDocument,
    bind_command: BindPublishedDefinitionCommand,
    audit: erp_audit::AuditLog,
    actor: AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    ports
        .access(db.clone())
        .require_object(&actor, "update", change_order.purchase_order_id.as_ref(), executor)
        .await?;
    persist_bound_change_document(db, ports, &mut document, &bind_command, &actor, executor).await?;
    db.purchase_change_orders().create(&change_order, executor).await?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}

/// 查询发布定义、写入绑定并持久化注册行。
///
/// # 错误
/// 无发布定义或绑定失败时返回错误。
async fn persist_bound_change_document(
    db: &mongodb::Database,
    ports: &PurchaseOrderPorts,
    document: &mut BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = purchase_change_order_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let binding = ports.approval.bind_document(db, bind_command, actor, executor).await?;
    let binding = binding.ok_or_else(|| Error::Internal("采购变更单必须绑定已发布定义".to_string()))?;
    attach_published_binding(document, binding)?;
    db.business_documents().create(document, executor).await?;
    Ok(())
}

struct PurchaseChangeApprovalContext {
    subject: SubjectRef,
    binding: ApprovalDefinitionBinding,
    now: Instant,
    snapshot: ApprovalSubjectSnapshotPayload,
    organization_id: String,
}
