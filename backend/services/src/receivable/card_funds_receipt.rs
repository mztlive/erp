//! W13 幂等回执的 Service 适配（FIN-E14）。
//!
//! 领域 [`CardFundsCommandReceipt`] / [`CardFundsRegistrationReceipt`] 是唯一
//! encode/decode 与 fingerprint 规则源；本文件只做审计 I/O、错误映射和 HTTP View
//! 装配。legacy WorkItem 补建仍由调用方执行。

use database::{AccessControlExt, Executor};
use entities::common::time::Instant;
use entities::receivable::{
    CardFundsCommandReceipt, CardFundsCommandReceiptError, CardFundsRegistrationKind,
    CardFundsRegistrationReceipt, CardFundsRegistrationReceiptError, EntityCardFundsReviewConclusion,
    EntityCardFundsReviewResult,
};
use mongodb::Database;

use super::dto::{
    CardFundsReviewBusinessResult, CardFundsReviewConclusion, CardFundsReviewFollowUpWorkItem,
    CardFundsReviewResult, CompleteCardFundsReviewResult, CompletedWorkItemStatus,
};
use crate::errors::{Error, Result};

/// 将正式复核回执错误映射为既有 HTTP 错误合同。
///
/// # 参数
/// * `error` - 领域编解码失败
///
/// # 返回
/// 载荷漂移为 `ConflictError`，其余为 `Internal`，文案不变。
///
/// # 错误
/// 本函数即错误转换。
///
/// # 约束
/// 不得落到透明 `Error::Logic`。
pub fn map_command_receipt_error(error: CardFundsCommandReceiptError) -> Error {
    match error {
        CardFundsCommandReceiptError::PayloadConflict => Error::ConflictError(error.to_string()),
        CardFundsCommandReceiptError::Malformed
        | CardFundsCommandReceiptError::ResultIllegal
        | CardFundsCommandReceiptError::ResultCodeIllegal
        | CardFundsCommandReceiptError::ConclusionCodeIllegal
        | CardFundsCommandReceiptError::FollowUpIncomplete
        | CardFundsCommandReceiptError::ReviewNoIllegal
        | CardFundsCommandReceiptError::CompletedAtIllegal
        | CardFundsCommandReceiptError::SerializeFailed(_) => Error::Internal(error.to_string()),
    }
}

/// 将历史登记回执错误映射为既有 HTTP 错误合同。
///
/// # 参数
/// * `error` - 领域编解码失败
///
/// # 返回
/// 前缀/指纹漂移为 `ConflictError`，其余为 `Internal`。
///
/// # 错误
/// 本函数即错误转换。
///
/// # 约束
/// 审计身份非法仍由调用方在 I/O 之后单独映射。
pub fn map_registration_receipt_error(error: CardFundsRegistrationReceiptError) -> Error {
    match error {
        CardFundsRegistrationReceiptError::PayloadConflict => Error::ConflictError(error.to_string()),
        CardFundsRegistrationReceiptError::Malformed
        | CardFundsRegistrationReceiptError::FactIllegal
        | CardFundsRegistrationReceiptError::SerializeFailed(_) => Error::Internal(error.to_string()),
    }
}

/// 将持久化回执装配为固定 W13 HTTP 结果。
///
/// # 参数
/// * `receipt` - 已解码或新写入的领域回执
/// * `work_item_id` - 已完成的原任务
/// * `receivable_account_id` - 应收账户
/// * `operation_id` - 稳定审计主键
///
/// # 返回
/// HTTP 结果；legacy 驳回在补建前不带后继。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 只做 View 映射，不查询审计或 WorkItem。
pub fn complete_review_result(
    receipt: &CardFundsCommandReceipt,
    work_item_id: &str,
    receivable_account_id: &str,
    operation_id: &str,
) -> CompleteCardFundsReviewResult {
    let data = receipt.data();
    let follow_up_work_item =
        receipt
            .follow_up_work_item()
            .map(|(id, work_item_type)| CardFundsReviewFollowUpWorkItem {
                work_item_id: id.to_string(),
                work_item_type: work_item_type.to_string(),
                status: "OPEN".to_string(),
            });
    CompleteCardFundsReviewResult {
        work_item_id: work_item_id.to_string(),
        work_item_status: CompletedWorkItemStatus::Completed,
        business_result: CardFundsReviewBusinessResult {
            receivable_funds_review_id: data.receivable_funds_review_id.clone(),
            receivable_account_id: receivable_account_id.to_string(),
            review_no: data.review_no,
            account_review_status: data.account_review_status.clone(),
            workflow_action_id: data.workflow_action_id.clone(),
            operation_id: operation_id.to_string(),
            completed_at: Instant::from_unix_secs(data.completed_at).as_utc().to_rfc3339(),
            review_result: map_dto_result(data.review_result),
            conclusion: map_dto_conclusion(data.conclusion),
            follow_up_work_item,
        },
    }
}

