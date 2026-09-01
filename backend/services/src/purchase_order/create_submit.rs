//! 新建采购单后在同一事务内冻结并启动统一审批。

use database::PurchaseOrderExt;
use entities::common::time::Instant;
use entities::document_registry::BusinessDocument;
use entities::ids::{PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId};
use entities::purchase_order::{PurchaseOrder, PurchaseOrderSubmission, PurchaseOrderSubmissionLine};
use entities::sales_order::SalesOrder;
use id_generator::next_id;
use mongodb::{ClientSession, Database};

use super::adapter::{
    build_purchase_order_snapshot, execute_purchase_order_domain_action, purchase_order_adapter,
    purchase_order_object_readable, purchase_order_responsible_org_id, purchase_order_start_command,
    purchase_order_subject_ref, require_frozen_binding, start_approval_command_kind, RECENT_HISTORY_LIMIT,
};
use super::start_approval::{
    build_purchase_order_start_input, load_bound_definition_graph_with_executor,
    persist_purchase_order_start_with_session, PurchaseOrderStartInput, PurchaseOrderStartPersistInput,
};
use crate::approval::execution::prepare_start;
use crate::approval::policy::ApprovalDomainAction;
use crate::audit::AuditActor;
use crate::document_registry::{find_approval_binding, find_registered_document};
use crate::errors::{Error, Result};

/// 创建并提交后的正式号与乐观锁版本。
pub(super) struct SubmittedCreatedOrder {
    /// 已分配正式采购单号。
    pub purchase_no: String,
    /// 提交写入完成后的乐观锁版本。
    pub lock_version: u64,
}

/// 刚写入的草稿聚合。
struct CreatedDraftBundle {
    /// 草稿态采购单。
    order: PurchaseOrder,
    /// 可编辑草稿提交。
    draft: PurchaseOrderSubmission,
    /// 草稿提交行。
    draft_lines: Vec<PurchaseOrderSubmissionLine>,
    /// 已绑定定义的注册行。
    document: BusinessDocument,
}

/// 冻结后待写入启动计划的采购单。
struct FrozenCreatedDraft {
    /// 已进入审批中的采购单。
    order: PurchaseOrder,
    /// 已同步正式号的注册行。
    document: BusinessDocument,
    /// 已失效的旧草稿提交。
    superseded_draft: PurchaseOrderSubmission,
    /// 冻结提交头。
    submission: PurchaseOrderSubmission,
    /// 冻结提交行。
    submission_lines: Vec<PurchaseOrderSubmissionLine>,
}

/// 把刚创建的采购草稿冻结并启动审批。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `sales_order` - 来源销售单
/// * `order_id` - 刚写入的采购单主键
/// * `actor` - 审计操作人
/// * `idempotency_key` - 建单命令幂等键，复用于启动审批
/// * `session` - 与建单相同的事务会话
///
/// # 返回
/// 返回正式号与提交后乐观锁版本。
///
/// # 错误
/// 草稿缺失、无审批绑定、定义图非法或仓储写入失败时返回错误。
///
/// # 关键业务约束
/// 必须与建单同事务；失败时整批采购单一起回滚，不得留下未提交草稿。
pub(super) async fn submit_created_draft_in_session(
    db: &Database,
    sales_order: &SalesOrder,
    order_id: &str,
    actor: &AuditActor,
    idempotency_key: &str,
    session: &mut ClientSession,
) -> Result<SubmittedCreatedOrder> {
    let now = Instant::now();
    let mut bundle = load_created_draft_bundle(db, order_id, session).await?;
    assign_formal_identifiers(&mut bundle.order, &mut bundle.document, now)?;
    let frozen = freeze_created_order(db, bundle, actor, session).await?;
    persist_created_order_start(db, sales_order, frozen, actor, idempotency_key, now, session).await
}

