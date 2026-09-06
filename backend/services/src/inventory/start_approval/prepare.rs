use bpm::engine::{DefinitionGraph, StartAssigneeBinding};
use bpm::ids::{
    ApprovalCommandReceiptId, ApprovalInstanceAssigneeId, ApprovalNodeExecutionId, ApprovalProcessInstanceId,
};
use bpm::model::types::ApprovalCommandKind;
use bpm::model::{IdempotencyKey, ParticipantId, SubjectRef, Timestamp};
use database::{AccessControlExt, ApprovalIntegrationExt, BpmExt, Executor, InventoryExt, NoTransaction};
use entities::approval_integration::ApprovalSubjectSnapshot;
use entities::common::time::Instant;
use entities::document_registry::DocumentType;
use entities::inventory::{StockAdjustment, StockAdjustmentLine, StockAdjustmentState};
use id_generator::next_id;
use mongodb::Database;

use super::super::adapter::require_frozen_binding;
use super::super::approval_query::load_approval_binding;
use super::super::dto::{ExpectedStockBalanceVersion, SubmitStockAdjustmentRequest};
use super::mapping::{
    build_adjustment_line_updates, is_supported_start_receipt_identity, stock_adjustment_start_identity,
    stock_adjustment_start_scopes,
};
use crate::approval::business_adapter::ensure_separation_of_duties;
use crate::approval::execution::authorization::converge_eligibility;
use crate::approval::execution::idempotency::{
    normalize_idempotency_key, payload_conflict_error, start_identity, ReceiptBranch, StartIdentityParams,
};
use crate::approval::execution::{
    prepare_start_with_identity, ExecutionCommandInput, PreparedExecution, StartExecutionInput,
};
use crate::approval::policy::require_process_required;
use crate::approval::process_kind::process_kind_of;
use crate::approval::{
    approval_actor_is_active_with_executor, approval_decide_scope_with_executor,
    approval_document_action_scope_with_executor, approval_document_read_scope_with_executor,
};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;
use entities::document_registry::business_document::ApprovalDefinitionBinding;

const STOCK_ADJUSTMENT_SUBMIT_FORBIDDEN: &str = "当前账号不可提交该库存调整单";

/// 加载绑定定义图。缺失时失败关闭，不得用空图启动。
///
/// # 参数
/// * `db` - 数据库
/// * `binding` - 创建时冻结的定义绑定
///
/// # 返回
/// 返回已持久化的定义图。
///
/// # 错误
/// 定义不存在或仓储失败时返回冲突或仓储错误。
pub async fn load_bound_definition_graph(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
) -> Result<DefinitionGraph> {
    load_bound_definition_graph_with_executor(db, binding, &mut NoTransaction).await
}

