//! 销售变更创建、启动、作废及撤回的跨域根流程。

use std::sync::Arc;

use application_core::AuditActor;
use bpm::model::SubjectRef;
use erp_approval_runtime::ApprovalRuntimePort;
use erp_approval_runtime::approval_recovery::recover_approval_command;
use erp_approval_runtime::approver_directory::resolve_start_approvers;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_core::ids::BusinessDocumentId;
use erp_identity::SharedRbacService;
use erp_read_models::sales_center::review::{SalesChangeOrderDetailView, SalesChangeReadService};
use erp_sales::dto::sales_review::{
    CancelSalesChangeApprovalRequest, CreateSalesChangeOrderRequest, SubmitSalesChangeRequest,
    VoidSalesChangeOrderRequest,
};
use erp_sales::entity::sales_review::{SalesChangeOrder, SalesChangeSubmissionCommand};
use erp_sales::repository::{SalesOrderExt, SalesReviewExt};
use erp_sales::service::sales_review::{
    CreatedChangeWrite, SalesChangeSubmissionWrite, SalesReviewService, latest_change_submission_no,
};
use erp_workflow::DocumentRegistryExt;
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshotPayload;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::document_registry::{
    BusinessDocument, WorkflowAction, WorkflowActionData, WorkflowActionId, WorkflowActionType,
};
use erp_workflow::ports::OrderTaskSource;
use erp_workflow::service::approval::binding::{BindPublishedDefinitionCommand, attach_published_binding};
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::{PreparedExecution, prepare_cancel, prepare_start};
use erp_workflow::service::document_registry::{find_approval_binding, new_registered_document};
use erp_workflow::service::work_item::approval_rework::complete_document_rework;
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::SalesChangeProcess;
use super::adapter::{
    SalesChangeOrderAdapter, build_sales_change_snapshot, execute_sales_change_domain_action,
    require_frozen_binding, sales_change_order_adapter, sales_change_order_object_readable,
    sales_change_order_subject_ref, sales_change_responsible_org_id,
};
use super::cancel_approval::{
    SalesChangeCancelPersistInput, build_sales_change_cancel_input, load_cancel_runtime,
    persist_sales_change_cancel,
};
use super::start_approval::{
    SalesChangeStartInput, SalesChangeStartPersistInput, build_sales_change_start_input,
    load_bound_definition_graph, load_start_receipt, persist_sales_change_start,
};
use super::submission_receipt::replay_submission;
use crate::{Error, Result};

impl SalesChangeProcess {
    /// 创建销售变更单（草稿 + 变更工作副本 + `BusinessDocument` 绑定原子形成）。
    ///
    /// `PROCESS_REQUIRED` 无发布定义时返回 `APPROVAL_PROCESS_NOT_CONFIGURED`，
    /// 业务单据零写入。客户端不得选择定义或审批人。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    /// * `rbac` - 共享 RBAC
    ///
    /// # 返回
    /// 返回变更单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 原销售单不存在或未生效
    /// * `ConflictError` - 同一基准版本已有进行中变更或未配置审批流程
    pub async fn create_sales_change_order(
        &self,
        req: CreateSalesChangeOrderRequest,
        actor: &AuditActor,
        rbac: &SharedRbacService,
    ) -> Result<SalesChangeOrderDetailView> {
        let source_order = self
            .command_access(actor, "update")
            .current(req.sales_order_id.as_ref(), &mut NoTransaction)
            .await?;
        let sales_write = SalesReviewService::new(self.db.clone()).prepare_creation(req, actor).await?;
        let change_id = sales_write.change_id().to_string();
        let bind_command = BindPublishedDefinitionCommand {
            document_type: erp_workflow::entity::document_registry::DocumentType::SalesChangeOrder,
            business_object_id: change_id.clone(),
            business_object_version: sales_write.version(),
            context: BindingRevalidationContext::new(
                sales_change_responsible_org_id(sales_write.settlement_party_id())?,
                actor.id().to_string(),
            )
            .with_order_source(Some(OrderTaskSource::Sales(source_order.base.id.clone())))
            .with_business_org_unit_id(Some(source_order.business_org_unit_id.clone()))
            .with_scope_owner_user_id(Some(source_order.sales_owner_user_id.clone())),
        };
        let document = new_registered_document(
            change_id.clone(),
            erp_workflow::entity::document_registry::DocumentType::SalesChangeOrder,
            String::new(),
        )
        .map_err(crate::Error::from)?;
        let audit = actor.clone().resource_log(
            "sales_change_order.create",
            "sales_change_order",
            change_id.clone(),
        )?;
        persist_created_change_order(
            &self.db,
            rbac,
            CreatedChangeOrderPersistInput {
                approvals: self.services.approvals.clone(),
                sales_write,
                document,
                bind_command,
                audit,
                actor: actor.clone(),
            },
        )
        .await?;

        SalesChangeReadService::with_rbac(self.db.clone(), self.rbac.clone())
            .sales_change_order_detail(&change_id, actor)
            .await
            .map_err(crate::Error::from)
    }