/// 读取刚写入的草稿采购单、提交、明细和注册行。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order_id` - 采购单主键
/// * `session` - 建单事务会话
///
/// # 返回
/// 返回尚未提交的草稿聚合。
///
/// # 错误
/// 采购单、草稿提交或注册行缺失时返回错误。
///
/// # 关键业务约束
/// 必须用同一事务会话读取，才能看见尚未提交的写入。
async fn load_created_draft_bundle(
    db: &Database,
    order_id: &str,
    session: &mut ClientSession,
) -> Result<CreatedDraftBundle> {
    let order = load_created_order(db, order_id, session).await?;
    let draft_id = order
        .draft_submission_id()
        .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
    let draft = db
        .purchase_order_submissions()
        .find_by_id(&draft_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("草稿提交不存在".to_string()))?;
    let draft_lines = db
        .purchase_order()
        .list_submission_lines(&draft_id, session)
        .await?;
    let document = find_registered_document(db, order_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("业务单据未注册".to_string()))?;
    Ok(CreatedDraftBundle {
        order,
        draft,
        draft_lines,
        document,
    })
}

/// 读取刚写入的草稿采购单。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order_id` - 采购单主键
/// * `session` - 建单事务会话
///
/// # 返回
/// 返回仍处于草稿的采购单。
///
/// # 错误
/// 采购单不存在或已不是草稿时返回错误。
///
/// # 关键业务约束
/// 创建并提交路径不得处理已提交单据。
async fn load_created_order(
    db: &Database,
    order_id: &str,
    session: &mut ClientSession,
) -> Result<PurchaseOrder> {
    let order = db
        .purchase_orders()
        .find_by_id(order_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
    order
        .ensure_draft_for_submission()
        .map_err(|_| Error::ConflictError("采购单已提交或已生效，请勿重复提交".to_string()))?;
    Ok(order)
}

/// 首次提交时分配正式采购单号并同步注册行编号。
///
/// # 参数
/// * `order` - 待分配正式号的采购单
/// * `document` - 对应业务注册行
/// * `now` - 编号分配时间
///
/// # 返回
/// 分配成功返回 `Ok(())`。
///
/// # 错误
/// 编号非法或注册行已分配编号时返回错误。
///
/// # 关键业务约束
/// 正式号只分配一次；已有正式号时保持不变。
fn assign_formal_identifiers(
    order: &mut PurchaseOrder,
    document: &mut BusinessDocument,
    now: Instant,
) -> Result<()> {
    if order.purchase_no.is_empty() {
        order.assign_purchase_no(format!("PO-{}", order.base.id))?;
    }
    if document.document_no.is_empty() {
        document.assign_document_no(order.purchase_no.clone(), now)?;
    }
    Ok(())
}

/// 冻结草稿提交并推进采购单进入审批中。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `bundle` - 刚写入的草稿聚合
/// * `actor` - 提交人
/// * `session` - 建单事务会话
///
/// # 返回
/// 返回冻结提交与已进入审批中的采购单。
///
/// # 错误
/// 草稿状态非法、提交序号溢出或领域动作失败时返回错误。
///
/// # 关键业务约束
/// 旧草稿标记失效，正式提交是新快照，不得改写草稿行。
async fn freeze_created_order(
    db: &Database,
    bundle: CreatedDraftBundle,
    actor: &AuditActor,
    session: &mut ClientSession,
) -> Result<FrozenCreatedDraft> {
    let CreatedDraftBundle {
        mut order,
        draft,
        draft_lines,
        document,
    } = bundle;
    let mut superseded_draft = draft.clone();
    superseded_draft.mark_superseded()?;
    let (submission, submission_lines) =
        freeze_submission_from_created_draft(db, &order, &draft, &draft_lines, actor, session).await?;
    execute_purchase_order_domain_action(
        &mut order,
        ApprovalDomainAction::PurchaseOrderSubmit,
        &submission.base.id,
        actor.id(),
    )?;
    Ok(FrozenCreatedDraft {
        order,
        document,
        superseded_draft,
        submission,
        submission_lines,
    })
}

/// 按当前草稿派生正式提交头和行。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order` - 采购主表
/// * `draft` - 当前草稿提交
/// * `draft_lines` - 当前草稿行
/// * `actor` - 提交人
/// * `session` - 建单事务会话
///
/// # 返回
/// 返回冻结提交头与重新挂接的行。
///
/// # 错误
/// 提交序号溢出或草稿行不变式失败时返回错误。
///
/// # 关键业务约束
/// 序号只识别 `SUB-{n}`，忽略草稿编号。
async fn freeze_submission_from_created_draft(
    db: &Database,
    order: &PurchaseOrder,
    draft: &PurchaseOrderSubmission,
    draft_lines: &[PurchaseOrderSubmissionLine],
    actor: &AuditActor,
    session: &mut ClientSession,
) -> Result<(PurchaseOrderSubmission, Vec<PurchaseOrderSubmissionLine>)> {
    let existing = db
        .purchase_order()
        .list_submissions_by_order(&order.base.id.clone().into(), session)
        .await?;
    let formal = PurchaseOrderSubmission::freeze_from_draft(
        PurchaseOrderSubmissionId::new(next_id()),
        PurchaseOrderSubmission::next_submission_no(&existing)?,
        draft,
        Instant::now(),
        actor.id(),
    )?;
    let formal_id = PurchaseOrderSubmissionId::new(formal.base.id.clone());
    let mut lines = Vec::with_capacity(draft_lines.len());
    for line in draft_lines {
        lines.push(PurchaseOrderSubmissionLine::freeze_from_draft(
            PurchaseOrderSubmissionLineId::new(next_id()),
            formal_id.clone(),
            line,
        )?);
    }
    Ok((formal, lines))
}

/// 构造启动计划并与冻结提交同会话写入。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `sales_order` - 来源销售单
/// * `frozen` - 已冻结的采购提交
/// * `actor` - 提交人
/// * `idempotency_key` - 建单幂等键
/// * `now` - 调用方时间
/// * `session` - 建单事务会话
///
/// # 返回
/// 返回提交后的正式号与乐观锁版本。
///
/// # 错误
/// 无绑定、启动计划失败或仓储写入失败时返回错误。
///
/// # 关键业务约束
/// 审批人取自已发布定义，不接受客户端选择。
async fn persist_created_order_start(
    db: &Database,
    sales_order: &SalesOrder,
    frozen: FrozenCreatedDraft,
    actor: &AuditActor,
    idempotency_key: &str,
    now: Instant,
    session: &mut ClientSession,
) -> Result<SubmittedCreatedOrder> {
    let organization_id = purchase_order_responsible_org_id(sales_order)?;
    let _ = purchase_order_object_readable(&organization_id, actor.id())?;
    let snapshot = build_purchase_order_snapshot(
        &frozen.order,
        sales_order,
        &frozen.submission,
        &frozen.submission_lines,
        actor.id(),
        now,
    )?;
    let prepared = prepare_created_order_start(
        db,
        &frozen.order,
        &organization_id,
        actor,
        idempotency_key,
        now,
        session,
    )
    .await?;
    persist_frozen_created_order_start(
        db,
        frozen,
        prepared,
        snapshot,
        organization_id,
        actor,
        now,
        session,
    )
    .await
}

/// 由冻结绑定构造 `prepare_start` 结果。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order` - 已进入审批中的采购单
/// * `organization_id` - 单据责任组织
/// * `actor` - 提交人
/// * `idempotency_key` - 建单幂等键
/// * `now` - 调用方时间
/// * `session` - 建单事务会话
///
/// # 返回
/// 返回可写入的启动计划。
///
/// # 错误
/// 绑定缺失、定义图非法或入口节点缺失时返回错误。
///
/// # 关键业务约束
/// 新建提交没有既有启动收据。
async fn prepare_created_order_start(
    db: &Database,
    order: &PurchaseOrder,
    organization_id: &str,
    actor: &AuditActor,
    idempotency_key: &str,
    now: Instant,
    session: &mut ClientSession,
) -> Result<crate::approval::execution::PreparedExecution> {
    let binding = find_approval_binding(db, &order.base.id, session).await?;
    let binding = require_frozen_binding(binding.as_ref())?.clone();
    let graph = load_bound_definition_graph_with_executor(db, &binding, session).await?;
    let start = purchase_order_start_command(
        &order.base.id,
        order.approval_subject_version,
        actor.id(),
        idempotency_key,
    );
    let _ = (start_approval_command_kind(&start), RECENT_HISTORY_LIMIT);
    let start_input = build_purchase_order_start_input(PurchaseOrderStartInput {
        graph,
        binding: &binding,
        subject: purchase_order_subject_ref(&order.base.id)?,
        subject_version: order.approval_subject_version,
        actor_id: actor.id(),
        organization_id,
        idempotency_key,
        receipt: None,
        now,
    })?;
    prepare_start(start_input)
}

/// 写入冻结提交、运行事实，并回读提交后的采购单。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `frozen` - 已冻结的采购提交
/// * `prepared` - 启动计划
/// * `snapshot` - 不可变快照载荷
/// * `organization_id` - 单据责任组织
/// * `actor` - 提交人
/// * `now` - 调用方时间
/// * `session` - 建单事务会话
///
/// # 返回
/// 返回正式号与提交后乐观锁版本。
///
/// # 错误
/// 仓储写入失败或提交后采购单丢失时返回错误。
///
/// # 关键业务约束
/// 提交审计与建单收据分开展示，互不覆盖幂等键。
#[allow(clippy::too_many_arguments)]
async fn persist_frozen_created_order_start(
    db: &Database,
    frozen: FrozenCreatedDraft,
    prepared: crate::approval::execution::PreparedExecution,
    snapshot: entities::approval_integration::ApprovalSubjectSnapshotPayload,
    organization_id: String,
    actor: &AuditActor,
    now: Instant,
    session: &mut ClientSession,
) -> Result<SubmittedCreatedOrder> {
    let order_id = frozen.order.base.id.clone();
    let audit = create_submit_audit(actor, &frozen.order)?;
    persist_purchase_order_start_with_session(
        db,
        PurchaseOrderStartPersistInput {
            order: frozen.order,
            document: frozen.document,
            superseded_draft: frozen.superseded_draft,
            submission: frozen.submission,
            submission_lines: frozen.submission_lines,
            procurement_guard: None,
            snapshot_payload: snapshot,
            prepared,
            owner_role: purchase_order_adapter()?.owner_role,
            organization_id,
            now,
            audit,
            receipt: None,
        },
        session,
    )
    .await?;
    load_submitted_created_order(db, &order_id, session).await
}

/// 构造创建并提交的审计记录。
///
/// # 参数
/// * `actor` - 提交人
/// * `order` - 已分配正式号的采购单
///
/// # 返回
/// 返回稳定主键的提交审计。
///
/// # 错误
/// 审计构造失败时返回错误。
///
/// # 关键业务约束
/// 主键按采购单稳定，不复用独立提交接口的收据格式。
fn create_submit_audit(actor: &AuditActor, order: &PurchaseOrder) -> Result<entities::AuditLog> {
    actor.clone().resource_log_with_id(
        format!("purchase-create-submit-{}", order.base.id),
        "purchase_order.submit",
        "purchase_order",
        order.base.id.clone(),
        Some(format!("create_and_submit;purchase_no={}", order.purchase_no)),
    )
}

/// 回读提交后的采购单正式号与版本。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order_id` - 采购单主键
/// * `session` - 建单事务会话
///
/// # 返回
/// 返回正式号与乐观锁版本。
///
/// # 错误
/// 提交后采购单丢失时返回内部错误。
///
/// # 关键业务约束
/// 以事务内回读为准，不使用提交前内存版本。
async fn load_submitted_created_order(
    db: &Database,
    order_id: &str,
    session: &mut ClientSession,
) -> Result<SubmittedCreatedOrder> {
    let order = db
        .purchase_orders()
        .find_by_id(order_id, session)
        .await?
        .ok_or_else(|| Error::Internal("采购单提交后丢失".to_string()))?;
    Ok(SubmittedCreatedOrder {
        purchase_no: order.purchase_no,
        lock_version: order.base.version,
    })
}

#[cfg(test)]
mod tests {
    /// 创建并提交必须走统一 `start_approval`，不得停在草稿。
    #[test]
    fn create_submit_starts_unified_approval() {
        let production = include_str!("create_submit.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码必须存在");
        assert!(production.contains("execute_purchase_order_domain_action"));
        assert!(production.contains("ApprovalDomainAction::PurchaseOrderSubmit"));
        assert!(production.contains("persist_purchase_order_start_with_session"));
        assert!(production.contains("prepare_start"));
        assert!(production.contains("procurement_guard: None"));
    }
}