/// 在调用方数据库快照内加载冻结绑定对应的定义图。
pub(super) async fn load_bound_definition_graph_with_executor(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
    executor: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let graph = db
        .bpm_workflow()
        .load_definition_graph(&binding.approval_process_definition_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整单绑定的审批定义不存在".to_string()))?;
    Ok(engine_graph(graph))
}

/// 将仓储定义图转为引擎定义图。字段一一对应，不得在此补默认节点。
///
/// # 参数
/// * `graph` - 仓储一次批量读取结果
///
/// # 返回
/// 返回引擎可消费的定义图。
///
/// # 错误
/// 无。
fn engine_graph(graph: database::repository::bpm::DefinitionGraph) -> DefinitionGraph {
    DefinitionGraph {
        definition: graph.definition,
        nodes: graph.nodes,
        transitions: graph.transitions,
    }
}

/// 使用完整库存提交身份规划启动；禁止调用方在规划后覆盖 receipt digest。
pub fn prepare_stock_adjustment_start(
    input: StartExecutionInput,
    req: &SubmitStockAdjustmentRequest,
) -> Result<PreparedExecution> {
    if input.subject.subject_kind() != DocumentType::StockAdjustment.as_str()
        || input.subject.subject_id().trim().is_empty()
        || input.subject_version != req.expected_subject_version
    {
        return Err(Error::ValidationError(
            "库存调整启动输入与提交载荷不一致".to_string(),
        ));
    }
    let identity = stock_adjustment_start_identity(
        input.subject.subject_id(),
        req,
        input.actor.as_str(),
        &input.binding_id,
        input.definition_version,
    )?;
    prepare_start_with_identity(input, identity)
}

/// 按当前 V3、历史无前缀作用域顺序读取规范幂等键对应的启动收据。
async fn find_stock_adjustment_start_receipt(
    db: &Database,
    scopes: &[String],
    key: &IdempotencyKey,
    executor: &mut dyn Executor,
) -> Result<Option<bpm::model::ApprovalCommandReceipt>> {
    for scope in scopes {
        let receipt = db
            .bpm_workflow()
            .find_command_receipt(ApprovalCommandKind::StartApproval, scope, key, executor)
            .await?;
        if receipt.is_some() {
            return Ok(receipt);
        }
    }
    Ok(None)
}

/// 在同一数据库快照内先按稳定作用域解析启动收据，再重验当前权限与结果事实。
pub async fn reconcile_stock_adjustment_start_receipt(
    db: &Database,
    rbac: &SharedRbacService,
    adjustment_id: &str,
    req: &SubmitStockAdjustmentRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let key = normalize_idempotency_key(&req.idempotency_key)?;
    let scopes = stock_adjustment_start_scopes(adjustment_id, req.expected_subject_version)?;
    let receipt = find_stock_adjustment_start_receipt(db, &scopes, &key, executor).await?;
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.command_kind != ApprovalCommandKind::StartApproval
        || !scopes.iter().any(|scope| scope == &receipt.scope_id)
    {
        return Err(Error::ConflictError(
            "库存调整启动收据与命令作用域不一致".to_string(),
        ));
    }
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&ApprovalProcessInstanceId::new(&receipt.result_ref), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整启动收据引用的实例不存在".to_string()))?;
    if instance.process_kind != process_kind_of(DocumentType::StockAdjustment)
        || instance.subject.subject_kind() != DocumentType::StockAdjustment.as_str()
        || instance.subject.subject_id() != adjustment_id
        || instance.subject_version != req.expected_subject_version
        || instance.base.id != receipt.result_ref
    {
        return Err(Error::ConflictError(
            "库存调整启动收据与实例事实不一致".to_string(),
        ));
    }
    if instance.started_by.as_str() != actor.id() {
        return Err(Error::Forbidden(STOCK_ADJUSTMENT_SUBMIT_FORBIDDEN.to_string()));
    }
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(&instance.base.id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整启动实例缺少冻结快照".to_string()))?;
    snapshot
        .ensure_matches_runtime_subject(
            DocumentType::StockAdjustment,
            adjustment_id,
            req.expected_subject_version,
        )
        .map_err(|_| Error::ConflictError("库存调整启动实例与冻结快照不一致".to_string()))?;
    if snapshot.payload.submitted_by != instance.started_by.as_str() {
        return Err(Error::ConflictError(
            "库存调整启动实例与冻结提交人不一致".to_string(),
        ));
    }
    let binding = load_approval_binding(db, adjustment_id, executor).await?;
    let binding = require_frozen_binding(binding.as_ref())?;
    if instance.process_definition_id != binding.approval_process_definition_id
        || instance.definition_version != binding.approval_definition_version
    {
        return Err(Error::ConflictError(
            "库存调整启动实例与冻结定义绑定不一致".to_string(),
        ));
    }
    let identity = stock_adjustment_start_identity(
        adjustment_id,
        req,
        actor.id(),
        binding.approval_process_definition_id.as_ref(),
        binding.approval_definition_version,
    )?;
    if !matches!(identity.classify(Some(&receipt)), ReceiptBranch::SamePayload(_)) {
        return Err(payload_conflict_error());
    }
    let legacy_standard_identity = start_identity(StartIdentityParams {
        idempotency_key: key.clone(),
        process_kind: process_kind_of(DocumentType::StockAdjustment).as_str(),
        subject_kind: DocumentType::StockAdjustment.as_str(),
        subject_id: adjustment_id,
        subject_version: req.expected_subject_version,
        binding_id: binding.approval_process_definition_id.as_ref(),
        definition_version: binding.approval_definition_version,
        actor_participant_id: actor.id(),
    })?;
    let weak_legacy_receipt = matches!(
        legacy_standard_identity.classify(Some(&receipt)),
        ReceiptBranch::SamePayload(_)
    );
    let adjustment = db
        .inventory()
        .stock_adjustment(adjustment_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))?;
    if adjustment.approval_subject_version < req.expected_subject_version {
        return Err(Error::ConflictError(
            "库存调整启动收据早于当前业务事实".to_string(),
        ));
    }
    if weak_legacy_receipt
        && !legacy_start_payload_matches_result(db, &adjustment, &snapshot, req, actor, executor).await?
    {
        return Err(payload_conflict_error());
    }
    ensure_stock_adjustment_submit_authorized_with_executor(db, rbac, &adjustment, actor, executor).await?;
    Ok(Some(instance.base.id))
}

/// 按稳定 StartApproval 作用域与幂等键只读解析已提交结果。
///
/// 收据是第一读；不存在时必须返回 `None`，不得根据当前单据状态推断命令成功。
pub async fn find_stock_adjustment_start_result(
    db: &Database,
    rbac: &SharedRbacService,
    adjustment_id: &str,
    expected_subject_version: u32,
    idempotency_key: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let key = normalize_idempotency_key(idempotency_key)?;
    let scopes = stock_adjustment_start_scopes(adjustment_id, expected_subject_version)?;
    let receipt = find_stock_adjustment_start_receipt(db, &scopes, &key, executor).await?;
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.command_kind != ApprovalCommandKind::StartApproval
        || !is_supported_start_receipt_identity(&receipt.scope_id, &receipt.payload_digest, &scopes)
    {
        return Err(Error::ConflictError("库存调整提交结果收据身份不一致".to_string()));
    }
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&ApprovalProcessInstanceId::new(&receipt.result_ref), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整提交结果引用的审批实例不存在".to_string()))?;
    if instance.base.id != receipt.result_ref
        || instance.process_kind != process_kind_of(DocumentType::StockAdjustment)
        || instance.subject.subject_kind() != DocumentType::StockAdjustment.as_str()
        || instance.subject.subject_id() != adjustment_id
        || instance.subject_version != expected_subject_version
    {
        return Err(Error::ConflictError(
            "库存调整提交结果与审批实例事实不一致".to_string(),
        ));
    }
    if instance.started_by.as_str() != actor.id() {
        return Err(Error::NotFound("库存调整提交结果不存在".to_string()));
    }
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(&instance.base.id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整提交结果缺少冻结快照".to_string()))?;
    snapshot
        .ensure_matches_runtime_subject(
            DocumentType::StockAdjustment,
            adjustment_id,
            expected_subject_version,
        )
        .map_err(|_| Error::ConflictError("库存调整提交结果与冻结快照不一致".to_string()))?;
    if snapshot.payload.submitted_by != instance.started_by.as_str() {
        return Err(Error::ConflictError(
            "库存调整提交结果与冻结提交人不一致".to_string(),
        ));
    }
    let binding = load_approval_binding(db, adjustment_id, executor).await?;
    let binding = require_frozen_binding(binding.as_ref())?;
    if instance.process_definition_id != binding.approval_process_definition_id
        || instance.definition_version != binding.approval_definition_version
    {
        return Err(Error::ConflictError(
            "库存调整提交结果与冻结定义绑定不一致".to_string(),
        ));
    }
    let adjustment = db
        .inventory()
        .stock_adjustment(adjustment_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))?;
    if adjustment.approval_subject_version < expected_subject_version {
        return Err(Error::ConflictError(
            "库存调整提交结果早于当前业务事实".to_string(),
        ));
    }
    ensure_stock_adjustment_submit_authorized_with_executor(db, rbac, &adjustment, actor, executor).await?;
    Ok(Some(instance.base.id))
}

/// 对无版本前缀的历史收据，从未再修改的成功结果重建完整提交载荷。
///
/// 历史 generic digest 不含库存草稿、明细和余额版本；只有业务版本恰好推进一次、
/// 当前仍为该次审批结果，且所有可重建事实逐字段相等时才允许只读兼容回放。
async fn legacy_start_payload_matches_result(
    db: &Database,
    adjustment: &StockAdjustment,
    snapshot: &ApprovalSubjectSnapshot,
    req: &SubmitStockAdjustmentRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let expected_persisted_version = req.expected_version.checked_add(1);
    let expected_note = match req.note.trim() {
        "" => None,
        value => Some(value),
    };
    // 历史正式 caller 始终显式发送方向；`None` 与 `Some(当前方向)` 会得到同一
    // 持久化结果但属于不同 V1 载荷，因此弱 legacy 收据只能接受唯一旧 wire。
    if req.lines.iter().any(|line| line.direction.is_none())
        || expected_persisted_version != Some(adjustment.base.version)
        || adjustment.status != StockAdjustmentState::InApproval
        || adjustment.approval_subject_version != req.expected_subject_version
        || adjustment.reason_type != req.reason_type
        || adjustment.note.as_deref() != expected_note
        || adjustment.occurred_at != Some(Instant::from_unix_secs(req.occurred_at))
        || adjustment.prepared_by != actor.id()
    {
        return Ok(false);
    }
    let persisted_lines = db
        .inventory()
        .adjustment_lines_by_adjustment_ids(
            &[entities::ids::StockAdjustmentId::new(adjustment.base.id.clone())],
            executor,
        )
        .await?;
    let updates = match build_adjustment_line_updates(&req.lines) {
        Ok(updates) => updates,
        Err(_) => return Ok(false),
    };
    let mut requested_result = persisted_lines.clone();
    if adjustment
        .apply_line_updates(&mut requested_result, &updates, true)
        .is_err()
        || requested_result != persisted_lines
    {
        return Ok(false);
    }
    let reconstructed_snapshot = entities::inventory::StockAdjustmentApprovalSnapshot::build(
        adjustment,
        &persisted_lines,
        actor.id(),
        snapshot.payload.submitted_at,
    )?;
    if reconstructed_snapshot != snapshot.payload {
        return Ok(false);
    }
    legacy_balance_versions_match(db, adjustment, &persisted_lines, &req.balances, executor).await
}

/// 历史回放只在余额版本仍与原命令完全相等且一一覆盖明细维度时成立。
async fn legacy_balance_versions_match(
    db: &Database,
    adjustment: &StockAdjustment,
    lines: &[StockAdjustmentLine],
    expected: &[ExpectedStockBalanceVersion],
    executor: &mut dyn Executor,
) -> Result<bool> {
    let required_skus = lines
        .iter()
        .map(|line| line.sku_id.to_string())
        .collect::<std::collections::HashSet<_>>();
    let mut ids = std::collections::HashSet::with_capacity(expected.len());
    let mut covered_skus = std::collections::HashSet::with_capacity(expected.len());
    for item in expected {
        if !ids.insert(item.balance_id.as_str()) {
            return Ok(false);
        }
        let Some(balance) = db.stock_balances().find_by_id(&item.balance_id, executor).await? else {
            return Ok(false);
        };
        if balance.base.version != item.expected_version
            || balance.warehouse_id != adjustment.warehouse_id
            || !required_skus.contains(&balance.sku_id.to_string())
            || !covered_skus.insert(balance.sku_id.to_string())
        {
            return Ok(false);
        }
    }
    Ok(covered_skus == required_skus)
}

/// 在给定数据库快照内重验库存调整提交人的账号、动作权限、对象读取与范围。
pub async fn ensure_stock_adjustment_submit_authorized_with_executor(
    db: &Database,
    rbac: &SharedRbacService,
    adjustment: &StockAdjustment,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !approval_actor_is_active_with_executor(db, actor, executor).await?
        || adjustment.prepared_by != actor.id()
    {
        return Err(Error::Forbidden(STOCK_ADJUSTMENT_SUBMIT_FORBIDDEN.to_string()));
    }
    let organization_id = adjustment.warehouse_id.as_ref();
    let action_scope =
        approval_document_action_scope_with_executor(db, rbac, actor, "stock_adjustment:submit", executor)
            .await?;
    let read_scope =
        approval_document_read_scope_with_executor(db, rbac, actor, DocumentType::StockAdjustment, executor)
            .await?;
    if !action_scope.covers(organization_id) || !read_scope.covers(organization_id) {
        return Err(Error::Forbidden("无权提交该责任组织的库存调整单".to_string()));
    }
    Ok(())
}

/// 当前调用人是否可获得库存调整提交命令令牌。
pub async fn actor_can_submit(
    db: &Database,
    rbac: &SharedRbacService,
    adjustment: &StockAdjustment,
    actor: &AuditActor,
) -> Result<bool> {
    match ensure_stock_adjustment_submit_authorized_with_executor(
        db,
        rbac,
        adjustment,
        actor,
        &mut NoTransaction,
    )
    .await
    {
        Ok(()) => Ok(true),
        Err(Error::Forbidden(_)) => Ok(false),
        Err(error) => Err(error),
    }
}

/// 库存调整启动输入。
///
/// # 用途
/// 收拢 `build_stock_adjustment_start_input` 的定义图、绑定与提交人参数。
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
/// 审批人取自已发布节点，不接受客户端选择。
pub struct StockAdjustmentStartInput<'a> {
    /// 绑定定义图。
    pub graph: DefinitionGraph,
    /// 冻结绑定。
    pub binding: &'a ApprovalDefinitionBinding,
    /// 业务对象引用。
    pub subject: SubjectRef,
    /// 冻结提交版本。
    pub subject_version: u32,
    /// 提交人。
    pub actor_id: &'a str,
    /// 单据责任组织。
    pub organization_id: &'a str,
    /// 规范化前的幂等键。
    pub idempotency_key: &'a str,
    /// 已存在收据。
    pub receipt: Option<bpm::model::ApprovalCommandReceipt>,
    /// 调用方时间。
    pub now: Instant,
}

