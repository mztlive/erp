//! 库存调整提交启动：加载定义图、构造 `prepare_start` 输入并持久化运行事实。

use bpm::engine::{DefinitionGraph, StartAssigneeBinding, TaskIntent};
use bpm::ids::{
    ApprovalCommandReceiptId, ApprovalInstanceAssigneeId, ApprovalNodeExecutionId, ApprovalProcessInstanceId,
};
use bpm::model::types::{
    ApprovalCommandKind, ApprovalExecutionAssignmentSource, ApprovalNodeExecutionStatus,
    ApprovalProcessInstanceStatus,
};
use bpm::model::{
    ApprovalNodeExecution, CommandPayloadField, IdempotencyKey, ParticipantId, SubjectRef, Timestamp,
};
use database::repository::bpm::ApprovalInstanceListProjection;
use database::{
    AccessControlExt, ApprovalIntegrationExt, BpmExt, DocumentRegistryExt, Executor, InventoryExt,
    NoTransaction, Transactional, WorkItemExt,
};
use entities::approval_integration::{
    ApprovalNotificationEventKind, ApprovalNotificationOutbox, ApprovalNotificationTemplateParams,
    ApprovalSubjectSnapshot, ApprovalSubjectSnapshotPayload,
};
use entities::common::time::Instant;
use entities::document_registry::DocumentType;
use entities::ids::{ApprovalNotificationOutboxId, ApprovalSubjectSnapshotId, WorkItemId};
use entities::inventory::{StockAdjustment, StockAdjustmentLine, StockAdjustmentState};
use entities::work_item::DocumentApprovalWorkItemData;
use entities::work_item::{WorkItem, WorkItemPriority};
use id_generator::next_id;
use mongodb::Database;

