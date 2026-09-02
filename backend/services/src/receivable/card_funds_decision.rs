//! W13 正式复核决定 DTO 到实体值对象的转换与证据资产校验（FIN-E10）。
//!
//! `ValidatedCardFundsReviewDecision` 与 `CardFundsReviewEvidence` 的纯规则已收敛至
//! `entities::receivable::card_funds_review_decision`；本文件仅负责 DTO 枚举到实体
//! 枚举的映射、批量文件资产的仓储读取结果到 `validate_assets` 的转交，以及 canonical
//! 证据与工作流意见的唯一生成入口。Service 仍保留批量读取、当前时间注入、授权与审批提交。

use entities::common::time::Instant;
use entities::file_asset::FileAsset;
use entities::receivable::{
    EntityCardFundsReviewConclusion, EntityCardFundsReviewResult, EntityCardFundsReviewType,
    ValidatedCardFundsReviewDecision,
};

use super::dto::{
    CardFundsReviewConclusion, CardFundsReviewDecision, CardFundsReviewResult, CardFundsReviewType,
};
use crate::errors::{Error, Result};

/// 将服务 DTO 的复核类型映射为实体类型。
///
/// # 参数
/// * `value` - 服务 DTO 枚举
///
/// # 返回
/// 实体侧枚举。
fn map_review_type(value: CardFundsReviewType) -> EntityCardFundsReviewType {
    match value {
        CardFundsReviewType::Opening => EntityCardFundsReviewType::Opening,
        CardFundsReviewType::SyncDelta => EntityCardFundsReviewType::SyncDelta,
    }
}

/// 将服务 DTO 的复核结果映射为实体结果。
///
/// # 参数
/// * `value` - 服务 DTO 枚举
///
/// # 返回
/// 实体侧枚举。
fn map_review_result(value: CardFundsReviewResult) -> EntityCardFundsReviewResult {
    match value {
        CardFundsReviewResult::Approved => EntityCardFundsReviewResult::Approved,
        CardFundsReviewResult::Rejected => EntityCardFundsReviewResult::Rejected,
    }
}

/// 将服务 DTO 的复核结论映射为实体结论。
///
/// # 参数
/// * `value` - 服务 DTO 枚举
///
/// # 返回
/// 实体侧枚举。
fn map_conclusion(value: CardFundsReviewConclusion) -> EntityCardFundsReviewConclusion {
    match value {
        CardFundsReviewConclusion::NoHistoryFromZero => EntityCardFundsReviewConclusion::NoHistoryFromZero,
        CardFundsReviewConclusion::RecordedFactsReconciled => {
            EntityCardFundsReviewConclusion::RecordedFactsReconciled
        }
        CardFundsReviewConclusion::Rejected => EntityCardFundsReviewConclusion::Rejected,
    }
}

/// 将 `CardFundsReviewDecision` DTO 转换为已校验的实体决定。
///
/// 统一完成账户 ID、结论／理由组合、证据去重、长度边界、canonical 排序与工作流意见
/// 的纯规则校验；任一失败整体不产生部分结果。
///
/// # 参数
/// * `decision` - HTTP DTO 决定的原始输入
///
/// # 返回
/// 校验通过的 `ValidatedCardFundsReviewDecision`，其 `canonical_evidence` 与
/// `workflow_comment` 为字节级稳定的唯一文本。
///
/// # 错误
/// 透传实体校验错误（首错顺序与文案与原 Service 一致）；实体侧 `LogicError` 已按首错文案
/// 映射为 `ValidationError`，保持既有 HTTP 422 语义。
///
/// # 约束
/// 纯内存转换；不执行 I/O、时钟或加密；错误文案与原 `validate_card_funds_decision` 保持一致。
pub fn validated_from_dto(decision: &CardFundsReviewDecision) -> Result<ValidatedCardFundsReviewDecision> {
    ValidatedCardFundsReviewDecision::try_new(
        decision.receivable_account_id.as_ref(),
        decision.expected_review_chain_tail_id.as_deref(),
        map_review_type(decision.review_type),
        map_review_result(decision.review_result),
        map_conclusion(decision.conclusion),
        &decision.evidence_document_ids,
        &decision.evidence_references,
        decision.reason_code.as_deref(),
        decision.comment.as_deref(),
    )
    .map_err(|err| Error::ValidationError(err.to_string()))
}