/// 在事务内读取并严格验证 W13 登记幂等收据。
///
/// # 参数
/// * `db` - 数据库
/// * `audit_id` - 稳定登记审计主键
/// * `expected_action` - 回款或发票登记动作
/// * `expected_fingerprint` - 当前请求指纹
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 无审计时 `None`；命中时返回 `(account_id, fact_id)`。
///
/// # 错误
/// 审计身份非法、缺账户、或领域解码失败。
///
/// # 约束
/// 只读审计；编解码由 [`CardFundsRegistrationReceipt`] 独占。
pub async fn replay_card_funds_registration(
    db: &Database,
    audit_id: &str,
    expected_action: &str,
    expected_fingerprint: &str,
    executor: &mut dyn Executor,
) -> Result<Option<(String, String)>> {
    let Some(audit) = db.audit_logs().find_by_id(audit_id, executor).await? else {
        return Ok(None);
    };
    if audit.action != expected_action || audit.resource_type != "receivable_account" || !audit.success {
        return Err(Error::Internal("卡券票款登记幂等收据身份非法".to_string()));
    }
    let account_id = audit
        .resource_id
        .ok_or_else(|| Error::Internal("卡券票款登记幂等收据缺少应收账户".to_string()))?;
    let receipt = CardFundsRegistrationReceipt::parse(
        audit.message.as_deref().unwrap_or(""),
        expected_fingerprint,
        CardFundsRegistrationKind::from_expected_action(expected_action),
    )
    .map_err(map_registration_receipt_error)?;
    Ok(Some((account_id, receipt.fact_id().to_string())))
}

/// 映射实体复核结果到 HTTP DTO。
///
/// # 参数
/// * `result` - 实体结果
///
/// # 返回
/// 服务 DTO。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 纯枚举映射。
fn map_dto_result(result: EntityCardFundsReviewResult) -> CardFundsReviewResult {
    match result {
        EntityCardFundsReviewResult::Approved => CardFundsReviewResult::Approved,
        EntityCardFundsReviewResult::Rejected => CardFundsReviewResult::Rejected,
    }
}

/// 映射实体复核结论到 HTTP DTO。
///
/// # 参数
/// * `conclusion` - 实体结论
///
/// # 返回
/// 服务 DTO。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 纯枚举映射。
fn map_dto_conclusion(conclusion: EntityCardFundsReviewConclusion) -> CardFundsReviewConclusion {
    match conclusion {
        EntityCardFundsReviewConclusion::NoHistoryFromZero => CardFundsReviewConclusion::NoHistoryFromZero,
        EntityCardFundsReviewConclusion::RecordedFactsReconciled => {
            CardFundsReviewConclusion::RecordedFactsReconciled
        }
        EntityCardFundsReviewConclusion::Rejected => CardFundsReviewConclusion::Rejected,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        complete_review_result, map_command_receipt_error, map_dto_result, map_registration_receipt_error,
    };
    use crate::errors::Error;
    use crate::receivable::dto::{CardFundsReviewConclusion, CardFundsReviewResult};
    use entities::receivable::{
        CardFundsCommandFollowUp, CardFundsCommandReceipt, CardFundsCommandReceiptData,
        CardFundsCommandReceiptError, CardFundsRegistrationReceiptError, EntityCardFundsReviewConclusion,
        EntityCardFundsReviewResult,
    };

    fn rejected_receipt() -> CardFundsCommandReceipt {
        CardFundsCommandReceipt::new(
            "a".repeat(64),
            CardFundsCommandReceiptData {
                receivable_funds_review_id: "review-1".to_string(),
                workflow_action_id: "workflow-1".to_string(),
                review_no: 1,
                account_review_status: "opening_pending".to_string(),
                completed_at: 1_700_000_000,
                review_result: EntityCardFundsReviewResult::Rejected,
                conclusion: EntityCardFundsReviewConclusion::Rejected,
                follow_up: CardFundsCommandFollowUp::Rejected {
                    work_item_id: "wi-2".to_string(),
                    work_item_type: "CARD_FUNDS_REVIEW".to_string(),
                },
            },
        )
        .unwrap()
    }

    #[test]
    fn payload_conflict_maps_to_conflict_error() {
        assert!(matches!(
            map_command_receipt_error(CardFundsCommandReceiptError::PayloadConflict),
            Error::ConflictError(_)
        ));
        assert!(matches!(
            map_registration_receipt_error(CardFundsRegistrationReceiptError::PayloadConflict),
            Error::ConflictError(_)
        ));
        assert!(matches!(
            map_command_receipt_error(CardFundsCommandReceiptError::Malformed),
            Error::Internal(_)
        ));
    }

    #[test]
    fn complete_result_maps_follow_up_and_keeps_open_status() {
        let result = complete_review_result(&rejected_receipt(), "wi-1", "ra-1", "operation-1");
        let follow_up = result.business_result.follow_up_work_item.unwrap();
        assert_eq!(follow_up.work_item_id, "wi-2");
        assert_eq!(follow_up.work_item_type, "CARD_FUNDS_REVIEW");
        assert_eq!(follow_up.status, "OPEN");
        assert_eq!(
            result.business_result.review_result,
            map_dto_result(EntityCardFundsReviewResult::Rejected)
        );
        assert_eq!(CardFundsReviewResult::Rejected.as_str(), "REJECTED");
        assert_eq!(CardFundsReviewConclusion::Rejected.as_str(), "REJECTED");
        assert_eq!(CardFundsReviewResult::Approved.as_str(), "APPROVED");
        assert_eq!(
            CardFundsReviewConclusion::NoHistoryFromZero.as_str(),
            "NO_HISTORY_FROM_ZERO"
        );
        assert_eq!(
            CardFundsReviewConclusion::RecordedFactsReconciled.as_str(),
            "RECORDED_FACTS_RECONCILED"
        );
    }
}