    /// 提交销售变更并调用统一 `start_approval`。
    ///
    /// `subject_version` 取 `sales_change_submission.submission_no`，不得复用
    /// `BaseModel.version`。定义与审批人取自已绑定事实，不接受客户端选择。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    /// * `req` - 提交请求（含期望版本与幂等键）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单或变更工作副本不存在
    /// * `ConflictError` - 期望版本不一致、无绑定或状态不允许
    pub async fn submit_sales_change(
        &self,
        id: &str,
        req: SubmitSalesChangeRequest,
        actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        req.validate()?;
        let adapter = sales_change_order_adapter()?;
        let command = SalesChangeSubmissionCommand::new(
            id,
            actor.id(),
            req.version,
            &req.idempotency_key,
            req.resubmission_reason.as_deref(),
        )?;
        if replay_submission(&self.db, self.rbac.clone(), actor, &command, &mut NoTransaction)
            .await?
            .is_some()
        {
            return SalesChangeReadService::with_rbac(self.db.clone(), self.rbac.clone())
                .sales_change_order_detail(id, actor)
                .await
                .map_err(Error::from);
        }
        let change_order =
            SalesReviewService::new(self.db.clone()).load_for_submission(id, req.version).await?;
        self.start_change_approval(id, change_order, req, command, actor, adapter).await
    }

