//! 域 D33 `supplier_settlement` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；金额使用 `entities::money`
//! 定点类型（serde_json 下自动字符串化）；业务日期使用 `BusinessDate`（`YYYY-MM-DD`）。

mod difference;
mod draft;
mod query;
mod review;
mod source;

pub use self::difference::{
    SettlementDifferenceDecisionRequest, SettlementDifferenceDecisionResult,
    SettlementDifferenceDecisionStatus, SettlementDifferenceEvidenceRequest,
    SettlementDifferenceEvidenceResult, SettlementDifferenceResolution,
};
pub use self::draft::{
    CreateSettlementStatementRequest, RefreshSettlementStatementRequest, SettlementDraftAction,
    SettlementDraftCommandResult, VoidSettlementRequest,
};
pub(crate) use self::query::StatementListQuery;
pub use self::query::{
    SettlementDifferenceEvidenceView, SettlementPageView, SettlementReviewActionBlockerView,
    SettlementReviewProcessingState, SettlementStatementListStatsView, SettlementStatementStatsView,
    SupplierSettlementDifferenceListParams, SupplierSettlementDifferenceView,
    SupplierSettlementItemListParams, SupplierSettlementItemView, SupplierSettlementStatementListParams,
    SupplierSettlementStatementListView, SupplierSettlementStatementView,
};
pub use self::review::{
    SettlementObjectAction, SettlementReviewAction, SettlementReviewCommand, SettlementReviewDecisionStatus,
    SettlementReviewSubmissionStatus, SubmitSettlementReviewRequest, SubmitSettlementReviewResult,
};
pub use self::source::{
    RecordSettlementSourceEvidenceLineRequest, RecordSettlementSourceEvidenceRequest,
    SupplierSettlementSourceEvidenceQuery, SupplierSettlementSourceEvidenceView,
};

#[cfg(test)]
use crate::dto::supplier_fulfillment::normalize_sort;

/// 校验需写入幂等收据的操作 ID 不含协议分隔符。
fn safe_command_id(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Ok(());
    }
    Err(validator::ValidationError::new("操作ID包含非法字符"))
}

