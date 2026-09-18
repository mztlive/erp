//! 域 D22 `legacy_import` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；业务日期 `baseline_date`
//! 为 `YYYY-MM-DD` 字符串；本域无金额字段。

use application_core::{page_or_default, page_size_or_default};
use serde::Serialize;

use crate::error::Result;

/// Background-job status snapshot; wire code matches support `JobStatus`.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportJobStatus {
    /// 等待执行。
    Pending,
    /// 执行中。
    Running,
    /// 部分成功。
    PartiallySucceeded,
    /// 成功。
    Succeeded,
    /// 失败。
    Failed,
    /// 已取消。
    Cancelled,
}

/// 导入批次列表允许的排序字段白名单（与仓储投影白名单一致，Service 层先校验）。
pub const LEGACY_IMPORT_BATCH_SORT_FIELDS: &[&str] = &["created_at", "batch_no", "baseline_date"];
/// 导入行列表允许的排序字段白名单。
pub const LEGACY_IMPORT_ROW_SORT_FIELDS: &[&str] = &["created_at", "source_row_key"];
/// 导入确认列表允许的排序字段白名单。
pub const LEGACY_IMPORT_CONFIRMATION_SORT_FIELDS: &[&str] = &["created_at", "trial_version"];

/// 排序方向。
pub use application_core::SortDir;

/// 归一化后的分页查询参数（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

impl PageParams {
    /// 返回仓储筛选用的排序字段名（白名单已校验）。
    ///
    /// # 返回
    /// 返回归一化后的排序字段。
    pub fn sort_field(&self) -> String {
        self.sort_by.to_owned()
    }

    /// 判断排序是否为升序。
    ///
    /// # 返回
    /// 升序返回 `true`，降序返回 `false`。
    pub fn sort_ascending(&self) -> bool {
        matches!(self.sort_dir, SortDir::Asc)
    }
}

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use application_core::PageView;
/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。
///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `allowed_fields` - 白名单
///
/// # 返回
/// 返回 `(排序字段, 方向)`；未提供时默认 `("created_at", Desc)`。
///
/// # 错误
/// 字段不在白名单或方向不是 `asc`/`desc` 时返回 `ValidationError`。
pub use application_core::normalize_sort;

/// 归一化分页与排序参数（白名单校验 + 默认值），三类列表查询共用。
///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `page` - 可选页码；缺省取首页
/// * `page_size` - 可选单页条数；缺省取默认值
/// * `allowed_fields` - 排序字段白名单
///
/// # 返回
/// 返回归一化后的分页查询参数。
///
/// # 错误
/// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
pub(crate) fn normalize_paging(
    sort_by: &Option<String>,
    sort_dir: &Option<String>,
    page: Option<u64>,
    page_size: Option<u32>,
    allowed_fields: &'static [&'static str],
) -> Result<PageParams> {
    let (sort_by, sort_dir) = normalize_sort(sort_by, sort_dir, allowed_fields)?;
    Ok(PageParams {
        page: page_or_default(page),
        page_size: page_size_or_default(page_size),
        sort_by,
        sort_dir,
    })
}

mod apply;
mod batch;
mod confirmation;
mod execution;
mod row;