/// 由定义图与单据组织构造启动输入。
///
/// 审批人取自已发布节点，不接受客户端选择。对象读取权失败时收敛为 BLOCKED，
/// `prepare_start` 会拒绝创建实例。
///
/// # 用途
/// 把启动参数收敛为引擎 `prepare_start` 输入。
///
/// # 参数
/// * `input` - 定义图、绑定、主体与提交人
///
/// # 返回
/// 返回可交给 `prepare_start` 的输入。
///
/// # 错误
/// 入口缺失、审批人非法、幂等键非法或读取权校验失败时返回错误。
///
/// # 关键业务约束
/// 定义版本必须与冻结绑定一致；对象读取权失败时收敛为 BLOCKED。
pub async fn build_stock_adjustment_start_input(
    db: &Database,
    rbac: &SharedRbacService,
    input: StockAdjustmentStartInput<'_>,
    executor: &mut dyn Executor,
) -> Result<StartExecutionInput> {
    let StockAdjustmentStartInput {
        graph,
        binding,
        subject,
        subject_version,
        actor_id,
        organization_id,
        idempotency_key,
        receipt,
        now,
    } = input;
    if graph.definition.definition_version != binding.approval_definition_version {
        return Err(Error::ConflictError(
            "库存调整单绑定定义版本与已加载定义不一致".to_string(),
        ));
    }
    let idempotency_key = normalize_idempotency_key(idempotency_key)?;
    let actor =
        ParticipantId::new(actor_id).map_err(|_| Error::ValidationError("提交人引用无效".to_string()))?;
    let timestamp = Timestamp::from_utc(now.as_utc());
    let bindings = start_bindings_from_graph(db, rbac, &graph, actor_id, organization_id, executor).await?;
    let entry = graph
        .entry_node()
        .map_err(|_| Error::ConflictError("审批定义缺少入口节点".to_string()))?;
    let entry_eligibility = bindings
        .iter()
        .find(|item| item.node_key == entry.node_key)
        .map(|item| item.eligibility.clone())
        .ok_or_else(|| Error::ConflictError("入口节点缺少审批人绑定".to_string()))?;
    Ok(StartExecutionInput {
        command: ExecutionCommandInput {
            graph,
            current_eligibility: entry_eligibility.clone(),
            next_eligibility: entry_eligibility,
            receipt,
            idempotency_key,
            now: timestamp,
        },
        process_kind: process_kind_of(DocumentType::StockAdjustment),
        subject,
        subject_version,
        binding_id: binding.approval_process_definition_id.as_ref().to_string(),
        definition_version: binding.approval_definition_version,
        actor,
        instance_id: ApprovalProcessInstanceId::new(next_id()),
        entry_execution_id: ApprovalNodeExecutionId::new(next_id()),
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
        bindings,
    })
}

