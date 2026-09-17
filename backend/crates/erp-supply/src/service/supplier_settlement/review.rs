//! 结算复核的本域快照、差异完整性与岗位分离规则。
use mongodb::Database;
use persistence_core::Executor;

use super::shared::{load_statement_differences, load_statement_items};
use crate::dto::supplier_settlement::{self as dto, *};
use crate::entity::supplier_settlement::{SupplierSettlementDifference, SupplierSettlementStatement};
use crate::{Error, Result};
/// 结算复核的固定财务责任角色。
pub const SETTLEMENT_REVIEW_OWNER_ROLE: &str = "role-finance";
/// WorkItem 责任组织身份；不是内部组织 DataScope 事实，不得当作业务组织。
pub const SETTLEMENT_REVIEW_OWNER_ORGANIZATION_ID: &str = "company";
pub async fn ensure_review_submission_ready(
    db: &Database,
    statement: &SupplierSettlementStatement,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !statement.is_prepared_by(actor_id) || !statement.is_editable() {
        return Err(Error::ConflictError("结算单责任或状态已变化，请刷新后重试".to_string()));
    }
    let items = load_statement_items(db, &statement.base.id, executor).await?;
    if items.is_empty() {
        return Err(Error::BusinessLogicError("结算单没有冻结明细".to_string()));
    }
    let differences = load_statement_differences(db, &items, executor).await?;
    ensure_current_subject_and_resolved_differences(statement, &differences)
}

pub fn validate_review_submission_snapshot(
    statement: &SupplierSettlementStatement,
    req: &SubmitSettlementReviewRequest,
) -> Result<()> {
    if !matches!(req.action, dto::SettlementObjectAction::SubmitReview) {
        return Err(Error::ValidationError("结算对象动作不受支持".to_string()));
    }
    statement
        .ensure_review_snapshot(
            &req.subject_hash,
            &req.refresh_cutoff_policy_id,
            &req.expected_refresh_cutoff_policy_version,
        )
        .map_err(|_| Error::ConflictError("结算主题或刷新截止策略已变化，请刷新后重试".to_string()))
}

pub fn ensure_current_subject_and_resolved_differences(
    statement: &SupplierSettlementStatement,
    differences: &[SupplierSettlementDifference],
) -> Result<()> {
    statement.ensure_resolved_subject(differences).map_err(|error| {
        if differences.iter().any(SupplierSettlementDifference::is_pending) {
            Error::BusinessLogicError(error.to_string())
        } else {
            Error::ConflictError("结算主题摘要与当前差异结论不一致，请刷新后重试".to_string())
        }
    })
}

pub fn review_blocker(action: &str, code: &str, message: &str) -> dto::SettlementReviewActionBlockerView {
    dto::SettlementReviewActionBlockerView {
        action: action.to_string(),
        code: code.to_string(),
        message: message.to_string(),
    }
}

pub fn settlement_review_access(
    owned: bool,
    eligible: bool,
    separation_satisfied: bool,
) -> (Vec<String>, Vec<dto::SettlementReviewActionBlockerView>) {
    if !eligible {
        return (
            Vec::new(),
            vec![review_blocker(
                "REVIEW_DECISION",
                "ASSIGNMENT_NOT_ELIGIBLE",
                "当前账号不具备该任务的财务角色或组织数据范围",
            )],
        );
    }
    if !separation_satisfied {
        return (
            Vec::new(),
            vec![review_blocker(
                "REVIEW_DECISION",
                "SEGREGATION_OF_DUTIES",
                "结算经办人不得复核自己的结算单",
            )],
        );
    }
    if owned {
        return (vec!["REJECT".to_string(), "CONFIRM".to_string()], Vec::new());
    }
    (
        Vec::new(),
        vec![review_blocker("REVIEW_DECISION", "CURRENT_OWNER_MISMATCH", "该复核任务当前由其他账号负责")],
    )
}
pub fn ensure_reviewer_separation(statement: &SupplierSettlementStatement, actor_id: &str) -> Result<()> {
    if statement.is_prepared_by(actor_id) {
        return Err(Error::Forbidden("结算经办人不得复核自己的结算单".to_string()));
    }
    Ok(())
}