#[cfg(test)]
mod tests {
    use super::review::SettlementReviewDecisionData;
    use super::{
        normalize_sort, SettlementDifferenceDecisionRequest, SettlementDifferenceResolution,
        SettlementReviewAction, SettlementReviewCommand, SubmitSettlementReviewRequest,
        SupplierSettlementDifferenceListParams, SupplierSettlementItemListParams,
        SupplierSettlementStatementListParams,
    };
    use crate::dto::supplier_fulfillment::SortDir;
    use crate::entity::supplier_settlement::SettlementStatus;
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" period_start ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "period_start"],
        )
        .unwrap();
        assert_eq!(field, "period_start");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn statement_list_params_normalize_paging_filters_and_sort_defaults() {
        let params = SupplierSettlementStatementListParams {
            q: None,
            statement_no: Some(" ST-2026 ".to_string()),
            supplier_id: None,
            status: Some(SettlementStatus::PendingReview),
            period_from: None,
            period_to: None,
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.statement_no.as_deref(), Some("ST-2026"));
        assert_eq!(query.status, Some(SettlementStatus::PendingReview));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params = SupplierSettlementItemListParams {
            statement_id: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());

        let params = SupplierSettlementDifferenceListParams {
            statement_item_id: None,
            status: None,
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("difference_amount".to_string()),
            sort_dir: Some("asc".to_string()),
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.paging.sort_by, "difference_amount");
        assert_eq!(query.paging.sort_dir, SortDir::Asc);
    }

    #[test]
    fn strong_commands_match_frozen_wire_and_reject_receipt_delimiters() {
        let submit: SubmitSettlementReviewRequest = serde_json::from_value(serde_json::json!({
            "action": "SUBMIT_REVIEW",
            "statement_id": "statement-1",
            "expected_lock_version": 1,
            "subject_hash": "a".repeat(64),
            "refresh_cutoff_policy_id": "supplier-settlement-review-cutoff",
            "expected_refresh_cutoff_policy_version": "1",
            "reviewer_user_id": "reviewer-1",
            "operation_id": "submit:1",
            "idempotency_key": "submit-key-1",
            "comment": "提交复核"
        }))
        .unwrap();
        assert!(submit.validate().is_ok());

        let review: SettlementReviewCommand = serde_json::from_value(serde_json::json!({
            "work_item_id": "work-item-1",
            "expected_task_version": "2",
            "expected_subject_version": "a".repeat(64),
            "decision": {
                "statement_id": "statement-1",
                "expected_lock_version": 2,
                "action": "REJECT",
                "operation_id": "review-1",
                "reason_code": "NEEDS_MORE_EVIDENCE",
                "comment": "证据不足"
            },
            "idempotency_key": "review-key-1"
        }))
        .unwrap();
        assert!(review.validate().is_ok());

        let mut invalid = review;
        invalid.decision.operation_id = "review|1".to_string();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn difference_decision_uses_fixed_resolution_codes() {
        let request = SettlementDifferenceDecisionRequest {
            statement_id: "statement-1".to_string(),
            difference_id: "difference-1".to_string(),
            expected_lock_version: 1,
            expected_difference_version: 1,
            resolution: SettlementDifferenceResolution::ClosedNoAdjustment,
            reason_code: "NO_BUSINESS_IMPACT".to_string(),
            evidence_reference_ids: vec!["evidence-1".to_string()],
            operation_id: "difference-1".to_string(),
            idempotency_key: "difference-key-1".to_string(),
        };
        assert!(request.validate().is_ok());
        assert_eq!(
            serde_json::to_value(request).unwrap()["resolution"],
            "CLOSED_NO_ADJUSTMENT"
        );

        let command = SettlementReviewCommand {
            work_item_id: "work-item-1".to_string(),
            expected_task_version: "1".to_string(),
            expected_subject_version: "b".repeat(64),
            decision: SettlementReviewDecisionData {
                statement_id: "statement-1".to_string(),
                expected_lock_version: 1,
                action: SettlementReviewAction::Confirm,
                operation_id: "confirm-1".to_string(),
                reason_code: None,
                comment: None,
            },
            idempotency_key: "confirm-key-1".to_string(),
        };
        assert_eq!(
            serde_json::to_value(command).unwrap()["decision"]["action"],
            "CONFIRM"
        );
    }

    #[test]
    fn review_reject_reason_parses_and_enforces_command_protocol() {
        use crate::entity::supplier_settlement::SettlementReviewRejectReason;

        let decision = SettlementReviewDecisionData {
            statement_id: "statement-1".to_string(),
            expected_lock_version: 1,
            action: SettlementReviewAction::Reject,
            operation_id: "review-1".to_string(),
            reason_code: Some("  amount_mismatch ".to_string()),
            comment: None,
        };
        assert_eq!(
            decision.parsed_reject_reason().unwrap(),
            Some(SettlementReviewRejectReason::AmountMismatch)
        );

        for code in ["NEEDS_MORE_EVIDENCE", "AMOUNT_MISMATCH", "OTHER"] {
            let decision = SettlementReviewDecisionData {
                statement_id: "statement-1".to_string(),
                expected_lock_version: 1,
                action: SettlementReviewAction::Reject,
                operation_id: "review-1".to_string(),
                reason_code: Some(code.to_string()),
                comment: None,
            };
            let reason = decision.parsed_reject_reason().unwrap().unwrap();
            assert_eq!(reason.as_str(), code);
        }

        for bad in [
            None,
            Some("   ".to_string()),
            Some("A".repeat(65)),
            Some("AMOUNT_UNRESOLVED".to_string()),
            Some("NEEDS MORE".to_string()),
        ] {
            let decision = SettlementReviewDecisionData {
                statement_id: "statement-1".to_string(),
                expected_lock_version: 1,
                action: SettlementReviewAction::Reject,
                operation_id: "review-1".to_string(),
                reason_code: bad,
                comment: None,
            };
            assert!(decision.parsed_reject_reason().is_err());
        }

        let confirm_with_reason = SettlementReviewDecisionData {
            statement_id: "statement-1".to_string(),
            expected_lock_version: 1,
            action: SettlementReviewAction::Confirm,
            operation_id: "confirm-1".to_string(),
            reason_code: Some("OTHER".to_string()),
            comment: None,
        };
        assert!(confirm_with_reason.parsed_reject_reason().is_err());

        let confirm = SettlementReviewDecisionData {
            statement_id: "statement-1".to_string(),
            expected_lock_version: 1,
            action: SettlementReviewAction::Confirm,
            operation_id: "confirm-1".to_string(),
            reason_code: None,
            comment: None,
        };
        assert_eq!(confirm.parsed_reject_reason().unwrap(), None);
    }
}
