use bpm::model::{ApprovalNodeExecution, CommandPayloadField};
use erp_core::common::time::Instant;
use erp_inventory::{
    StockAdjustmentLineUpdate, StockAdjustmentLineUpdateInput, SubmitStockAdjustmentRequest,
};
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::repository::bpm::ApprovalInstanceListProjection;
use erp_workflow::service::approval::execution::idempotency::{
    PreparedCommandIdentity, legacy_payload_digest, legacy_standard_start_receipt_identity,
    legacy_start_receipt_identity, normalize_idempotency_key, specialized_start_identity,
    start_scope_candidates,
};
use erp_workflow::service::approval::process_kind::process_kind_of;

use crate::{Error, Result};

const STOCK_ADJUSTMENT_START_DIGEST_VERSION: &str = "STOCK_ADJUSTMENT_START_V1";
const STOCK_ADJUSTMENT_START_VARIANT: &str = "STOCK_ADJUSTMENT_SUBMISSION";

/// 返回库存调整启动命令的当前 V3 与历史无前缀作用域。
pub(super) fn stock_adjustment_start_scopes(
    adjustment_id: &str,
    target_subject_version: u32,
) -> Result<Vec<String>> {
    let kind = process_kind_of(DocumentType::StockAdjustment);
    start_scope_candidates(
        kind.as_str(),
        DocumentType::StockAdjustment.as_str(),
        adjustment_id,
        target_subject_version,
    )
    .map_err(Error::from)
}

/// 对完整规范化库存调整提交载荷计算版本化摘要。
pub(super) fn stock_adjustment_start_digest(
    req: &SubmitStockAdjustmentRequest,
    actor_id: &str,
) -> Result<String> {
    let mut lines = build_adjustment_line_updates(&req.lines)?
        .into_iter()
        .map(|line| {
            (
                line.line_id,
                line.quantity.to_decimal().normalize().to_string(),
                line.direction.map(|value| value.as_str()).unwrap_or("").to_string(),
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
pub(super) fn stock_adjustment_start_identity(
    adjustment_id: &str,
    req: &SubmitStockAdjustmentRequest,
    actor_id: &str,
    binding_id: &str,
    definition_version: u32,
) -> Result<PreparedCommandIdentity> {
    let key = normalize_idempotency_key(&req.idempotency_key)?;
    let process_kind = process_kind_of(DocumentType::StockAdjustment);
    let mut lines = build_adjustment_line_updates(&req.lines)?
        .into_iter()
        .map(|line| {
            (
                line.line_id,
                line.quantity.to_decimal().normalize().to_string(),
                line.direction.map(|value| value.as_str()).unwrap_or("").to_string(),
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

/// Unknown-result 查询仅接受作用域与摘要版本的精确历史配对。
pub(super) fn is_supported_start_receipt_identity(
    scope_id: &str,
    payload_digest: &str,
    scopes: &[String],
) -> bool {
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
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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
pub(super) fn list_projection_from_execution(
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

/// 把服务输入转换为已解析的调整明细更新值对象。
///
/// # 参数
/// * `updates` - 客户端提交的明细更新
///
/// # 返回
/// 返回完成主键规范化与数量解析的值对象集合。
///
/// # 错误
/// 行主键或数量非法时返回 `ValidationError`。
pub fn build_adjustment_line_updates(
    updates: &[StockAdjustmentLineUpdateInput],
) -> Result<Vec<StockAdjustmentLineUpdate>> {
    erp_inventory::build_adjustment_line_updates(updates).map_err(Error::from)
}
