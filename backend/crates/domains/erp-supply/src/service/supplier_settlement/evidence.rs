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
    /// 在调用方执行器内核对结算单允许补证、差异版本仍匹配且所属明细归该结算单后，构造带提供人、请求号和
    /// 载荷指纹的不可变证据；这里不落库。缺对象返回 `NotFound`，版本过期冲突，状态或归属不符为业务错误。
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
    /// 在原事务内重读差异、明细及结算单并核对版本、归属和可补证状态，再 CAS 推进复核主题并追加不可变补证。
    /// 缺记录为 `NotFound`，版本变化为冲突，归属或状态不符为业务错误；写入失败向上传播。
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
    /// 在原执行器按请求 ID 查询已存差异补证。未登记返回 `None`；结算单、差异及指纹均吻合才恢复原结果，同
    /// ID 异载荷或异归属返回冲突。
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

// 校验结算差异乐观锁版本（补证与正式结论共用同一冲突口径）。
// 版本一致时无返回值。
// 版本不一致时返回冲突错误。
pub(super) fn ensure_difference_version(actual: u64, expected: u64) -> Result<()> {
    if actual != expected {
        return Err(Error::ConflictError("结算差异版本已变化，请刷新后重试".to_string()));
    }
    Ok(())
}

/// 计算证据引用去重、排序后的补证命令指纹。
/// 返回与证据引用输入顺序无关的稳定摘要。
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
/// 一致时返回状态为 `REPLAYED` 的补证结果。
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
/// 返回含详情投影的补证命令结果。
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
/// 返回证据引用、意见、说明、提供人和提供时间。
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
