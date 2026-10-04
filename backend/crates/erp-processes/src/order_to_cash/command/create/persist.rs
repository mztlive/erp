//! 建单原子写入步骤；所有领域、审批与审计写入沿用入口的 Executor。

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::SharedRbacService;
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::service::sales_order::SalesOrderService;
use erp_workflow::entity::document_registry::BusinessDocument;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::approval::binding::BindPublishedDefinitionCommand;
use erp_workflow::service::approval::execution::{PreparedExecution, prepare_start};
use erp_workflow::{ApprovalObjectReadPort, DocumentRegistryExt};
use mongodb::Database;
use persistence_core::Executor;

use super::replay_creation_event;
use super::submission::{CreationDraft, CreationPlan, CreationSubmit};
use crate::Result;
use crate::business_ownership::ensure_creation_org as ensure_order_creation_org;
use crate::order_to_cash::SalesOrderCommandProcess;
use crate::order_to_cash::adapters::catalog::CatalogQualificationAdapter;
use crate::order_to_cash::authorization::SalesCommandAccess;
use crate::order_to_cash::command::identity::{persist_bound_sales_document, sales_create_bind_command};
use crate::order_to_cash::start_approval::{
    SalesOrderRuntimeWriteInput, SalesOrderStartInput, build_sales_order_start_input,
    load_bound_definition_graph_with_executor, persist_runtime_writes,
};

/// 本次写入的认证、授权和引用；进入事务后按原顺序重新校验。
pub(super) struct CreationWriteContext {
    db: Database,
    rbac: SharedRbacService,
    object_read: Arc<dyn ApprovalObjectReadPort>,
    actor: AuditActor,
    access: SalesCommandAccess,
    bind_command: BindPublishedDefinitionCommand,
    sellable_refs: Vec<(String, String)>,
}

impl SalesOrderCommandProcess {
    /// 按绑定命令、授权源和可售引用的顺序准备写入上下文。
    /// # 参数
    /// `plan` 为本次建单计划；`actor` 和 `access` 保持认证命令身份。
    /// # 返回
    /// 返回可移入入口事务的依赖与参数，不执行持久化。
    /// # 错误
    /// 绑定输入、授权源或可售引用不完整时返回首个错误。
    pub(super) fn creation_write_context(
        &self,
        plan: &CreationPlan,
        actor: &AuditActor,
        access: &SalesCommandAccess,
    ) -> Result<CreationWriteContext> {
        let bind_command = sales_create_bind_command(plan.order(), actor)?;
        let rbac = self.require_rbac().cloned()?;
        let object_read = Arc::clone(&self.object_read);
        let sellable_refs = SalesOrderService::sellable_working_copy_refs(plan.working_copy_lines())?;
        Ok(CreationWriteContext {
            db: self.db.clone(),
            rbac,
            object_read,
            actor: actor.clone(),
            access: access.clone(),
            bind_command,
            sellable_refs,
        })
    }
}

/// 事务内先重验关联、创建和可售资格，再查证命令结果，未重放时才写入。
/// # 参数
/// `context`、`plan` 来自同一建单入口；`executor` 为入口开启的唯一写入事务。
/// # 返回
/// 重放返回原销售单 ID，新鲜写入返回 `None`。
/// # 错误
/// 资格、载荷身份或任一步写入失败时返回错误，由入口统一回滚或查证未知提交。
pub(super) async fn persist_creation(
    context: CreationWriteContext,
    plan: CreationPlan,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    ensure_creation_ready(&context, plan.order(), executor).await?;
    if let Some(order_id) =
        replay_creation_event(&context.db, plan.create_event(), context.actor.id(), &context.access, executor)
            .await?
    {
        return Ok(Some(order_id));
    }
    match plan {
        CreationPlan::Draft(draft) => persist_created_draft(&context, *draft, executor).await?,
        CreationPlan::Submit(submission) => {
            persist_created_submission(&context, *submission, executor).await?;
        },
    }
    Ok(None)
}

/// 草稿按组织、凭证、销售单、审批绑定、副本、事件的顺序原子形成。
async fn persist_created_draft(
    context: &CreationWriteContext,
    mut draft: CreationDraft,
    executor: &mut dyn Executor,
) -> Result<()> {
    let creation = &mut draft.creation;
    ensure_creation_org(&context.db, &creation.order, executor).await?;
    SalesOrderCommandProcess::persist_creation_evidence(
        &context.db,
        &context.rbac,
        &creation.order,
        &context.actor,
        executor,
    )
    .await?;
    let sales = SalesOrderService::new(context.db.clone());
    sales.create_order(&creation.order, executor).await?;
    persist_creation_binding(context, &mut creation.document, executor).await?;
    sales
        .create_working_copy(
            &creation.stable_lines,
            &creation.working_copy,
            &creation.working_copy_lines,
            executor,
        )
        .await?;
    draft.audit.persist(&context.db, executor).await?;
    Ok(())
}

