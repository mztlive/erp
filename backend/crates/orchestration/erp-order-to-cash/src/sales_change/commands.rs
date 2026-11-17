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
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::{sales_order, sales_review};
use erp_read_models::sales_center::review::{SalesChangeOrderDetailView, SalesChangeReadService};
use erp_sales::dto::sales_review::{
    CancelSalesChangeApprovalRequest, CreateSalesChangeOrderRequest, SubmitSalesChangeRequest,
    VoidSalesChangeOrderRequest,
};
use erp_sales::entity::sales_order::SalesOrder;
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
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::SalesChangeProcess;
use super::adapter::{
    build_sales_change_snapshot, execute_sales_change_domain_action, require_frozen_binding,
    sales_change_order_adapter, sales_change_order_object_readable, sales_change_order_subject_ref,
    sales_change_responsible_org_id,
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
    ) -> Result<SalesChangeOrderDetailView> {
        let this = self.clone();
        let actor_for_tx = actor.clone();
        let change_id = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.create_change_stage(req, &actor_for_tx, executor).await })
            })
            .await?;

        SalesChangeReadService::new(self.db.clone(), self.authorization.clone())
            .command_detail(&change_id, actor, &sales_review::SALES_CHANGE_ORDER_CREATE)
            .await
            .map_err(crate::Error::from)
    }

    /// 在授权、准备和写入共用的快照中创建销售变更。
    async fn create_change_stage(
        &self,
        request: CreateSalesChangeOrderRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        self.authorization
            .require(actor, &sales_review::SALES_CHANGE_ORDER_CREATE, AccessTarget::Operation, executor)
            .await?;
        let source = self
            .command_access(actor, &sales_order::HANDOVER)
            .current(request.sales_order_id.as_ref(), executor)
            .await?;
        let sales_write =
            SalesReviewService::new(self.db.clone()).prepare_creation(request, actor, executor).await?;
        let change_id = sales_write.change_id().to_string();
        let input = CreatedChangeOrderPersistInput::new(
            self.services.approvals.clone(),
            sales_write,
            &source,
            actor,
        )?;
        persist_created_change_order(&self.db, input, executor).await?;
        Ok(change_id)
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
        let command = SalesChangeSubmissionCommand::new(
            id,
            actor.id(),
            req.version,
            &req.idempotency_key,
            req.resubmission_reason.as_deref(),
        )?;
        let this = self.clone();
        let actor_copy = actor.clone();
        let id_copy = id.to_string();
        let command_copy = command.clone();
        let result = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.submit_change_stage(&id_copy, req, &command_copy, &actor_copy, executor).await
                })
            })
            .await;
        if let Err(error) = result {
            if !error.command_may_have_committed() {
                return Err(error);
            }
            self.recover_sales_change_start(&command, actor, error).await?;
        }
        SalesChangeReadService::new(self.db.clone(), self.authorization.clone())
            .command_detail(id, actor, &sales_review::SALES_CHANGE_DRAFT)
            .await
            .map_err(Error::from)
    }

    async fn submit_change_stage(
        &self,
        id: &str,
        request: SubmitSalesChangeRequest,
        command: &SalesChangeSubmissionCommand,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if replay_submission(&self.db, self.authorization.clone(), actor, command, executor).await?.is_some()
        {
            return Ok(());
        }
        let change = SalesReviewService::new(self.db.clone())
            .load_for_submission(id, request.version, executor)
            .await?;
        self.start_change_approval(id, change, request, command.clone(), actor, executor).await
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
        let db = self.db.clone();
        let authorization = self.authorization.clone();
        let actor_for_tx = actor.clone();
        let document_id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    void_change(&db, &authorization, &document_id, req, &actor_for_tx, executor).await
                })
            })
            .await?;

        SalesChangeReadService::new(self.db.clone(), self.authorization.clone())
            .command_detail(id, actor, &sales_review::SALES_CHANGE_ORDER_VOID)
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
        let this = self.clone();
        let actor_copy = actor.clone();
        let id_copy = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.cancel_change_stage(&id_copy, req, &actor_copy, executor).await })
            })
            .await?;
        SalesChangeReadService::new(self.db.clone(), self.authorization.clone())
            .command_detail(id, actor, &sales_review::SALES_CHANGE_DRAFT)
            .await
            .map_err(Error::from)
    }

    async fn cancel_change_stage(
        &self,
        id: &str,
        req: CancelSalesChangeApprovalRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        req.validate()?;
        self.authorization
            .require(
                actor,
                &sales_review::SALES_CHANGE_ORDER_CANCEL_APPROVAL,
                AccessTarget::Operation,
                executor,
            )
            .await?;
        let preview = self
            .db
            .sales_change_orders()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在或无权操作".to_string()))?;
        self.command_access(actor, &sales_order::CANCEL_APPROVAL)
            .current(preview.sales_order_id.as_ref(), executor)
            .await?;
        let change_order = SalesReviewService::new(self.db.clone())
            .load_for_cancellation(id, req.expected_version, executor)
            .await?;
        let input = self.prepare_change_cancel(id, change_order, &req, actor, executor).await?;
        persist_sales_change_cancel(&self.db, input, executor).await
    }

    /// 按已授权原事务中的业务版本构造撤回运行态和审计写入计划。
    async fn prepare_change_cancel(
        &self,
        id: &str,
        mut change_order: SalesChangeOrder,
        req: &CancelSalesChangeApprovalRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SalesChangeCancelPersistInput> {
        let adapter = sales_change_order_adapter()?;
        let binding = find_approval_binding(&self.db, id, executor).await.map_err(crate::Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let subject = sales_change_order_subject_ref(id)?;
        let subject_version = latest_change_submission_no(&self.db, id, executor).await?;
        let runtime = load_cancel_runtime(&self.db, &binding, &subject, subject_version, executor).await?;
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
        Ok(SalesChangeCancelPersistInput {
            approvals: self.services.approvals.clone(),
            change_order,
            prepared,
            open_tasks: runtime.open_tasks,
            actor_id: actor.id().to_string(),
            reason: req.reason.clone(),
            now,
            audit,
            actor: actor.clone(),
        })
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
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let from_status = change_order.stable.status.as_str();
        let mut context = self.prepare_change_context(id, change_order, actor, executor).await?;
        context.resubmission_reason = request.resubmission_reason.clone();
        let mut input = self
            .prepare_change_start(id, context, &request.idempotency_key, actor, command, executor)
            .await?;
        input.workflow_action.comment = request.resubmission_reason;
        input.workflow_action.from_status = from_status.to_string();
        persist_sales_change_start(&self.db, input, executor).await
    }

    /// 在原读取位置准备领域提交与冻结快照，不执行写入。
    async fn prepare_change_context(
        &self,
        id: &str,
        change_order: SalesChangeOrder,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SalesChangeApprovalContext> {
        let sales_order = self
            .db
            .sales_orders()
            .find_by_id(&change_order.sales_order_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        let subject = sales_change_order_subject_ref(id)?;
        let binding = find_approval_binding(&self.db, id, executor).await.map_err(crate::Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let sales_write = SalesReviewService::new(self.db.clone())
            .prepare_submission(change_order, actor, executor)
            .await?;
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
        command: SalesChangeSubmissionCommand,
        executor: &mut dyn Executor,
    ) -> Result<SalesChangeStartPersistInput> {
        let adapter = sales_change_order_adapter()?;
        let prepared = self.prepare_change_runtime(&context, idempotency_key, actor, executor).await?;
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
            authorization: self.authorization.clone(),
        })
    }

    /// 读取冻结定义与回执并解析审批人，构造本次启动计划。
    async fn prepare_change_runtime(
        &self,
        context: &SalesChangeApprovalContext,
        idempotency_key: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PreparedExecution> {
        let graph = load_bound_definition_graph(&self.db, &context.binding, executor).await?;
        let subject_version = context.sales_write.submission().submission_no;
        let existing_receipt =
            load_start_receipt(&self.db, &context.subject, subject_version, idempotency_key, executor)
                .await?;
        let graph = resolve_start_approvers(
            &self.db,
            &self.authorization,
            graph,
            actor.id(),
            existing_receipt.as_ref(),
            executor,
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
        let authorization = self.authorization.clone();
        recover_approval_command(original_error, || {
            let db = self.db.clone();
            let command = command.clone();
            let actor = actor.clone();
            let authorization = authorization.clone();
            self.db.client().with_transaction(move |executor| {
                Box::pin(
                    async move { replay_submission(&db, authorization, &actor, &command, executor).await },
                )
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
impl CreatedChangeOrderPersistInput {
    /// 用已授权来源和本事务准备的变更冻结注册、绑定与审计元数据。
    fn new(
        approvals: Arc<dyn ApprovalRuntimePort>,
        sales_write: CreatedChangeWrite,
        source: &SalesOrder,
        actor: &AuditActor,
    ) -> Result<Self> {
        let change_id = sales_write.change_id().to_string();
        let bind_command = BindPublishedDefinitionCommand {
            document_type: erp_workflow::entity::document_registry::DocumentType::SalesChangeOrder,
            business_object_id: change_id.clone(),
            business_object_version: sales_write.version(),
            context: BindingRevalidationContext::new(
                sales_change_responsible_org_id(sales_write.settlement_party_id())?,
                actor.id().into(),
            )
            .with_order_source(Some(OrderTaskSource::Sales(source.base.id.clone())))
            .with_business_org_unit_id(Some(source.business_org_unit_id.clone()))
            .with_scope_owner_user_id(Some(source.sales_owner_user_id.clone())),
        };
        let document = new_registered_document(
            change_id.clone(),
            erp_workflow::entity::document_registry::DocumentType::SalesChangeOrder,
            String::new(),
        )?;
        let audit =
            actor.clone().resource_log("sales_change_order.create", "sales_change_order", change_id)?;
        Ok(Self { approvals, sales_write, document, bind_command, audit, actor: actor.clone() })
    }
}

/// 在原写事务内先证明独立作废资格，再沿来源范围加载并持久化业务变化。
async fn void_change(
    db: &mongodb::Database,
    authorization: &Arc<AuthorizationService>,
    id: &str,
    request: VoidSalesChangeOrderRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorization
        .require(actor, &sales_review::SALES_CHANGE_ORDER_VOID, AccessTarget::Operation, executor)
        .await?;
    let change = db
        .sales_change_orders()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("销售变更单不存在或无权操作".into()))?;
    erp_read_models::sales_center::access::SalesAccess::new(db.clone(), authorization.clone())
        .require_object(actor, &sales_order::HANDOVER, change.sales_order_id.as_ref(), executor)
        .await?;
    let mut sales_write =
        SalesReviewService::new(db.clone()).prepare_void(id, request, actor, executor).await?;
    sales_write.persist(db, executor).await?;
    complete_document_rework(db, "sales_change_order", id, actor.id(), Instant::now(), executor).await?;
    let audit = actor.clone().resource_log("sales_change_order.void", "sales_change_order", id.into())?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}

/// 在创建事务内写入变更单、绑定发布定义并登记单据。
///
/// # 用途
/// 创建变更单时原子写入单据、工作副本与发布定义绑定。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 变更单、工作副本、注册行与审计
/// * `executor` - 入口授权和准备读取使用的原事务执行器。
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
    input: CreatedChangeOrderPersistInput,
    executor: &mut dyn Executor,
) -> Result<()> {
    let CreatedChangeOrderPersistInput { approvals, sales_write, mut document, bind_command, audit, actor } =
        input;
    persist_bound_change_document(db, approvals.as_ref(), &mut document, &bind_command, &actor, executor)
        .await?;
    sales_write.persist(db, executor).await?;
    persist_log(db, &audit, executor).await?;
    Ok(())
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

#[cfg(test)]
mod authorization_tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    /// 作废入口先检查独立动作资格，拒绝时不读取变更、不执行任何业务写入。
    #[tokio::test]
    async fn void_entry_rejects_before_loading_or_changing_business_facts() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("sales_change_void_test");
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let error = void_change(
            &db,
            &unavailable_authorization(),
            "change",
            VoidSalesChangeOrderRequest { version: 1 },
            &actor,
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
        let error = void_change(
            &db,
            &unavailable_authorization(),
            "change",
            VoidSalesChangeOrderRequest { version: 1 },
            &actor,
            &mut NoTransaction,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::ValidationError(message) if message == "写入授权必须使用快照事务"));
    }
}
