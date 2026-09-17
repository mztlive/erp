//! 结算复核的本域快照、差异完整性与岗位分离规则。
use mongodb::Database;
use persistence_core::Executor;

use super::shared::{load_statement_differences, load_statement_items};
use crate::dto::supplier_settlement::{self as dto, *};
use crate::entity::supplier_settlement::{SupplierSettlementDifference, SupplierSettlementStatement};
use crate::{Error, Result};
/// 结算复核的固定财务责任角色。
pub const SETTLEMENT_REVIEW_OWNER_ROLE: &str = "role-finance";

/// 复核任务责任组织取结算单内部组织，禁止公司根。
///
/// # 参数
/// * `statement` - 已规范化业务组织的结算单
///
/// # 返回
/// 结算单 `business_org_unit_id`。
///
/// # 错误
/// 空组织或 `"company"` 根时返回校验错误。
pub fn review_owner_organization_id(statement: &SupplierSettlementStatement) -> Result<&str> {
    let org = statement.business_org_unit_id.as_str();
    if org.trim().is_empty() || org.eq_ignore_ascii_case("company") {
        return Err(Error::ValidationError("结算单缺少有效内部组织，禁止写入公司根".to_string()));
    }
    Ok(org)
}

/// 正式复核任务身份须绑定财务角色与结算单业务组织。
///
/// # 参数
/// * `owner_role` - 任务责任角色
/// * `owner_organization_id` - 任务责任组织
/// * `statement` - 当前结算单
///
/// # 返回
/// 角色与内部组织均匹配时为 `true`；公司根或组织不一致为 `false`。
pub fn review_task_identity_matches(
    owner_role: &str,
    owner_organization_id: &str,
    statement: &SupplierSettlementStatement,
) -> bool {
    owner_role == SETTLEMENT_REVIEW_OWNER_ROLE
        && review_owner_organization_id(statement).is_ok_and(|org| owner_organization_id == org)
}

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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::{BusinessDate, Instant};
    use erp_core::ids::{SupplierAccountId, SupplierSettlementStatementId};
    use erp_core::money::Amount;

    use super::*;
    use crate::entity::supplier_settlement::SupplierSettlementStatementData;
    use crate::service::supplier_settlement::shared::{
        REVIEW_CUTOFF_POLICY_ID, REVIEW_CUTOFF_POLICY_VERSION,
    };

    fn sample_statement() -> SupplierSettlementStatement {
        let mut statement = SupplierSettlementStatement::new(
            SupplierSettlementStatementId::new("statement-1"),
            SupplierSettlementStatementData {
                statement_no: "ST-2026-001".to_string(),
                supplier_id: SupplierAccountId::new("supplier-1"),
                period_start: BusinessDate::from_ymd(2026, 7, 1).unwrap(),
                period_end: BusinessDate::from_ymd(2026, 7, 31).unwrap(),
                period_policy_id: "calendar-month".to_string(),
                period_policy_version: "1".to_string(),
                period_timezone: "Asia/Shanghai".to_string(),
                external_bill_no: Some("BILL-1".to_string()),
                external_bill_version: Some("1".to_string()),
                erp_amount: Amount::from_str("100.00").unwrap(),
                supplier_amount: Amount::from_str("101.00").unwrap(),
                subject_hash: "a".repeat(64),
                source_as_of: Instant::from_unix_secs(1_700_000_000),
                source_snapshot_at: Instant::from_unix_secs(1_700_000_000),
                source_snapshot_hash: "b".repeat(64),
                refresh_cutoff_policy_id: REVIEW_CUTOFF_POLICY_ID.to_string(),
                refresh_cutoff_policy_version: REVIEW_CUTOFF_POLICY_VERSION.to_string(),
                prepared_by: "preparer-1".to_string(),
                business_org_unit_id: "org-finance".to_string(),
                difference_handler_user_id: String::new(),
            },
        )
        .unwrap();
        statement.update_subject_hash(statement.review_subject_hash(&[])).unwrap();
        statement
    }

    #[test]
    fn review_owner_organization_rejects_company_and_empty() {
        let mut statement = sample_statement();
        assert_eq!(review_owner_organization_id(&statement).unwrap(), "org-finance");
        statement.business_org_unit_id = "company".into();
        assert!(review_owner_organization_id(&statement).is_err());
        statement.business_org_unit_id = "Company".into();
        assert!(review_owner_organization_id(&statement).is_err());
        statement.business_org_unit_id.clear();
        assert!(review_owner_organization_id(&statement).is_err());
    }

    #[test]
    fn review_task_identity_binds_finance_role_and_statement_org() {
        let statement = sample_statement();
        assert!(review_task_identity_matches(SETTLEMENT_REVIEW_OWNER_ROLE, "org-finance", &statement));
        assert!(!review_task_identity_matches(SETTLEMENT_REVIEW_OWNER_ROLE, "company", &statement));
        assert!(!review_task_identity_matches(SETTLEMENT_REVIEW_OWNER_ROLE, "other-org", &statement));
        assert!(!review_task_identity_matches("role-other", "org-finance", &statement));
    }
}