use super::dto::{ExpectedStockBalanceVersion, StockAdjustmentView, SubmitStockAdjustmentRequest};
use crate::approval::business_adapter::ensure_separation_of_duties;
use crate::approval::execution::apply_plan::PlannedWrites;
use crate::approval::execution::authorization::converge_eligibility;
use crate::approval::execution::idempotency::{
    legacy_payload_digest, legacy_standard_start_receipt_identity, legacy_start_receipt_identity,
    normalize_idempotency_key, payload_conflict_error, specialized_start_identity, start_identity,
    start_scope_candidates, PreparedCommandIdentity, ReceiptBranch,
};
use crate::approval::execution::{
    map_receipt_first_write_error, prepare_start_with_identity, ExecutionCommandInput, PreparedExecution,
    StartExecutionInput,
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

const STOCK_ADJUSTMENT_START_DIGEST_VERSION: &str = "STOCK_ADJUSTMENT_START_V1";
const STOCK_ADJUSTMENT_START_VARIANT: &str = "STOCK_ADJUSTMENT_SUBMISSION";
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
pub(super) async fn load_bound_definition_graph(
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

/// 返回库存调整启动命令的当前 V3 与历史无前缀作用域。
fn stock_adjustment_start_scopes(adjustment_id: &str, target_subject_version: u32) -> Result<Vec<String>> {
    let kind = process_kind_of(DocumentType::StockAdjustment);
    start_scope_candidates(
        kind.as_str(),
        DocumentType::StockAdjustment.as_str(),
        adjustment_id,
        target_subject_version,
    )
}

/// 对完整规范化库存调整提交载荷计算版本化摘要。
pub(super) fn stock_adjustment_start_digest(
    req: &SubmitStockAdjustmentRequest,
    actor_id: &str,
) -> Result<String> {
    let mut lines = super::build_adjustment_line_updates(&req.lines)?
        .into_iter()
        .map(|line| {
            (
                line.line_id,
                line.quantity.to_decimal().normalize().to_string(),
                line.direction
                    .map(|value| value.as_str())
                    .unwrap_or("")
                    .to_string(),
            )
        })
        .collect::<Vec<_>>();
    lines.sort();
    let mut balances = req
        .balances
        .iter()
        .map(|balance| (balance.balance_id.clone(), balance.expected_version.to_string()))
        .collect::<Vec<_>>();
    balances.sort();
    // 固定顺序 JSON tuple 负责字符串转义与字段边界。不得复用 U+001F/NULL
    // 拼接格式：note、ID 等外部文本可合法包含这些字符，拼接会产生碰撞。
    let canonical = serde_json::to_string(&(
        STOCK_ADJUSTMENT_START_DIGEST_VERSION,
        req.expected_version.to_string(),
        req.expected_subject_version.to_string(),
        req.reason_type.as_str(),
        lines,
        balances,
        req.note.trim(),
        req.occurred_at.to_string(),
        actor_id,
    ))
    .map_err(|error| Error::Internal(format!("库存调整提交摘要失败: {error}")))?;
    Ok(format!("v1:{}", legacy_payload_digest(&canonical)))
}

/// 构造库存调整完整提交载荷的当前 V3 身份，并登记两代精确历史候选。
fn stock_adjustment_start_identity(
    adjustment_id: &str,
    req: &SubmitStockAdjustmentRequest,
    actor_id: &str,
    binding_id: &str,
    definition_version: u32,
) -> Result<PreparedCommandIdentity> {
    let key = normalize_idempotency_key(&req.idempotency_key)?;
    let process_kind = process_kind_of(DocumentType::StockAdjustment);
    let mut lines = super::build_adjustment_line_updates(&req.lines)?
        .into_iter()
        .map(|line| {
            (
                line.line_id,
                line.quantity.to_decimal().normalize().to_string(),
                line.direction
                    .map(|value| value.as_str())
                    .unwrap_or("")
                    .to_string(),
            )
        })
        .collect::<Vec<_>>();
    lines.sort();
    let mut balances = req
        .balances
        .iter()
        .map(|balance| (balance.balance_id.clone(), balance.expected_version))
        .collect::<Vec<_>>();
    balances.sort();
    let occurred_at = req.occurred_at.to_string();
    let line_fields = lines
        .iter()
        .map(|(line_id, quantity, direction)| {
            CommandPayloadField::Sequence(vec![
                CommandPayloadField::Text(line_id),
                CommandPayloadField::Text(quantity),
                CommandPayloadField::Text(direction),
            ])
        })
        .collect::<Vec<_>>();
    let balance_fields = balances
        .iter()
        .map(|(balance_id, expected_version)| {
            CommandPayloadField::Sequence(vec![
                CommandPayloadField::Text(balance_id),
                CommandPayloadField::U64(*expected_version),
            ])
        })
        .collect::<Vec<_>>();
    let identity = specialized_start_identity(
        key,
        process_kind.as_str(),
        DocumentType::StockAdjustment.as_str(),
        adjustment_id,
        req.expected_subject_version,
        STOCK_ADJUSTMENT_START_VARIANT,
        vec![
            CommandPayloadField::Text(binding_id),
            CommandPayloadField::U32(definition_version),
            CommandPayloadField::U64(req.expected_version),
            CommandPayloadField::U32(req.expected_subject_version),
            CommandPayloadField::Text(req.reason_type.as_str()),
            CommandPayloadField::Sequence(line_fields),
            CommandPayloadField::Sequence(balance_fields),
            CommandPayloadField::Text(req.note.trim()),
            CommandPayloadField::Text(&occurred_at),
            CommandPayloadField::Text(actor_id),
        ],
    )?
    .with_legacy(legacy_start_receipt_identity(
        process_kind.as_str(),
        DocumentType::StockAdjustment.as_str(),
        adjustment_id,
        req.expected_subject_version,
        stock_adjustment_start_digest(req, actor_id)?,
    ))
    .with_legacy(legacy_standard_start_receipt_identity(
        process_kind.as_str(),
        DocumentType::StockAdjustment.as_str(),
        adjustment_id,
        req.expected_subject_version,
        binding_id,
        definition_version,
        actor_id,
    ));
    Ok(identity)
}

/// 使用完整库存提交身份规划启动；禁止调用方在规划后覆盖 receipt digest。
pub(super) fn prepare_stock_adjustment_start(
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
pub(super) async fn reconcile_stock_adjustment_start_receipt(
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
    let binding = super::load_approval_binding(db, adjustment_id, executor).await?;
    let binding = super::require_frozen_binding(binding.as_ref())?;
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
    let legacy_standard_identity = start_identity(
        key.clone(),
        process_kind_of(DocumentType::StockAdjustment).as_str(),
        DocumentType::StockAdjustment.as_str(),
        adjustment_id,
        req.expected_subject_version,
        binding.approval_process_definition_id.as_ref(),
        binding.approval_definition_version,
        actor.id(),
    )?;
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
pub(super) async fn find_stock_adjustment_start_result(
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
    let binding = super::load_approval_binding(db, adjustment_id, executor).await?;
    let binding = super::require_frozen_binding(binding.as_ref())?;
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

/// Unknown-result 查询仅接受作用域与摘要版本的精确历史配对。
fn is_supported_start_receipt_identity(scope_id: &str, payload_digest: &str, scopes: &[String]) -> bool {
    let [current_scope, legacy_scope] = scopes else {
        return false;
    };
    if scope_id == current_scope {
        return has_versioned_sha256(payload_digest, "v3:");
    }
    if scope_id == legacy_scope {
        return has_versioned_sha256(payload_digest, "v1:") || has_bare_sha256(payload_digest);
    }
    false
}

fn has_versioned_sha256(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(has_bare_sha256)
}

fn has_bare_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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
    let updates = match super::build_adjustment_line_updates(&req.lines) {
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
pub(super) async fn ensure_stock_adjustment_submit_authorized_with_executor(
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
pub(super) async fn actor_can_submit(
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
pub(super) struct StockAdjustmentStartInput<'a> {
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
pub(super) async fn build_stock_adjustment_start_input(
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

/// 库存调整启动事务写入集合。
///
/// # 用途
/// 收拢提交后需一并写入的调整单、快照、启动计划与审计身份。
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
/// 运行事实、不可变快照与入口任务必须与单据迁移同事务。
pub(super) struct StockAdjustmentStartPersistInput {
    /// 授权服务；事务内重验提交人与节点候选人权限。
    pub rbac: SharedRbacService,
    /// 已进入 `IN_APPROVAL` 的调整单。
    pub adjustment: StockAdjustment,
    /// 审计操作人。
    pub actor: AuditActor,
    /// 调整单主键。
    pub id: String,
    /// 冻结快照载荷。
    pub snapshot_payload: ApprovalSubjectSnapshotPayload,
    /// `prepare_start` 的 Apply 写入计划。
    pub writes: PlannedWrites,
    /// 创建时冻结的定义绑定。
    pub binding: ApprovalDefinitionBinding,
    /// 合同签署的责任角色。
    pub owner_role: &'static str,
    /// 责任组织。
    pub organization_id: String,
    /// 调用方时间。
    pub now: Instant,
    /// 已完成服务端校验、用于冻结快照的最终明细。
    pub lines: Vec<StockAdjustmentLine>,
    /// 提交时必须仍匹配的余额版本。
    pub balances: Vec<ExpectedStockBalanceVersion>,
    /// 提交命令读取到的单据乐观锁版本。
    pub expected_document_version: u64,
    /// 本次启动必须冻结的审批主题版本。
    pub expected_subject_version: u32,
}

/// 在同一事务中写入单据迁移、快照、BPM 运行事实与入口任务。
///
/// # 用途
/// 提交启动后原子写入调整单、快照与运行事实。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 调整单、快照与启动计划
///
/// # 返回
/// 返回提交后的调整单视图。
///
/// # 错误
/// 仓储写入失败或计划不完整时返回错误，事务回滚。
///
/// # 关键业务约束
/// Replay 不得重复写运行事实；Apply 必须写入快照与入口任务。
pub(super) async fn persist_stock_adjustment_start(
    db: &Database,
    input: StockAdjustmentStartPersistInput,
) -> Result<StockAdjustmentView> {
    let StockAdjustmentStartPersistInput {
        rbac,
        adjustment,
        actor,
        id,
        snapshot_payload,
        writes,
        binding,
        owner_role,
        organization_id,
        now,
        lines,
        balances,
        expected_document_version,
        expected_subject_version,
    } = input;
    let audit = actor
        .clone()
        .resource_log("stock_adjustment.submit", "stock_adjustment", id.clone())?;
    let db = db.clone();
    let client = db.client().clone();
    let updated = client
        .with_transaction(move |session| {
            Box::pin(async move {
                let current = db
                    .inventory()
                    .stock_adjustment(&id, session)
                    .await?
                    .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))?;
                ensure_fresh_start_document(
                    &current,
                    &adjustment,
                    expected_document_version,
                    expected_subject_version,
                )?;
                ensure_stock_adjustment_submit_authorized_with_executor(
                    &db, &rbac, &current, &actor, session,
                )
                .await?;
                let persisted_binding = super::load_approval_binding(&db, &id, session).await?;
                let persisted_binding = super::require_frozen_binding(persisted_binding.as_ref())?;
                if persisted_binding != &binding {
                    return Err(Error::ConflictError(
                        "库存调整审批定义绑定已变化，请刷新后重试".to_string(),
                    ));
                }
                let graph =
                    load_bound_definition_graph_with_executor(&db, persisted_binding, session).await?;
                revalidate_stock_adjustment_start_candidates(
                    &db,
                    &rbac,
                    &graph,
                    actor.id(),
                    &organization_id,
                    session,
                )
                .await?;
                revalidate_start_lines(&db, &current, &lines, session).await?;
                validate_balance_versions(&db, &adjustment, &lines, &balances, session).await?;
                validate_start_writes(
                    &writes,
                    &graph,
                    &binding,
                    &id,
                    actor.id(),
                    expected_subject_version,
                )?;
                // 命令收据是事务内第一笔写入。并发 loser 退出失败事务后只允许
                // 使用新会话回读 winner，不得先留下任何业务或 BPM 写入。
                db.bpm_workflow()
                    .insert_command_receipt(&writes.receipt, session)
                    .await
                    .map_err(map_receipt_first_write_error)?;
                let guarded = db
                    .business_documents()
                    .mark_approval_started(
                        &id,
                        DocumentType::StockAdjustment,
                        &writes.instance.process_definition_id,
                        writes.instance.definition_version,
                        now,
                        session,
                    )
                    .await?;
                if guarded.is_none() {
                    return Err(Error::ConflictError(
                        "库存调整单审批启动守卫冲突，请刷新后重试".to_string(),
                    ));
                }
                persist_runtime_writes(
                    &db,
                    &writes,
                    &snapshot_payload,
                    StartRuntimeContext {
                        owner_role,
                        organization_id: &organization_id,
                        document_no: &adjustment.adjustment_no,
                        submitted_by: actor.id(),
                        now,
                    },
                    session,
                )
                .await?;
                for line in &lines {
                    if !db
                        .inventory()
                        .update_adjustment_line(&line.base.id, line.quantity, Some(line.direction), session)
                        .await?
                    {
                        return Err(Error::NotFound("调整明细不存在".to_string()));
                    }
                }
                let mut adjustment = adjustment;
                db.stock_adjustments().update(&mut adjustment, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<StockAdjustment, crate::errors::Error>(adjustment)
            })
        })
        .await?;
    Ok(updated.into())
}

/// 校验事务内草稿与预先构造的启动后单据仍为同一条原子迁移。
fn ensure_fresh_start_document(
    current: &StockAdjustment,
    target: &StockAdjustment,
    expected_document_version: u64,
    expected_subject_version: u32,
) -> Result<()> {
    let expected_next_subject = current
        .approval_subject_version
        .checked_add(1)
        .ok_or_else(|| Error::ConflictError("库存调整审批主题版本已达上限".to_string()))?;
    if current.base.id != target.base.id
        || current.base.version != expected_document_version
        || target.base.version != expected_document_version
        || current.status != StockAdjustmentState::Draft
        || target.status != StockAdjustmentState::InApproval
        || expected_next_subject != expected_subject_version
        || target.approval_subject_version != expected_subject_version
        || current.warehouse_id != target.warehouse_id
        || current.prepared_by != target.prepared_by
    {
        return Err(Error::ConflictError(
            "库存调整单事务内版本、状态或审批主题已变化".to_string(),
        ));
    }
    Ok(())
}

/// 在事务快照内重读明细身份与版本；数量和方向可由本次提交命令修改。
async fn revalidate_start_lines(
    db: &Database,
    current: &StockAdjustment,
    target_lines: &[StockAdjustmentLine],
    executor: &mut dyn Executor,
) -> Result<()> {
    let persisted = db
        .inventory()
        .adjustment_lines_by_adjustment_ids(
            &[entities::ids::StockAdjustmentId::new(current.base.id.clone())],
            executor,
        )
        .await?;
    if persisted.len() != target_lines.len()
        || persisted.iter().any(|line| {
            !target_lines.iter().any(|target| {
                target.base.id == line.base.id
                    && target.base.version == line.base.version
                    && target.stock_adjustment_id == line.stock_adjustment_id
                    && target.sku_id == line.sku_id
            })
        })
    {
        return Err(Error::ConflictError(
            "库存调整明细身份或版本已变化，请刷新后重试".to_string(),
        ));
    }
    Ok(())
}

/// 校验引擎启动计划精确绑定本次业务命令。
fn validate_start_writes(
    writes: &PlannedWrites,
    graph: &DefinitionGraph,
    binding: &ApprovalDefinitionBinding,
    adjustment_id: &str,
    actor_id: &str,
    expected_subject_version: u32,
) -> Result<()> {
    let expected_scope = stock_adjustment_start_scopes(adjustment_id, expected_subject_version)?
        .into_iter()
        .next()
        .ok_or_else(|| Error::Internal("库存调整启动命令缺少 V3 scope".to_string()))?;
    if writes.receipt.command_kind != ApprovalCommandKind::StartApproval
        || writes.receipt.scope_id != expected_scope
        || writes.receipt.result_ref != writes.instance.base.id
        || writes.instance.process_kind != process_kind_of(DocumentType::StockAdjustment)
        || writes.instance.subject.subject_kind() != DocumentType::StockAdjustment.as_str()
        || writes.instance.subject.subject_id() != adjustment_id
        || writes.instance.subject_version != expected_subject_version
        || writes.instance.process_definition_id != binding.approval_process_definition_id
        || writes.instance.definition_version != binding.approval_definition_version
        || writes.instance.started_by.as_str() != actor_id
        || writes.instance.status != ApprovalProcessInstanceStatus::Running
        || writes.instance.current_round_no != 1
        || writes.instance.blocker_code.is_some()
        || writes.instance.ended_at.is_some()
    {
        return Err(Error::Internal(
            "库存调整启动计划与签署命令身份不一致".to_string(),
        ));
    }
    let [first] = writes.created_executions.as_slice() else {
        return Err(Error::Internal(
            "库存调整启动计划必须且只能创建一个入口执行".to_string(),
        ));
    };
    if first.process_instance_id.as_ref() != writes.instance.base.id
        || first.round_no != writes.instance.current_round_no
        || first.status != ApprovalNodeExecutionStatus::Active
        || first.assignment_source != ApprovalExecutionAssignmentSource::Definition
        || first.replaces_execution_id.is_some()
        || writes
            .instance
            .current_node_execution_id
            .as_ref()
            .map(|id| id.as_ref())
            != Some(first.base.id.as_str())
    {
        return Err(Error::Internal(
            "库存调整启动计划的入口执行身份不一致".to_string(),
        ));
    }
    let entry = graph
        .entry_node()
        .map_err(|_| Error::Internal("库存调整事务内定义缺少入口节点".to_string()))?;
    if first.node_key != entry.node_key
        || first.node_name != entry.node_name
        || first.assignee_participant_id != entry.assignee_participant_id
        || first.assignee_name_snapshot != entry.assignee_label_snapshot
    {
        return Err(Error::ConflictError(
            "库存调整启动入口执行与事务内定义事实不一致".to_string(),
        ));
    }
    let [TaskIntent::HumanTaskRequested {
        execution_id,
        assignee,
        node_key,
        node_name,
        round_no,
    }] = writes.create_tasks.as_slice()
    else {
        return Err(Error::Internal(
            "库存调整启动计划必须且只能创建一个入口任务".to_string(),
        ));
    };
    if execution_id.as_ref() != first.base.id
        || assignee != &first.assignee_participant_id
        || node_key != &first.node_key
        || node_name != &first.node_name
        || *round_no != first.round_no
    {
        return Err(Error::Internal(
            "库存调整启动计划的入口任务身份不一致".to_string(),
        ));
    }
    if writes.created_assignees.len() != graph.nodes.len()
        || graph.nodes.iter().any(|node| {
            !writes.created_assignees.iter().any(|assignee| {
                assignee.process_instance_id.as_ref() == writes.instance.base.id
                    && assignee.node_key == node.node_key
                    && assignee.definition_assignee_participant_id == node.assignee_participant_id
                    && assignee.current_assignee_participant_id == node.assignee_participant_id
            })
        })
        || !writes.created_assignees.iter().any(|assignee| {
            assignee.node_key == first.node_key
                && assignee.current_assignee_participant_id == first.assignee_participant_id
        })
        || !writes.updated_executions.is_empty()
        || !writes.complete_tasks.is_empty()
        || !writes.close_tasks.is_empty()
    {
        return Err(Error::Internal(
            "库存调整启动计划的审批人绑定或写入集合不一致".to_string(),
        ));
    }
    Ok(())
}

/// 在提交事务内校验余额身份、维度与乐观锁版本。
///
/// # 参数
/// * `db` - 数据库
/// * `adjustment` - 待提交调整单
/// * `lines` - 最终调整明细
/// * `expected` - 客户端编辑时冻结的余额版本
/// * `session` - 当前事务
///
/// # 错误
/// 余额缺失、版本冲突、重复或不能完整覆盖明细维度时返回错误。
async fn validate_balance_versions(
    db: &Database,
    adjustment: &StockAdjustment,
    lines: &[StockAdjustmentLine],
    expected: &[ExpectedStockBalanceVersion],
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let mut balance_ids = std::collections::HashSet::with_capacity(expected.len());
    let mut covered_dimensions = std::collections::HashSet::with_capacity(expected.len());
    for item in expected {
        if !balance_ids.insert(item.balance_id.as_str()) {
            return Err(Error::ValidationError("库存余额版本行不得重复".to_string()));
        }
        let balance = db
            .stock_balances()
            .find_by_id(&item.balance_id, session)
            .await?
            .ok_or_else(|| Error::NotFound("库存余额不存在".to_string()))?;
        if balance.base.version != item.expected_version {
            return Err(Error::ConflictError("库存余额已变化，请刷新后重试".to_string()));
        }
        if balance.warehouse_id != adjustment.warehouse_id
            || !lines.iter().any(|line| line.sku_id == balance.sku_id)
        {
            return Err(Error::ValidationError("库存余额与调整单维度不一致".to_string()));
        }
        covered_dimensions.insert((balance.warehouse_id.to_string(), balance.sku_id.to_string()));
    }
    if lines.iter().any(|line| {
        !covered_dimensions.contains(&(adjustment.warehouse_id.to_string(), line.sku_id.to_string()))
    }) {
        return Err(Error::ValidationError(
            "提交缺少调整明细对应的库存余额版本".to_string(),
        ));
    }
    Ok(())
}

/// 将启动计划写入 BPM 集合、不可变快照和入口 WorkItem。
///
/// # 参数
/// * `db` - 数据库
/// * `writes` - 启动写入集合
/// * `snapshot_payload` - 快照载荷
/// * `owner_role` - 责任角色
/// * `organization_id` - 责任组织
/// * `now` - 调用方时间
/// * `session` - 当前事务
///
/// # 返回
/// 成功时无返回值。
///
/// # 错误
/// 计划缺少入口执行或写入失败时返回错误。
struct StartRuntimeContext<'a> {
    owner_role: &'a str,
    organization_id: &'a str,
    document_no: &'a str,
    submitted_by: &'a str,
    now: Instant,
}

async fn persist_runtime_writes(
    db: &Database,
    writes: &PlannedWrites,
    snapshot_payload: &ApprovalSubjectSnapshotPayload,
    context: StartRuntimeContext<'_>,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let first = writes
        .created_executions
        .first()
        .ok_or_else(|| Error::Internal("启动计划缺少入口执行，不得提交库存调整".to_string()))?;
    db.bpm_workflow()
        .create_bpm_runtime_after_receipt(
            &writes.instance,
            &writes.created_assignees,
            first,
            &list_projection_from_execution(first, context.now),
            session,
        )
        .await?;
    let snapshot = ApprovalSubjectSnapshot::new(
        ApprovalSubjectSnapshotId::new(next_id()),
        ApprovalProcessInstanceId::new(writes.instance.base.id.clone()),
        DocumentType::StockAdjustment,
        writes.instance.subject.subject_id(),
        writes.instance.subject_version,
        snapshot_payload.clone(),
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;
    db.approval_subject_snapshots()
        .create_immutable_snapshot(&snapshot, session)
        .await?;
    persist_open_tasks(
        db,
        writes,
        context.owner_role,
        context.organization_id,
        context.now,
        session,
    )
    .await?;
    persist_start_notifications(
        db,
        writes,
        first,
        context.document_no,
        context.submitted_by,
        context.now,
        session,
    )
    .await
}

/// 严格消费启动计划的 `Started`/`Entered` 通知意图并同事务追加 outbox。
async fn persist_start_notifications(
    db: &Database,
    writes: &PlannedWrites,
    first: &ApprovalNodeExecution,
    document_no: &str,
    submitted_by: &str,
    now: Instant,
    executor: &mut dyn Executor,
) -> Result<()> {
    let notification_identities = writes
        .notifications
        .iter()
        .map(|intent| (intent.event_kind, intent.dedup_key.clone()))
        .collect::<Vec<_>>();
    validate_start_notification_identities(
        &notification_identities,
        &writes.instance.base.id,
        &first.base.id,
    )?;
    for intent in &writes.notifications {
        let mut recipients = vec![first.assignee_participant_id.as_str().to_string()];
        if intent.event_kind == ApprovalNotificationEventKind::Started {
            recipients.push(submitted_by.to_string());
        }
        recipients.sort();
        recipients.dedup();
        let record = ApprovalNotificationOutbox::enqueue(
            ApprovalNotificationOutboxId::new(intent.dedup_key.clone()),
            intent.dedup_key.clone(),
            intent.event_kind,
            recipients,
            ApprovalNotificationTemplateParams {
                document_type_label: DocumentType::StockAdjustment.label().to_string(),
                document_no: document_no.to_string(),
                current_node_name: first.node_name.clone(),
                current_approver_display_name: first.assignee_name_snapshot.clone(),
                round_no: writes.instance.current_round_no,
                reject_reason_summary: None,
            },
            now,
        )
        .map_err(|error| Error::ValidationError(error.to_string()))?;
        db.approval_notification_outbox()
            .create(&record, executor)
            .await?;
    }
    Ok(())
}

/// 校验启动通知恰为一条 Started 与一条 Entered，且去重键绑定运行身份。
fn validate_start_notification_identities(
    notifications: &[(ApprovalNotificationEventKind, String)],
    instance_id: &str,
    execution_id: &str,
) -> Result<()> {
    let expected_started = format!("started:{instance_id}");
    let expected_entered = format!("entered:{execution_id}");
    if notifications.len() != 2
        || notifications
            .iter()
            .filter(|(kind, key)| *kind == ApprovalNotificationEventKind::Started && key == &expected_started)
            .count()
            != 1
        || notifications
            .iter()
            .filter(|(kind, key)| *kind == ApprovalNotificationEventKind::Entered && key == &expected_entered)
            .count()
            != 1
    {
        return Err(Error::Internal(
            "库存调整启动计划必须包含唯一且身份一致的 Started/Entered 通知意图".to_string(),
        ));
    }
    Ok(())
}

/// 由入口执行构造有界列表投影。
///
/// # 参数
/// * `execution` - 入口执行
/// * `now` - 状态变更时间
///
/// # 返回
/// 返回启动时的列表投影。
///
/// # 错误
/// 无。
fn list_projection_from_execution(
    execution: &ApprovalNodeExecution,
    now: Instant,
) -> ApprovalInstanceListProjection {
    ApprovalInstanceListProjection {
        current_node_key: Some(execution.node_key.clone()),
        current_node_name: Some(execution.node_name.clone()),
        current_assignee_participant_id: Some(execution.assignee_participant_id.as_str().to_string()),
        current_assignee_name: Some(execution.assignee_name_snapshot.clone()),
        latest_rejected_execution_id: None,
        latest_rejection_summary: None,
        last_status_changed_at: Some(now.unix_secs()),
    }
}

/// 将 `HumanTaskRequested` 映射为 `DOCUMENT_APPROVAL` 任务并写入。
///
/// # 参数
/// * `db` - 数据库
/// * `writes` - 启动写入
/// * `owner_role` - 责任角色
/// * `organization_id` - 责任组织
/// * `now` - 创建时间
/// * `session` - 当前事务
///
/// # 返回
/// 成功时无返回值。
///
/// # 错误
/// 责任人为空或仓储失败时返回错误。
async fn persist_open_tasks(
    db: &Database,
    writes: &PlannedWrites,
    owner_role: &str,
    organization_id: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    for intent in &writes.create_tasks {
        let TaskIntent::HumanTaskRequested {
            execution_id,
            assignee,
            ..
        } = intent
        else {
            continue;
        };
        let item = WorkItem::new_document_approval(
            WorkItemId::new(next_id()),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: execution_id.clone(),
                business_object_type: DocumentType::StockAdjustment.as_str().to_string(),
                business_object_id: writes.instance.subject.subject_id().to_string(),
                subject_version: writes.instance.subject_version.to_string(),
                owner_role: owner_role.to_string(),
                owner_organization_id: organization_id.to_string(),
                owner_user_id: assignee.as_str().to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            now,
        )?;
        db.work_items().create(&item, session).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        is_supported_start_receipt_identity, legacy_payload_digest, list_projection_from_execution,
        stock_adjustment_start_digest, stock_adjustment_start_identity, stock_adjustment_start_scopes,
        validate_start_notification_identities, ReceiptBranch,
    };
    use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
    use bpm::model::{
        ApprovalCommandReceipt, ApprovalNodeExecution, NewNodeExecution, ParticipantId, Timestamp,
    };
    use entities::approval_integration::ApprovalNotificationEventKind;
    use entities::common::time::Instant;
    use entities::inventory::{AdjustmentReasonType, MovementDirection};

    use crate::inventory::dto::{
        ExpectedStockBalanceVersion, StockAdjustmentLineUpdateInput, SubmitStockAdjustmentRequest,
    };

    fn assert_start_write_order(source: &str, expected_paths: usize, guard_marker: &str) {
        let production = source.split("#[cfg(test)]").next().expect("生产代码必须存在");
        assert_eq!(
            production.matches(".insert_command_receipt(").count(),
            expected_paths,
            "每条 Fresh 启动路径必须恰有一个 receipt-first 写入"
        );
        assert_eq!(
            production.matches(guard_marker).count(),
            expected_paths,
            "每条 Fresh 启动路径必须恰有一个注册表启动守卫"
        );
        assert_eq!(
            production.matches(".create_bpm_runtime_after_receipt(").count(),
            expected_paths,
            "每条 Fresh 启动路径必须使用 receipt 已仲裁的 BPM 写入口"
        );
        assert!(
            !production.contains(".create_bpm_runtime("),
            "业务启动路径不得回退到 receipt-last 仓储入口"
        );

        let mut cursor = 0;
        for _ in 0..expected_paths {
            let receipt = production[cursor..]
                .find(".insert_command_receipt(")
                .map(|offset| cursor + offset)
                .expect("缺少启动收据写入");
            let guard = production[receipt..]
                .find(guard_marker)
                .map(|offset| receipt + offset)
                .expect("启动收据后缺少注册表守卫");
            let runtime = production[guard..]
                .find(".create_bpm_runtime_after_receipt(")
                .map(|offset| guard + offset)
                .expect("注册表守卫后缺少 BPM 运行事实写入");
            assert!(receipt < guard && guard < runtime);
            cursor = runtime + ".create_bpm_runtime_after_receipt(".len();
        }
    }

    fn execution() -> ApprovalNodeExecution {
        ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new("e1"),
            process_instance_id: ApprovalProcessInstanceId::new("inst-1"),
            node_key: "n1".into(),
            node_name: "仓储复核".into(),
            round_no: 1,
            execution_no: 1,
            assignment_source: bpm::model::types::ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: ParticipantId::new("u1").unwrap(),
            assignee_name_snapshot: "张三".into(),
            at: Timestamp::from_unix_secs(10).unwrap(),
        })
        .expect("入口执行夹具")
    }

    fn submit_request() -> SubmitStockAdjustmentRequest {
        SubmitStockAdjustmentRequest {
            expected_version: 7,
            expected_subject_version: 2,
            reason_type: AdjustmentReasonType::StockGain,
            lines: vec![
                StockAdjustmentLineUpdateInput {
                    line_id: "line-b".to_string(),
                    quantity: "2.500".to_string(),
                    direction: Some(MovementDirection::Increase),
                },
                StockAdjustmentLineUpdateInput {
                    line_id: "line-a".to_string(),
                    quantity: "1".to_string(),
                    direction: None,
                },
            ],
            balances: vec![
                ExpectedStockBalanceVersion {
                    balance_id: "balance-b".to_string(),
                    expected_version: 11,
                },
                ExpectedStockBalanceVersion {
                    balance_id: "balance-a".to_string(),
                    expected_version: 9,
                },
            ],
            note: " 盘点 NULL\u{1f}备注 ".to_string(),
            occurred_at: 42,
            idempotency_key: "submit-1".to_string(),
        }
    }

    /// 列表投影必须来自入口执行，不得推断未知审批人。
    #[test]
    fn list_projection_copies_entry_assignee() {
        let projection = list_projection_from_execution(&execution(), Instant::from_unix_secs(10));
        assert_eq!(projection.current_node_key.as_deref(), Some("n1"));
        assert_eq!(projection.current_assignee_participant_id.as_deref(), Some("u1"));
        assert_eq!(projection.current_assignee_name.as_deref(), Some("张三"));
        assert_eq!(projection.last_status_changed_at, Some(10));
    }

    /// 十一种 PROCESS_REQUIRED 启动类型均固定 receipt -> guard -> BPM 顺序。
    #[test]
    fn all_process_required_start_paths_are_receipt_first_and_guarded() {
        assert_start_write_order(
            include_str!("../sales_order/start_approval.rs"),
            1,
            ".mark_approval_started(",
        );
        assert_start_write_order(
            include_str!("../sales_review/start_approval.rs"),
            1,
            ".mark_approval_started(",
        );
        assert_start_write_order(
            include_str!("../purchase_order/start_approval.rs"),
            1,
            ".mark_loaded_approval_started(",
        );
        assert_start_write_order(
            include_str!("../purchase_order/change_start.rs"),
            1,
            ".mark_approval_started(",
        );
        assert_start_write_order(
            include_str!("../receivable/start_approval.rs"),
            1,
            ".mark_approval_started(",
        );
        assert_start_write_order(
            include_str!("../returns/start_approval.rs"),
            4,
            ".mark_approval_started(",
        );
        assert_start_write_order(include_str!("start_approval.rs"), 1, ".mark_approval_started(");
    }

    /// 通用 Start Replay 必须在开启事务前返回，禁止重复写业务事实。
    #[test]
    fn generic_start_replay_paths_return_before_transaction_writes() {
        for source in [
            include_str!("../sales_order/start_approval.rs"),
            include_str!("../sales_review/start_approval.rs"),
            include_str!("../purchase_order/start_approval.rs"),
            include_str!("../purchase_order/change_start.rs"),
            include_str!("../receivable/start_approval.rs"),
            include_str!("../returns/start_approval.rs"),
        ] {
            let production = source.split("#[cfg(test)]").next().expect("生产代码必须存在");
            assert!(production.contains("let PreparedExecution::Apply(writes) = prepared else"));
        }
    }

    /// 库存启动摘要锁定为无歧义 JSON tuple 的字面 SHA-256。
    #[test]
    fn stock_adjustment_start_digest_has_literal_golden() {
        assert_eq!(
            stock_adjustment_start_digest(&submit_request(), "用户-α").unwrap(),
            "v1:06ca7d6d37aac050a5168f4aa2815e2e40b2c2569530f19ba7e555fd5256683b"
        );
    }

    /// `NULL`、空值、U+001F 与字段边界不得再产生旧拼接格式碰撞。
    #[test]
    fn stock_adjustment_start_digest_is_boundary_safe() {
        let normalized = submit_request();
        let mut equivalent = normalized.clone();
        equivalent.lines[1].line_id = " line-a ".to_string();
        equivalent.lines[1].quantity = "1.000".to_string();
        assert_eq!(
            stock_adjustment_start_digest(&normalized, "actor").unwrap(),
            stock_adjustment_start_digest(&equivalent, "actor").unwrap()
        );

        let mut empty = submit_request();
        empty.note.clear();
        let mut literal_null = empty.clone();
        literal_null.note = "NULL".to_string();
        assert_ne!(
            stock_adjustment_start_digest(&empty, "actor").unwrap(),
            stock_adjustment_start_digest(&literal_null, "actor").unwrap()
        );

        let mut left = submit_request();
        left.note = "a\u{1f}42".to_string();
        left.occurred_at = 9;
        let mut right = submit_request();
        right.note = "a".to_string();
        right.occurred_at = 42;
        assert_ne!(
            stock_adjustment_start_digest(&left, "b").unwrap(),
            stock_adjustment_start_digest(&right, "9\u{1f}b").unwrap()
        );
    }

    /// 每个受签署字段漂移都必须改变 V1 摘要。
    #[test]
    fn stock_adjustment_start_digest_covers_all_signed_fields() {
        let baseline = submit_request();
        let expected = stock_adjustment_start_digest(&baseline, "actor").unwrap();
        let mut variants = Vec::new();

        let mut value = baseline.clone();
        value.expected_version += 1;
        variants.push(value);
        let mut value = baseline.clone();
        value.expected_subject_version += 1;
        variants.push(value);
        let mut value = baseline.clone();
        value.reason_type = AdjustmentReasonType::Damage;
        variants.push(value);
        let mut value = baseline.clone();
        value.lines[0].quantity = "3".to_string();
        variants.push(value);
        let mut value = baseline.clone();
        value.balances[0].expected_version += 1;
        variants.push(value);
        let mut value = baseline.clone();
        value.note = "不同说明".to_string();
        variants.push(value);
        let mut value = baseline.clone();
        value.occurred_at += 1;
        variants.push(value);

        for value in variants {
            assert_ne!(stock_adjustment_start_digest(&value, "actor").unwrap(), expected);
        }
        assert_ne!(
            stock_adjustment_start_digest(&baseline, "other-actor").unwrap(),
            expected
        );
    }

    /// V3、库存 V1 与旧 generic 仅允许按各自原 scope/digest 成对回放。
    #[test]
    fn stock_start_identity_accepts_only_exact_generation_pairs() {
        let req = submit_request();
        let identity = stock_adjustment_start_identity("adj-1", &req, "actor", "def-1", 3).unwrap();
        let current = ApprovalCommandReceipt::new(
            ApprovalCommandReceiptId::new("receipt-1"),
            identity.current(),
            "instance-1",
            Timestamp::from_unix_secs(10).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            identity.classify(Some(&current)),
            ReceiptBranch::SamePayload(_)
        ));

        let scopes = stock_adjustment_start_scopes("adj-1", req.expected_subject_version).unwrap();
        let mut stock_v1 = current.clone();
        stock_v1.scope_id = scopes[1].clone();
        stock_v1.payload_digest = stock_adjustment_start_digest(&req, "actor").unwrap();
        assert!(matches!(
            identity.classify(Some(&stock_v1)),
            ReceiptBranch::SamePayload(_)
        ));

        let mut generic = current.clone();
        generic.scope_id = scopes[1].clone();
        generic.payload_digest = legacy_payload_digest("def-1\u{1f}3\u{1f}2\u{1f}actor");
        assert!(matches!(
            identity.classify(Some(&generic)),
            ReceiptBranch::SamePayload(_)
        ));

        stock_v1.scope_id = scopes[0].clone();
        assert_eq!(identity.classify(Some(&stock_v1)), ReceiptBranch::PayloadConflict);
        generic.payload_digest = stock_adjustment_start_digest(&req, "other-actor").unwrap();
        assert_eq!(identity.classify(Some(&generic)), ReceiptBranch::PayloadConflict);
    }

    /// Unknown-result 查询按作用域只接受对应世代的摘要形状。
    #[test]
    fn submit_result_receipt_digest_shape_is_fail_closed() {
        let digest = "06ca7d6d37aac050a5168f4aa2815e2e40b2c2569530f19ba7e555fd5256683b";
        let scopes = vec!["v3-scope".to_string(), "legacy-scope".to_string()];
        assert!(is_supported_start_receipt_identity(
            "v3-scope",
            &format!("v3:{digest}"),
            &scopes,
        ));
        assert!(is_supported_start_receipt_identity(
            "legacy-scope",
            &format!("v1:{digest}"),
            &scopes,
        ));
        assert!(is_supported_start_receipt_identity(
            "legacy-scope",
            digest,
            &scopes,
        ));
        assert!(!is_supported_start_receipt_identity(
            "v3-scope",
            &format!("v1:{digest}"),
            &scopes,
        ));
        assert!(!is_supported_start_receipt_identity(
            "legacy-scope",
            &format!("v3:{digest}"),
            &scopes,
        ));
        assert!(!is_supported_start_receipt_identity(
            "other-scope",
            digest,
            &scopes,
        ));
        assert!(!is_supported_start_receipt_identity(
            "v3-scope",
            &format!("v3:{}", "G".repeat(64)),
            &scopes,
        ));
    }

    /// 启动计划只允许精确一条 Started 和一条 Entered 通知。
    #[test]
    fn start_notifications_reject_missing_duplicate_or_extra_intents() {
        let valid = vec![
            (
                ApprovalNotificationEventKind::Started,
                "started:instance-1".to_string(),
            ),
            (
                ApprovalNotificationEventKind::Entered,
                "entered:execution-1".to_string(),
            ),
        ];
        assert!(validate_start_notification_identities(&valid, "instance-1", "execution-1").is_ok());
        assert!(validate_start_notification_identities(&valid[..1], "instance-1", "execution-1").is_err());
        assert!(validate_start_notification_identities(
            &[valid[0].clone(), valid[0].clone()],
            "instance-1",
            "execution-1"
        )
        .is_err());
        let mut extra = valid.clone();
        extra.push((
            ApprovalNotificationEventKind::Completed,
            "completed:instance-1".to_string(),
        ));
        assert!(validate_start_notification_identities(&extra, "instance-1", "execution-1").is_err());
    }
}