    /// 作废草稿或已驳回的销售变更单。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    /// * `req` - 作废请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn void_sales_change(
        &self,
        id: &str,
        req: VoidSalesChangeOrderRequest,
        actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        let preview = self
            .db
            .sales_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在或无权操作".to_string()))?;
        self.command_access(actor, "update")
            .current(preview.sales_order_id.as_ref(), &mut NoTransaction)
            .await?;
        let mut sales_write = SalesReviewService::new(self.db.clone()).prepare_void(id, req, actor).await?;
        let audit =
            actor.clone().resource_log("sales_change_order.void", "sales_change_order", id.to_string())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let rbac = self.rbac.clone();
        let actor_for_tx = actor.clone();
        let document_id = id.to_string();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    erp_read_models::sales_center::access::SalesAccess::new(db.clone(), rbac)
                        .require_object(&actor_for_tx, "update", sales_write.sales_order_id(), &[], executor)
                        .await
                        .map_err(crate::Error::from)?;
                    sales_write.persist(&db, executor).await?;
                    complete_document_rework(
                        &db,
                        "sales_change_order",
                        &document_id,
                        actor_for_tx.id(),
                        Instant::now(),
                        executor,
                    )
                    .await?;
                    persist_log(&db, &audit, executor).await?;
                    Ok::<(), crate::Error>(())
                })
            })
            .await?;

        SalesChangeReadService::with_rbac(self.db.clone(), self.rbac.clone())
            .sales_change_order_detail(id, actor)
            .await
            .map_err(crate::Error::from)
    }

    /// 撤回审批中的销售变更单，回到可修正草稿且 `subject_version` 不回退。
    ///
    /// # 参数
    /// * `id` - 变更单主键
    /// * `req` - 撤回请求（原因必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回撤回后的变更单详情。
    ///
    /// # 错误
    /// 非审批中、已最终通过、原因缺失或并发冲突时返回错误。
    pub async fn cancel_approval(
        &self,
        id: &str,
        req: CancelSalesChangeApprovalRequest,
        actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        req.validate()?;
        let preview = self
            .db
            .sales_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在或无权操作".to_string()))?;
        self.command_access(actor, "cancel_approval")
            .current(preview.sales_order_id.as_ref(), &mut NoTransaction)
            .await?;
        let mut change_order =
            SalesReviewService::new(self.db.clone()).load_for_cancellation(id, req.expected_version).await?;
        let adapter = sales_change_order_adapter()?;
        let binding =
            find_approval_binding(&self.db, id, &mut NoTransaction).await.map_err(crate::Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let subject = sales_change_order_subject_ref(id)?;
        let subject_version = latest_change_submission_no(&self.db, id).await?;
        let runtime = load_cancel_runtime(&self.db, &binding, &subject, subject_version).await?;
        let now = Instant::now();
        let idempotency_key = normalize_idempotency_key(&req.idempotency_key)?;
        let input =
            build_sales_change_cancel_input(&runtime, &req.reason, actor.id(), &idempotency_key, None, now)?;
        let prepared = prepare_cancel(input)?;
        execute_sales_change_domain_action(&mut change_order, adapter.cancel_action, actor.id())?;
        let audit = actor.clone().resource_log(
            "sales_change_order.cancel_approval",
            "sales_change_order",
            id.to_string(),
        )?;
        persist_sales_change_cancel(
            &self.db,
            SalesChangeCancelPersistInput {
                approvals: self.services.approvals.clone(),
                change_order,
                prepared,
                open_tasks: runtime.open_tasks,
                actor_id: actor.id().to_string(),
                reason: req.reason.clone(),
                now,
                audit,
                actor: actor.clone(),
                rbac: self.rbac.clone(),
            },
        )
        .await?;
        SalesChangeReadService::with_rbac(self.db.clone(), self.rbac.clone())
            .sales_change_order_detail(id, actor)
            .await
            .map_err(crate::Error::from)
    }

    /// 冻结提交并启动统一审批。
    ///
    /// # 错误
    /// 无绑定、定义缺失、状态不允许或写入失败时返回错误。
    async fn start_change_approval(
        &self,
        id: &str,
        change_order: SalesChangeOrder,
        request: SubmitSalesChangeRequest,
        command: SalesChangeSubmissionCommand,
        actor: &AuditActor,
        adapter: SalesChangeOrderAdapter,
    ) -> Result<SalesChangeOrderDetailView> {
        let idempotency_key = request.idempotency_key;
        let resubmission_reason = request.resubmission_reason;
        let from_status = change_order.stable.status.as_str();
        let mut context = self.prepare_change_context(id, change_order, actor).await?;
        context.resubmission_reason = resubmission_reason.clone();
        let mut input =
            self.prepare_change_start(id, context, &idempotency_key, actor, adapter, command.clone()).await?;
        input.workflow_action.comment = resubmission_reason.clone();
        input.workflow_action.from_status = from_status.to_string();
        let persisted = persist_sales_change_start(&self.db, input).await;
        if let Err(error) = persisted {
            if !error.command_may_have_committed() {
                return Err(error);
            }
            self.recover_sales_change_start(&command, actor, error).await?;
        }
        SalesChangeReadService::with_rbac(self.db.clone(), self.rbac.clone())
            .sales_change_order_detail(id, actor)
            .await
            .map_err(crate::Error::from)
    }

    /// 在原读取位置准备领域提交与冻结快照，不执行写入。
    async fn prepare_change_context(
        &self,
        id: &str,
        change_order: SalesChangeOrder,
        actor: &AuditActor,
    ) -> Result<SalesChangeApprovalContext> {
        let sales_order = self
            .db
            .sales_orders()
            .find_by_id(&change_order.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        let subject = sales_change_order_subject_ref(id)?;
        let binding =
            find_approval_binding(&self.db, id, &mut NoTransaction).await.map_err(crate::Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let sales_write =
            SalesReviewService::new(self.db.clone()).prepare_submission(change_order, actor).await?;
        let submission = sales_write.submission();
        let now = Instant::now();
        let snapshot = build_sales_change_snapshot(
            sales_write.change(),
            &sales_order,
            submission,
            sales_write.lines(),
            actor.id(),
            now,
        )?;
        let organization_id = sales_change_responsible_org_id(&sales_order.settlement_party_id)?;
        Ok(SalesChangeApprovalContext {
            sales_write,
            subject,
            binding,
            now,
            snapshot,
            organization_id,
            resubmission_reason: None,
        })
    }

    /// 从冻结定义生成启动计划及元数据，保留审批人重验和审计构造顺序。
    async fn prepare_change_start(
        &self,
        id: &str,
        context: SalesChangeApprovalContext,
        idempotency_key: &str,
        actor: &AuditActor,
        adapter: SalesChangeOrderAdapter,
        command: SalesChangeSubmissionCommand,
    ) -> Result<SalesChangeStartPersistInput> {
        let prepared = self.prepare_change_runtime(&context, idempotency_key, actor).await?;
        let SalesChangeApprovalContext {
            sales_write,
            subject: _,
            binding: _,
            now,
            snapshot,
            organization_id,
            resubmission_reason,
        } = context;
        let workflow_action = submission_workflow_action(id, actor, adapter.owner_role)?;
        let audit =
            actor.clone().resource_log("sales_change_order.submit", "sales_change_order", id.to_string())?;
        Ok(SalesChangeStartPersistInput {
            command,
            resubmission_reason,
            approvals: self.services.approvals.clone(),
            sales_write,
            workflow_action,
            snapshot_payload: snapshot,
            prepared,
            owner_role: adapter.owner_role,
            organization_id,
            now,
            audit,
            actor: actor.clone(),
            rbac: self.rbac.clone(),
        })
    }

    /// 读取冻结定义与回执并解析审批人，构造本次启动计划。
    async fn prepare_change_runtime(
        &self,
        context: &SalesChangeApprovalContext,
        idempotency_key: &str,
        actor: &AuditActor,
    ) -> Result<PreparedExecution> {
        let graph = load_bound_definition_graph(&self.db, &context.binding).await?;
        let subject_version = context.sales_write.submission().submission_no;
        let existing_receipt =
            load_start_receipt(&self.db, &context.subject, subject_version, idempotency_key).await?;
        let graph = resolve_start_approvers(
            &self.db,
            &self.rbac,
            graph,
            actor.id(),
            existing_receipt.as_ref(),
            &mut NoTransaction,
        )
        .await?;
        let start_input = build_sales_change_start_input(SalesChangeStartInput {
            resubmission_reason: context.resubmission_reason.clone(),
            graph,
            binding: &context.binding,
            subject: context.subject.clone(),
            subject_version,
            actor_id: actor.id(),
            organization_id: &context.organization_id,
            idempotency_key,
            receipt: existing_receipt,
            now: context.now,
        })?;
        prepare_start(start_input).map_err(Error::from)
    }

    /// receipt 唯一竞争、瞬态事务或提交结果未知后，以 fresh session 有界回读。
    async fn recover_sales_change_start(
        &self,
        command: &SalesChangeSubmissionCommand,
        actor: &AuditActor,
        original_error: Error,
    ) -> Result<String> {
        let rbac = self.rbac.clone();
        recover_approval_command(original_error, || {
            let db = self.db.clone();
            let command = command.clone();
            let actor = actor.clone();
            let rbac = rbac.clone();
            self.db.client().with_transaction(move |executor| {
                Box::pin(async move { replay_submission(&db, rbac, &actor, &command, executor).await })
            })
        })
        .await
    }
}

/// 销售变更单创建事务写入集合。
///
/// # 用途
/// 收拢创建变更单时需一并持久化的单据、工作副本、注册行与审计。
///
/// # 参数
/// 无。
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 必须绑定已发布定义；人员重验失败时不得写入。
struct CreatedChangeOrderPersistInput {
    approvals: Arc<dyn ApprovalRuntimePort>,
    /// 新建销售变更、工作副本与行的本域计划。
    sales_write: CreatedChangeWrite,
    /// 待登记的业务单据。
    document: BusinessDocument,
    /// 发布定义绑定命令。
    bind_command: BindPublishedDefinitionCommand,
    /// 已构造审计。
    audit: erp_audit::AuditLog,
    /// 审计操作人。
    actor: AuditActor,
}
/// 在创建事务内写入变更单、绑定发布定义并登记单据。
///
/// # 用途
/// 创建变更单时原子写入单据、工作副本与发布定义绑定。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `input` - 变更单、工作副本、注册行与审计
///
/// # 返回
/// 成功时无返回值。
///
/// # 错误
/// 无发布定义、人员重验失败或写入失败时返回错误，调用方必须回滚。
///
/// # 关键业务约束
/// 销售变更单必须绑定已发布定义，不得无定义创建。
async fn persist_created_change_order(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    input: CreatedChangeOrderPersistInput,
) -> Result<()> {
    let CreatedChangeOrderPersistInput { approvals, sales_write, mut document, bind_command, audit, actor } =
        input;
    let db = db.clone();
    let rbac = rbac.clone();

    let client = db.client().clone();
    client
        .with_transaction(move |executor| {
            Box::pin(async move {
                erp_read_models::sales_center::access::SalesAccess::new(db.clone(), rbac.clone())
                    .require_object(&actor, "update", sales_write.sales_order_id(), &[], executor)
                    .await
                    .map_err(crate::Error::from)?;
                persist_bound_change_document(
                    &db,
                    approvals.as_ref(),
                    &mut document,
                    &bind_command,
                    &actor,
                    executor,
                )
                .await?;
                sales_write.persist(&db, executor).await?;
                persist_log(&db, &audit, executor).await?;
                Ok::<(), crate::Error>(())
            })
        })
        .await
}
/// 查询发布定义、写入绑定并持久化注册行。
///
/// # 错误
/// 无发布定义或绑定失败时返回错误。
async fn persist_bound_change_document(
    db: &mongodb::Database,
    approvals: &dyn ApprovalRuntimePort,
    document: &mut BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = sales_change_order_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let binding = approvals.bind_document(db, bind_command, actor, executor).await?;
    let binding = binding.ok_or_else(|| Error::Internal("销售变更单必须绑定已发布定义".to_string()))?;
    attach_published_binding(document, binding)?;
    db.business_documents().create(document, executor).await?;
    Ok(())
}

struct SalesChangeApprovalContext {
    resubmission_reason: Option<String>,
    sales_write: SalesChangeSubmissionWrite,
    subject: SubjectRef,
    binding: ApprovalDefinitionBinding,
    now: Instant,
    snapshot: ApprovalSubjectSnapshotPayload,
    organization_id: String,
}

/// 构造本次提交的工作流元数据；不承载状态迁移规则。
fn submission_workflow_action(id: &str, actor: &AuditActor, owner_role: &str) -> Result<WorkflowAction> {
    WorkflowAction::new(
        WorkflowActionId::new(next_id()),
        WorkflowActionData {
            document_id: BusinessDocumentId::new(id.to_string()),
            action_type: WorkflowActionType::Submit,
            from_status: "DRAFT".to_string(),
            to_status: "IN_APPROVAL".to_string(),
            actor_id: actor.id().to_string(),
            actor_role: owner_role.to_string(),
            comment: None,
        },
    )
    .map_err(Error::from)
}