/// 为定义全部节点冻结启动绑定，并按单据组织重验对象读取权。
///
/// # 参数
/// * `graph` - 定义图
/// * `organization_id` - 单据责任组织
///
/// # 返回
/// 返回与节点一一对应的绑定。
///
/// # 错误
/// 节点审批人引用非法或显示名为空时返回校验错误。
async fn start_bindings_from_graph(
    db: &Database,
    rbac: &SharedRbacService,
    graph: &DefinitionGraph,
    initiator_id: &str,
    organization_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<StartAssigneeBinding>> {
    revalidate_stock_adjustment_start_candidates(db, rbac, graph, initiator_id, organization_id, executor)
        .await?;
    let mut bindings = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let assignee = node.assignee_participant_id.as_str();
        bindings.push(StartAssigneeBinding {
            id: ApprovalInstanceAssigneeId::new(next_id()),
            node_key: node.node_key.clone(),
            participant: node.assignee_participant_id.clone(),
            eligibility: converge_eligibility(assignee, &node.assignee_label_snapshot, None)?,
        });
    }
    Ok(bindings)
}

/// 在调用方快照内重验全部定义候选人的有效账号、决定权限、对象读取、范围与 SoD。
pub(super) async fn revalidate_stock_adjustment_start_candidates(
    db: &Database,
    rbac: &SharedRbacService,
    graph: &DefinitionGraph,
    initiator_id: &str,
    organization_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    if organization_id.trim().is_empty() {
        return Err(Error::ValidationError("库存调整责任组织不能为空".to_string()));
    }
    let assignee_ids = graph
        .nodes
        .iter()
        .map(|node| node.assignee_participant_id.as_str().to_string())
        .collect::<Vec<_>>();
    let policy = require_process_required(DocumentType::StockAdjustment)?;
    ensure_separation_of_duties(policy.separation_of_duties_policy, initiator_id, &assignee_ids)?;
    for node in &graph.nodes {
        let assignee = node.assignee_participant_id.as_str();
        let account = db
            .accounts()
            .find_approval_assignee_by_id(assignee, executor)
            .await?
            .filter(|account| account.is_active_backoffice())
            .ok_or_else(|| Error::ValidationError("指定审批人账号不存在、已停用或任职失效".to_string()))?;
        let assignee_actor = AuditActor::new(account.base.id.clone(), account.base.id.clone(), account.kind);
        let decide_scope = approval_decide_scope_with_executor(db, rbac, &assignee_actor, executor).await?;
        let read_scope = approval_document_read_scope_with_executor(
            db,
            rbac,
            &assignee_actor,
            DocumentType::StockAdjustment,
            executor,
        )
        .await?;
        if !decide_scope.covers(organization_id) {
            return Err(Error::ValidationError(
                "指定审批人缺少审批权限或数据范围不覆盖当前单据组织".to_string(),
            ));
        }
        if !read_scope.covers(organization_id) {
            return Err(Error::ValidationError(
                "指定审批人不能读取当前库存调整单".to_string(),
            ));
        }
    }
    if graph.nodes.is_empty() {
        return Err(Error::ConflictError(
            "审批定义没有节点，无法启动库存调整审批".to_string(),
        ));
    }
    Ok(())
}