/// 立即提交按绑定、凭证、审批计划、组织、业务事实、运行事实、双事件形成。
async fn persist_created_submission(
    context: &CreationWriteContext,
    mut submit: CreationSubmit,
    executor: &mut dyn Executor,
) -> Result<()> {
    let binding = persist_creation_binding(context, &mut submit.creation.document, executor).await?;
    SalesOrderCommandProcess::persist_creation_evidence(
        &context.db,
        &context.rbac,
        &submit.creation.order,
        &context.actor,
        executor,
    )
    .await?;
    let prepared = prepare_bound_submission(&context.db, &submit, &binding, &context.actor, executor).await?;
    ensure_creation_org(&context.db, &submit.creation.order, executor).await?;
    persist_submission_records(&context.db, &submit, executor).await?;
    persist_created_runtime(&context.db, &submit, prepared, executor).await?;
    let audit = submit.create_audit;
    audit.persist(&context.db, executor).await?;
    let audit = submit.submit_audit;
    audit.persist(&context.db, executor).await?;
    Ok(())
}

/// 沿用当前事务，冻结创建时的发布定义并保存单据注册行。
async fn persist_creation_binding(
    context: &CreationWriteContext,
    document: &mut BusinessDocument,
    executor: &mut dyn Executor,
) -> Result<ApprovalDefinitionBinding> {
    persist_bound_sales_document(
        &context.db,
        &context.rbac,
        context.object_read.as_ref(),
        document,
        &context.bind_command,
        &context.actor,
        executor,
    )
    .await
}

/// 绑定成功后才读取冻结定义并构造首次提交的审批计划。
async fn prepare_bound_submission(
    db: &Database,
    submit: &CreationSubmit,
    binding: &ApprovalDefinitionBinding,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<PreparedExecution> {
    let approval = &submit.approval;
    let graph = load_bound_definition_graph_with_executor(db, binding, executor).await?;
    let start_input = build_sales_order_start_input(SalesOrderStartInput {
        graph,
        binding,
        document_type: approval.ports.document_type,
        subject: approval.subject.clone(),
        subject_version: submit.submission.submission_no,
        actor_id: actor.id(),
        organization_id: &approval.organization_id,
        idempotency_key: &approval.idempotency_key,
        receipt: None,
        now: approval.now,
    })?;
    Ok(prepare_start(start_input)?)
}

/// 原子保存销售单、稳定行及锁定副本、冻结提交、工作流动作。
async fn persist_submission_records(
    db: &Database,
    submit: &CreationSubmit,
    executor: &mut dyn Executor,
) -> Result<()> {
    let creation = &submit.creation;
    let sales = SalesOrderService::new(db.clone());
    sales.create_order(&creation.order, executor).await?;
    sales
        .create_working_copy(
            &creation.stable_lines,
            &creation.working_copy,
            &creation.working_copy_lines,
            executor,
        )
        .await?;
    sales.create_submission(&submit.submission, &submit.submission_lines, executor).await?;
    db.workflow_actions().create(&submit.approval.workflow_action, executor).await?;
    Ok(())
}

/// 只对 Apply 写入运行事实；Replay 沿用已存在事实。
async fn persist_created_runtime(
    db: &Database,
    submit: &CreationSubmit,
    prepared: PreparedExecution,
    executor: &mut dyn Executor,
) -> Result<()> {
    if let PreparedExecution::Apply(writes) = prepared {
        let approval = &submit.approval;
        persist_runtime_writes(
            db,
            &writes,
            SalesOrderRuntimeWriteInput {
                document_type: approval.ports.document_type,
                snapshot_payload: &approval.snapshot,
                owner_role: approval.ports.owner_role,
                organization_id: &approval.organization_id,
                now: approval.now,
            },
            executor,
        )
        .await?;
    }
    Ok(())
}

/// 在原位置依次重验关联、创建资格与精确可售引用。
async fn ensure_creation_ready(
    context: &CreationWriteContext,
    order: &SalesOrder,
    executor: &mut dyn Executor,
) -> Result<()> {
    context.access.related_order(order, executor).await?;
    context.access.creation(order, executor).await?;
    SalesOrderService::new(context.db.clone())
        .ensure_sellable_refs(
            &context.sellable_refs,
            &CatalogQualificationAdapter::new(context.db.clone()),
            executor,
        )
        .await?;
    Ok(())
}

/// 在对应分支的原事务位置重验该单责任组织。
async fn ensure_creation_org(db: &Database, order: &SalesOrder, executor: &mut dyn Executor) -> Result<()> {
    ensure_order_creation_org(db, &order.sales_owner_user_id, &order.business_org_unit_id, executor).await
}