pub use apply::{
    ApplyLegacyImportBatchRequest, ApplyRowOutcome, ApplyRowResult, CUSTOMER_NOT_FOUND_ERROR_CODE,
    CUSTOMER_NOT_FOUND_ERROR_DETAIL, CUSTOMER_OBJECT_TYPE,
};
pub use batch::{
    CreateLegacyImportBatchRequest, ImportRowRequest, LegacyImportBatchListItem, LegacyImportBatchListParams,
    LegacyImportBatchListQuery, LegacyImportBatchView,
};
pub use confirmation::{
    CompleteImportBusinessConfirmationCommand, CreateLegacyImportConfirmationRequest,
    ImportBusinessConfirmationDecision, ImportBusinessConfirmationNextStep,
    ImportBusinessConfirmationResultStatus, LegacyImportConfirmationListParams,
    LegacyImportConfirmationListQuery, PreparedConfirmationCompletion,
};
pub use execution::{
    ImportExecutionAction, ImportExecutionCommand, ImportExecutionNextStep, ImportExecutionResult,
    ImportExecutionResultStatus, PreparedImportExecution,
};
pub use row::{LegacyImportRowListParams, LegacyImportRowListQuery, LegacyImportRowView};

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;
    use serde_json::json;
    use validator::Validate;

    use super::{LegacyImportBatchListParams, SortDir, normalize_sort};

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("id".to_string()), &None, &["created_at", "batch_no"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" batch_no ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "batch_no"],
        )
        .unwrap();
        assert_eq!(field, "batch_no");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn batch_list_params_normalize_paging_filters_and_sort_defaults() {
        let params = LegacyImportBatchListParams {
            batch_no: Some(" IMP-1 ".to_string()),
            baseline_date_from: Some(BusinessDate::from_ymd(2026, 1, 1).unwrap()),
            ..Default::default()
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.batch_no.as_deref(), Some("IMP-1"));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params =
            LegacyImportBatchListParams { page: Some(0), page_size: Some(u32::MAX), ..Default::default() };
        assert!(params.validate().is_err());
    }

    #[test]
    fn create_batch_request_rejects_empty_rows() {
        let request: super::CreateLegacyImportBatchRequest = serde_json::from_value(json!({
            "batch_no": "IMP-2026-001",
            "source_system_id": "sys-1",
            "source_object_set": "CUSTOMER",
            "baseline_date": "2026-01-01",
            "import_rule_version": "v1",
            "rows": []
        }))
        .unwrap();
        assert!(request.validate().is_err());
    }

    #[test]
    fn create_batch_request_rejects_illegal_inner_row() {
        let bad_row = super::ImportRowRequest {
            source_object_type: "   ".to_string(),
            source_row_key: "key-1".to_string(),
            normalized_payload_reference: "payload:1".to_string(),
        };
        let request = super::CreateLegacyImportBatchRequest {
            batch_no: "IMP-2026-001".to_string(),
            source_system_id: erp_core::ids::SourceSystemId::new("sys-1"),
            source_object_set: "CUSTOMER".to_string(),
            baseline_date: erp_core::common::time::BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            import_rule_version: "v1".to_string(),
            source_file_hmac: None,
            successful_sanitized_file_asset_id: None,
            success_manifest_file_asset_id: None,
            failure_diagnostic_file_asset_id: None,
            rows: vec![bad_row],
        };
        assert!(request.validate().is_err());
    }

    #[test]
    fn prepared_confirmation_enforces_action_reason_matrix() {
        use crate::entity::legacy_import::ConfirmationDecision;
        fn decision(
            action: ConfirmationDecision,
            reason: Option<&str>,
        ) -> super::ImportBusinessConfirmationDecision {
            super::ImportBusinessConfirmationDecision {
                batch_id: erp_core::ids::LegacyImportBatchId::new("batch-1"),
                expected_batch_version: "1".to_string(),
                expected_trial_version: "2".to_string(),
                confirmation_scope: " sales ".to_string(),
                action,
                reason_code: reason.map(str::to_string),
                comment: Some(" 意见 ".to_string()),
            }
        }
        fn command(
            action: ConfirmationDecision,
            reason: Option<&str>,
        ) -> super::CompleteImportBusinessConfirmationCommand {
            super::CompleteImportBusinessConfirmationCommand {
                work_item_id: erp_core::ids::WorkItemId::new("work-item-1"),
                expected_task_version: "3".to_string(),
                expected_subject_version: "subject-1".to_string(),
                decision: decision(action, reason),
                idempotency_key: " request-1 ".to_string(),
            }
        }
        assert!(
            super::PreparedConfirmationCompletion::try_from(command(
                ConfirmationDecision::ReturnForFix,
                None
            ))
            .is_err()
        );
        assert!(
            super::PreparedConfirmationCompletion::try_from(command(
                ConfirmationDecision::ConfirmScope,
                Some("REWORK")
            ))
            .is_err()
        );
        let prepared = super::PreparedConfirmationCompletion::try_from(command(
            ConfirmationDecision::ReturnForFix,
            Some("REWORK"),
        ))
        .unwrap();
        assert_eq!(prepared.confirmation_scope, "SALES");
        assert_eq!(prepared.idempotency_key, "request-1");
        assert!(
            super::PreparedConfirmationCompletion::try_from(command(
                ConfirmationDecision::ConfirmScope,
                None
            ))
            .is_ok()
        );
    }

    #[test]
    fn prepared_execution_enforces_trial_reason_matrix() {
        fn command(
            action: super::ImportExecutionAction,
            trial: Option<&str>,
            reason: Option<&str>,
        ) -> super::ImportExecutionCommand {
            super::ImportExecutionCommand {
                batch_id: erp_core::ids::LegacyImportBatchId::new("batch-1"),
                expected_batch_version: "4".to_string(),
                expected_trial_version: trial.map(str::to_string),
                action,
                reason_code: reason.map(str::to_string),
                comment: None,
                request_id: " request-1 ".to_string(),
            }
        }
        assert!(
            super::PreparedImportExecution::try_from(command(
                super::ImportExecutionAction::StartApply,
                None,
                None
            ))
            .is_err()
        );
        assert!(
            super::PreparedImportExecution::try_from(command(
                super::ImportExecutionAction::CancelPending,
                None,
                None
            ))
            .is_err()
        );
        assert!(
            super::PreparedImportExecution::try_from(command(
                super::ImportExecutionAction::StartApply,
                Some("2"),
                Some("REASON")
            ))
            .is_err()
        );
        let prepared = super::PreparedImportExecution::try_from(command(
            super::ImportExecutionAction::RetryFailed,
            Some("2"),
            Some("REASON"),
        ))
        .unwrap();
        assert_eq!(prepared.request_id, "request-1");
        assert_eq!(prepared.expected_trial_version, Some(2));
    }

    #[test]
    fn import_execution_command_uses_frozen_wire_shape() {
        let request: super::ImportExecutionCommand = serde_json::from_value(json!({
            "batch_id": "batch-1",
            "expected_batch_version": "7",
            "expected_trial_version": "3",
            "action": "START_APPLY",
            "comment": "提交应用",
            "request_id": "request-1"
        }))
        .unwrap();

        assert_eq!(request.action, super::ImportExecutionAction::StartApply);
        assert_eq!(request.expected_batch_version, "7");
        assert_eq!(request.expected_trial_version.as_deref(), Some("3"));
        assert!(request.validate().is_ok());
    }

    #[test]
    fn import_execution_command_rejects_unknown_fields_and_numeric_versions() {
        let unknown = json!({
            "batch_id": "batch-1",
            "expected_batch_version": "7",
            "expected_trial_version": "3",
            "action": "START_APPLY",
            "request_id": "request-1",
            "start_immediately": true
        });
        assert!(serde_json::from_value::<super::ImportExecutionCommand>(unknown).is_err());

        let numeric = json!({
            "batch_id": "batch-1",
            "expected_batch_version": 7,
            "expected_trial_version": 3,
            "action": "START_APPLY",
            "request_id": "request-1"
        });
        assert!(serde_json::from_value::<super::ImportExecutionCommand>(numeric).is_err());
    }

    #[test]
    fn page_params_exposes_repository_sort_shape() {
        let params = LegacyImportBatchListParams {
            sort_by: Some(" batch_no ".to_string()),
            sort_dir: Some(" asc ".to_string()),
            ..Default::default()
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.paging.sort_field(), "batch_no");
        assert!(query.paging.sort_ascending());
    }
}
