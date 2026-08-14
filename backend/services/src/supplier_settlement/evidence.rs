//! W27 结算差异的不可变补证强命令。

use database::{AccessControlExt, NoTransaction, SupplierSettlementExt, Transactional};
use entities::common::time::Instant;
use entities::ids::{SupplierSettlementDifferenceId, SupplierSettlementStatementId};
use entities::supplier_settlement::{
    SettlementStatus, SupplierSettlementDifferenceEvidence, SupplierSettlementDifferenceEvidenceData,
};
use id_generator::next_id;
use validator::Validate;

use super::{
    digest_parts, SettlementDifferenceEvidenceRequest, SettlementDifferenceEvidenceResult,
    SettlementDifferenceEvidenceView, SupplierSettlementService,
};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl SupplierSettlementService {
    /// 为一个精确差异追加不可变证据引用与业务意见。
    ///
    /// 请求 ID 是数据库唯一幂等键；同一请求 ID 的载荷摘要不一致时冲突。补证不会
    /// 直接改变差异结论，正式处理仍需单独差异决定命令。
    pub async fn append_difference_evidence(
        &self,
        difference_id: &str,
        req: SettlementDifferenceEvidenceRequest,
        actor: &AuditActor,
    ) -> Result<SettlementDifferenceEvidenceResult> {
        req.validate()?;
        if difference_id != req.difference_id {
            return Err(Error::ValidationError("差异路径ID与命令载荷不一致".to_string()));
        }
        let command_hash = evidence_command_hash(&req);
        if let Some(existing) = self
            .db
            .supplier_settlement_difference_evidence()
            .find_by_request_id(&req.request_id, &mut NoTransaction)
            .await?
        {
            return replay_evidence(existing, &req, &command_hash);
        }
        let statement = self.load_statement(&req.statement_id).await?;
        if !matches!(
            statement.status,
            SettlementStatus::Draft
                | SettlementStatus::PendingReconciliation
                | SettlementStatus::HasDifference
        ) {
            return Err(Error::BusinessLogicError(
                "当前结算状态禁止追加差异补证".to_string(),
            ));
        }
        let difference = self
            .db
            .supplier_settlement_differences()
            .find_by_id(difference_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商结算差异不存在".to_string()))?;
        if difference.base.version != req.expected_difference_version {
            return Err(Error::ConflictError(
                "结算差异版本已变化，请刷新后重试".to_string(),
            ));
        }
        let item = self
            .db
            .supplier_settlement_items()
            .find_by_id(difference.statement_item_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("结算差异所属明细不存在".to_string()))?;
        if item.statement_id.as_ref() != req.statement_id {
            return Err(Error::BusinessLogicError(
                "差异不属于命令指定的结算单".to_string(),
            ));
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
                provided_by: actor.id().to_string(),
                provided_at: Instant::now(),
                command_hash: command_hash.clone(),
            },
        )?;
        let audit = actor.clone().resource_log(
            "supplier_settlement.difference_evidence.append",
            "supplier_settlement_difference",
            difference_id.to_string(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let evidence_for_tx = evidence.clone();
        let expected_difference_version = req.expected_difference_version;
        let difference_id_for_tx = difference_id.to_string();
        let statement_id_for_tx = req.statement_id.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let current = db
                        .supplier_settlement_differences()
                        .find_by_id(&difference_id_for_tx, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商结算差异不存在".to_string()))?;
                    if current.base.version != expected_difference_version {
                        return Err(Error::ConflictError(
                            "结算差异版本已变化，请刷新后重试".to_string(),
                        ));
                    }
                    let item = db
                        .supplier_settlement_items()
                        .find_by_id(current.statement_item_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("结算差异所属明细不存在".to_string()))?;
                    if item.statement_id.as_ref() != statement_id_for_tx {
                        return Err(Error::BusinessLogicError(
                            "差异不属于命令指定的结算单".to_string(),
                        ));
                    }
                    let mut statement = db
                        .supplier_settlement_statements()
                        .find_by_id(&statement_id_for_tx, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商结算单不存在".to_string()))?;
                    if !matches!(
                        statement.status,
                        SettlementStatus::Draft
                            | SettlementStatus::PendingReconciliation
                            | SettlementStatus::HasDifference
                    ) {
                        return Err(Error::BusinessLogicError(
                            "当前结算状态禁止追加差异补证".to_string(),
                        ));
                    }
                    // 补证属于结算主题变更：与提交复核共同 CAS 同一结算单，禁止
                    // `PENDING_REVIEW` 状态推进和迟到证据在不同文档上并发穿透。
                    db.supplier_settlement_statements()
                        .update(&mut statement, session)
                        .await?;
                    db.supplier_settlement_difference_evidence()
                        .create(&evidence_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await;
        if let Err(error) = transaction_result {
            if let Some(existing) = self
                .db
                .supplier_settlement_difference_evidence()
                .find_by_request_id(&evidence.request_id, &mut NoTransaction)
                .await?
            {
                return replay_evidence(existing, &req, &command_hash);
            }
            return Err(error);
        }
        Ok(evidence_result(
            evidence,
            "RECORDED",
            "差异补证已登记，不会直接改变正式差异结论",
        ))
    }
}

fn evidence_command_hash(req: &SettlementDifferenceEvidenceRequest) -> String {
    let mut references = req.evidence_reference_ids.clone();
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

fn replay_evidence(
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

fn evidence_result(
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
pub(super) fn evidence_view(
    evidence: SupplierSettlementDifferenceEvidence,
) -> SettlementDifferenceEvidenceView {
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
