use erp_core::common::time::Instant;
use erp_core::ids::{SupplierSettlementDifferenceId, SupplierSettlementStatementId};
use id_generator::next_id;
use persistence_core::Executor;

use super::SupplierSettlementService;
use super::shared::*;
use crate::dto::supplier_settlement::*;
use crate::entity::supplier_settlement::*;
use crate::repository::SupplierSettlementExt;
use crate::repository::prelude::*;
use crate::{Error, Result};
impl SupplierSettlementService {
    /// 结算本域prepare_difference_evidence，保持原校验、构造和执行器顺序。
    ///
    /// # 参数
    /// * `difference_id` - 结算差异主键。
    /// * `req` - 补证请求。
    /// * `actor_id` - 补证提供人。
    /// * `command_hash` - 已计算的命令指纹。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回尚未写入的补证实体。
    ///
    /// # 错误
    /// 结算单、差异或明细不存在时返回 `NotFound`。差异版本不一致时返回 `ConflictError`。结算状态不允许补证，或差异不属于命令指定的结算单时返回 `BusinessLogicError`。实体构造或仓储读取失败时返回对应错误。
    pub async fn prepare_difference_evidence(
        &self,
        difference_id: &str,
        req: &SettlementDifferenceEvidenceRequest,
        actor_id: &str,
        command_hash: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierSettlementDifferenceEvidence> {
        let statement = self.load_statement(&req.statement_id, executor).await?;
        if !matches!(
            statement.status,
            SettlementStatus::Draft
                | SettlementStatus::PendingReconciliation
                | SettlementStatus::HasDifference
        ) {
            return Err(Error::BusinessLogicError("当前结算状态禁止追加差异补证".to_string()));
        }
        let difference = self
            .db
            .supplier_settlement_differences()
            .find_by_id(difference_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商结算差异不存在".to_string()))?;
        ensure_difference_version(difference.base.version, req.expected_difference_version)?;
        let item = self
            .db
            .supplier_settlement_items()
            .find_by_id(difference.statement_item_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("结算差异所属明细不存在".to_string()))?;
        if item.statement_id.as_ref() != req.statement_id {
            return Err(Error::BusinessLogicError("差异不属于命令指定的结算单".to_string()));
        }
        let evidence = SupplierSettlementDifferenceEvidence::new(
            next_id(),
            SupplierSettlementDifferenceEvidenceData {
                request_id: req.request_id.clone(),
                statement_id: SupplierSettlementStatementId::new(req.statement_id.clone()),
                difference_id: SupplierSettlementDifferenceId::new(req.difference_id.clone()),
                evidence_reference_ids: req.evidence_reference_ids.clone(),
                opinion_code: req.opinion_code.clone(),
                comment: req.comment.clone(),
                provided_by: actor_id.to_string(),
                provided_at: Instant::now(),
                command_hash: command_hash.to_string(),
            },
        )?;

        Ok(evidence)
    }
    /// 结算本域persist_difference_evidence，保持原校验、构造和执行器顺序。
    ///
    /// # 参数
    /// * `difference_id` - 结算差异主键。
    /// * `statement_id` - 命令指定的结算单。
    /// * `expected_version` - 期望的差异版本。
    /// * `evidence` - 已构造的补证。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 无返回值。结算主题已按 CAS 推进，补证已追加。
    ///
    /// # 错误
    /// 差异、明细或结算单不存在时返回 `NotFound`。差异版本变化时返回 `ConflictError`。差异不属于结算单或当前状态禁止补证时返回 `BusinessLogicError`。仓储写入失败时返回对应错误。
    pub async fn persist_difference_evidence(
        &self,
        difference_id: &str,
        statement_id: &str,
        expected_version: u64,
        evidence: &SupplierSettlementDifferenceEvidence,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        super::evidence_posting::persist(
            &self.db,
            difference_id,
            statement_id,
            expected_version,
            evidence,
            executor,
        )
        .await
    }
    /// 按请求 ID 查找原补证并保留完整关联和载荷复验。
    ///
    /// # 参数
    /// * `request_id` - 补证请求身份。
    /// * `req` - 本次补证请求，用于核对结算单和差异。
    /// * `hash` - 本次命令指纹。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 没有该请求时返回 `None`。指纹和关联一致时返回重放结果。
    ///
    /// # 错误
    /// 同一请求已用于不同命令时返回 `ConflictError`。仓储读取失败时返回对应错误。
    pub async fn replay_difference_evidence(
        &self,
        request_id: &str,
        req: &SettlementDifferenceEvidenceRequest,
        hash: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SettlementDifferenceEvidenceResult>> {
        let Some(existing) = self
            .db
            .supplier_settlement_difference_evidence()
            .find_by_request_id(request_id, executor)
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(replay_evidence(existing, req, hash)?))
    }
}

/// 校验结算差异乐观锁版本（补证与正式结论共用同一冲突口径）。
///
/// # 参数
/// * `actual` - 当前差异版本
/// * `expected` - 命令声明的期望版本
///
/// # 返回
/// 版本一致时无返回值。
///
/// # 错误
/// 版本不一致时返回冲突错误。
pub(super) fn ensure_difference_version(actual: u64, expected: u64) -> Result<()> {
    if actual != expected {
        return Err(Error::ConflictError("结算差异版本已变化，请刷新后重试".to_string()));
    }
    Ok(())
}

/// 计算证据引用去重、排序后的补证命令指纹。
///
/// # 参数
/// * `req` - 原始结算差异补证请求
///
/// # 返回
/// 返回与证据引用输入顺序无关的稳定摘要。
///
/// # 错误
/// 无。
pub fn evidence_command_hash(req: &SettlementDifferenceEvidenceRequest) -> String {
    let mut references = req.evidence_reference_ids.iter().map(String::as_str).collect::<Vec<_>>();
    references.sort();
    references.dedup();
    digest_parts(&[
        "supplier-settlement-difference-evidence-v1".to_string(),
        req.request_id.clone(),
        req.idempotency_key.clone(),
        req.statement_id.clone(),
        req.difference_id.clone(),
        req.expected_difference_version.to_string(),
        references.join(","),
        req.opinion_code.clone().unwrap_or_default(),
        req.comment.clone().unwrap_or_default(),
    ])
}

/// 复验已存补证的指纹、结算单和差异是否与本次命令相同。
///
/// # 参数
/// * `existing` - 已持久化的补证。
/// * `req` - 本次补证请求。
/// * `command_hash` - 本次命令指纹。
///
/// # 返回
/// 一致时返回状态为 `REPLAYED` 的补证结果。
///
/// # 错误
/// 指纹、结算单或差异不同时返回 `ConflictError`。
pub fn replay_evidence(
    existing: SupplierSettlementDifferenceEvidence,
    req: &SettlementDifferenceEvidenceRequest,
    command_hash: &str,
) -> Result<SettlementDifferenceEvidenceResult> {
    if existing.command_hash != command_hash
        || existing.statement_id.as_ref() != req.statement_id
        || existing.difference_id.as_ref() != req.difference_id
    {
        return Err(Error::ConflictError("补证请求ID已用于不同命令".to_string()));
    }
    Ok(evidence_result(existing, "REPLAYED", "差异补证结果已恢复"))
}

/// 由补证实体组装命令结果。
///
/// # 参数
/// * `evidence` - 补证实体。
/// * `result_status` - 结果状态文本。
/// * `message` - 结果说明。
///
/// # 返回
/// 返回含详情投影的补证命令结果。
///
/// # 错误
/// 不返回错误。
pub fn evidence_result(
    evidence: SupplierSettlementDifferenceEvidence,
    result_status: &str,
    message: &str,
) -> SettlementDifferenceEvidenceResult {
    SettlementDifferenceEvidenceResult {
        result_status: result_status.to_string(),
        message: message.to_string(),
        request_id: evidence.request_id.clone(),
        statement_id: evidence.statement_id.to_string(),
        difference_id: evidence.difference_id.to_string(),
        evidence: evidence_view(evidence),
    }
}

/// 将不可变补证实体转换为详情投影。
///
/// # 参数
/// * `evidence` - 补证实体。
///
/// # 返回
/// 返回证据引用、意见、说明、提供人和提供时间。
///
/// # 错误
/// 不返回错误。
pub fn evidence_view(evidence: SupplierSettlementDifferenceEvidence) -> SettlementDifferenceEvidenceView {
    SettlementDifferenceEvidenceView {
        evidence_id: evidence.base.id,
        evidence_reference_ids: evidence.evidence_reference_ids,
        opinion_code: evidence.opinion_code,
        comment: evidence.comment,
        provided_by: evidence.provided_by,
        provided_at: evidence.provided_at.unix_secs(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn difference_version_guard_rejects_stale_command() {
        assert!(super::ensure_difference_version(1, 1).is_ok());
        assert!(super::ensure_difference_version(2, 1).is_err());
    }

    #[test]
    fn command_hash_is_order_insensitive_for_reference_set() {
        let mut request = SettlementDifferenceEvidenceRequest {
            statement_id: "statement-1".to_string(),
            difference_id: "difference-1".to_string(),
            expected_difference_version: 1,
            evidence_reference_ids: vec!["ticket://2".to_string(), "ticket://1".to_string()],
            opinion_code: Some("PROCUREMENT_CONFIRMED".to_string()),
            comment: None,
            request_id: "request-1".to_string(),
            idempotency_key: "key-1".to_string(),
        };
        let first = evidence_command_hash(&request);
        request.evidence_reference_ids.reverse();
        assert_eq!(first, evidence_command_hash(&request));
    }
}
