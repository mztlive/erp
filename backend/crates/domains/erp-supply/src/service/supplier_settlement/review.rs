//! 结算复核的本域快照、差异完整性与岗位分离规则。
use mongodb::Database;
use persistence_core::Executor;

use super::shared::{load_statement_differences, load_statement_items};
use crate::dto::supplier_settlement::{self as dto, *};
use crate::entity::supplier_settlement::{SupplierSettlementDifference, SupplierSettlementStatement};
use crate::{Error, Result};
/// 结算复核的固定财务责任角色。
pub const SETTLEMENT_REVIEW_OWNER_ROLE: &str = "role-finance";

/// 从结算单读取内部业务组织供分派财务复核任务；空值或公司根 `company` 不是可用的责任组织，返回校验错误。
pub fn review_owner_organization_id(statement: &SupplierSettlementStatement) -> Result<&str> {
    let org = statement.business_org_unit_id.as_str();
    if org.trim().is_empty() || org.eq_ignore_ascii_case("company") {
        return Err(Error::ValidationError("结算单缺少有效内部组织，禁止写入公司根".to_string()));
    }
    Ok(org)
}

/// 核验现有复核任务是否仍属于结算单业务组织和固定财务角色；非法组织或任一身份不符时不能据此办理任务。
pub fn review_task_identity_matches(
    owner_role: &str,
    owner_organization_id: &str,
    statement: &SupplierSettlementStatement,
) -> bool {
    owner_role == SETTLEMENT_REVIEW_OWNER_ROLE
        && review_owner_organization_id(statement).is_ok_and(|org| owner_organization_id == org)
}

/// 身份域授权提交后，在原执行器检查结算单状态可编辑、已有冻结明细、差异已处理且主题摘要仍对应其结论。不
/// 可编辑或无待处理差异但摘要变化时报冲突，缺明细或仍有待处理差异时报业务错误，读库失败向上传播。
pub async fn ensure_review_submission_ready(
    db: &Database,
    statement: &SupplierSettlementStatement,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !statement.is_editable() {
        return Err(Error::ConflictError("结算单责任或状态已变化，请刷新后重试".to_string()));
    }

    let items = load_statement_items(db, &statement.base.id, executor).await?;
    if items.is_empty() {
        return Err(Error::BusinessLogicError("结算单没有冻结明细".to_string()));
    }

    let differences = load_statement_differences(db, &items, executor).await?;
    ensure_current_subject_and_resolved_differences(statement, &differences)
}

/// 接受结算单提交复核前，核对命令动作以及所见主题摘要、刷新截止策略 ID 和版本。动作不符返回校验错误，快
/// 照变化须刷新后重试。
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

/// 以正式差异结论重新核对当前复核主题，防止复核忽略新差异。核验失败且仍有待处理差异时报业务错误，否则作
/// 为需刷新结算资料的冲突。
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

/// 把动作名、稳定阻断代码和说明组装为复核界面的响应项；调用方先判定阻断，此处不解释权限或任务状态。
pub fn review_blocker(action: &str, code: &str, message: &str) -> dto::SettlementReviewActionBlockerView {
    dto::SettlementReviewActionBlockerView {
        action: action.to_string(),
        code: code.to_string(),
        message: message.to_string(),
    }
}

/// 根据当前任务资格、经办与复核岗位分离及任务归属投影按钮。三项均满足才提供 `REJECT` 和 `CONFIRM`；否则
/// 返回对应阻断原因，不把投影视为正式授权。
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
/// 执行复核决定前拒绝结算经办人复核自己的结算单；违反岗位分离时返回 `Forbidden`。
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