/// 校验批量读取的证据资产在指定时点可用。
///
/// 批量仓储结果不承诺顺序；本方法按 `ValidatedCardFundsReviewDecision` 的证据顺序
/// 逐项解释首个缺失与首个不可用，保持既有首错语义。可用性唯一规则为
/// `FileAsset::is_usable_at`，由实体层提供。
///
/// # 参数
/// * `validated` - 已校验决定（含证据顺序）
/// * `assets` - 仓储一次批量读取的资产事实（无序）
/// * `now` - 本次校验的统一时点
///
/// # 返回
/// 全部文件均存在且可用时返回 `Ok(())`。
///
/// # 错误
/// 首个缺失返回 `NotFound("复核证据文件不存在: <id>")`；首个扫描未通过／已销毁／已过期返回
/// `BusinessLogicError("复核证据文件未通过安全检查、已销毁或已过期")`。
///
/// # 约束
/// 唯一可用性规则在实体层 `FileAsset::is_usable_at`；错误文案与原 Service 一致。
pub fn validate_evidence_assets(
    validated: &ValidatedCardFundsReviewDecision,
    assets: &[FileAsset],
    now: Instant,
) -> Result<()> {
    use std::collections::HashMap;
    let assets_by_id: HashMap<&str, &FileAsset> = assets
        .iter()
        .map(|asset| (asset.base.id.as_str(), asset))
        .collect();
    for id in validated.evidence().document_ids() {
        let asset = assets_by_id
            .get(id.as_ref())
            .ok_or_else(|| Error::NotFound(format!("复核证据文件不存在: {}", id.as_ref())))?;
        if !asset.is_usable_at(now) {
            return Err(Error::BusinessLogicError(
                "复核证据文件未通过安全检查、已销毁或已过期".to_string(),
            ));
        }
    }
    Ok(())
}

/// 返回已校验决定的 canonical 证据文本（唯一生成入口）。
///
/// # 参数
/// * `validated` - 已校验决定
///
/// # 返回
/// `None` 或排序后 `"; "` 连接的文本；相同输入必得相同输出。
pub fn canonical_evidence(validated: &ValidatedCardFundsReviewDecision) -> Option<String> {
    validated.canonical_evidence().map(|s| s.to_string())
}

/// 返回已校验决定的工作流意见（唯一生成入口）。
///
/// # 参数
/// * `validated` - 已校验决定
///
/// # 返回
/// `Some(comment)`，与原 `workflow_decision_comment` 语义一致且字节级稳定。
pub fn workflow_comment(validated: &ValidatedCardFundsReviewDecision) -> Option<String> {
    validated.workflow_comment().map(|s| s.to_string())
}

/// 供单测使用的受控理由白名单透出（与实体保持一致）。
#[cfg(test)]
pub(crate) fn allowed_reasons() -> &'static [&'static str] {
    &[
        "EVIDENCE_INSUFFICIENT",
        "FACTS_MISMATCH",
        "COUNTERPARTY_UNCLEAR",
        "OTHER",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use entities::ids::FileAssetId;

    fn decision_with_reason(reason: Option<&str>) -> CardFundsReviewDecision {
        CardFundsReviewDecision {
            receivable_account_id: entities::ids::ReceivableAccountId::new("ra-1"),
            expected_account_seq: 1,
            expected_account_domain_version: "1".to_string(),
            expected_review_chain_tail_id: None,
            expected_review_chain_version: "rcv:empty".to_string(),
            expected_next_review_no: 1,
            expected_sales_order_revision_id: "sor-1".to_string(),
            expected_funds_fact_version: "ffv:empty".to_string(),
            review_type: CardFundsReviewType::Opening,
            review_result: CardFundsReviewResult::Rejected,
            conclusion: CardFundsReviewConclusion::Rejected,
            evidence_document_ids: vec![FileAssetId::new("file-1")],
            evidence_references: vec![],
            comment: None,
            reason_code: reason.map(|s| s.to_string()),
        }
    }

    /// DTO 到实体转换保持受控理由白名单一致。
    #[test]
    fn dto_conversion_keeps_controlled_reasons() {
        for reason in allowed_reasons() {
            assert!(validated_from_dto(&decision_with_reason(Some(reason))).is_ok());
        }
        assert!(validated_from_dto(&decision_with_reason(Some("UNKNOWN"))).is_err());
    }

    /// canonical 与 workflow 文本通过实体唯一生成，相同输入字节稳定。
    #[test]
    fn canonical_and_workflow_are_byte_stable() {
        let dto = CardFundsReviewDecision {
            receivable_account_id: entities::ids::ReceivableAccountId::new("ra-1"),
            expected_account_seq: 1,
            expected_account_domain_version: "1".to_string(),
            expected_review_chain_tail_id: None,
            expected_review_chain_version: "rcv:empty".to_string(),
            expected_next_review_no: 1,
            expected_sales_order_revision_id: "sor-1".to_string(),
            expected_funds_fact_version: "ffv:empty".to_string(),
            review_type: CardFundsReviewType::Opening,
            review_result: CardFundsReviewResult::Approved,
            conclusion: CardFundsReviewConclusion::NoHistoryFromZero,
            evidence_document_ids: vec![FileAssetId::new("file-2"), FileAssetId::new("file-1")],
            evidence_references: vec!["z-ref".to_string(), "a-ref".to_string()],
            comment: Some("已核对".to_string()),
            reason_code: None,
        };
        let v1 = validated_from_dto(&dto).unwrap();
        let v2 = validated_from_dto(&dto).unwrap();
        assert_eq!(canonical_evidence(&v1), canonical_evidence(&v2));
        assert_eq!(workflow_comment(&v1), workflow_comment(&v2));
        // canonical 为排序后结果
        let canonical = canonical_evidence(&v1).unwrap();
        let mut parts = [
            "a-ref".to_string(),
            "z-ref".to_string(),
            "file_asset:file-1".to_string(),
        ];
        parts.sort();
        assert_eq!(canonical, parts.join("; "));
    }
}
