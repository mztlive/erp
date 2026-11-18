//! 结算命令按实际制单人或差异处理人登记对象政策。
use std::sync::Arc;

use async_trait::async_trait;
use erp_identity::entity::authorization::{AccessContext, FactValue, ObjectFacts};
use erp_identity::{AccessFactsPort, ResourceTargets};
use erp_permissions::{Permission, internal, supplier_settlement};
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::supplier_settlement::SupplierSettlementStatement;
use crate::repository::SupplierSettlementExt;

/// 结算单命令用途；功能权限继续使用既有 update 或 submit。
#[derive(Clone, Copy)]
pub enum SettlementAction {
    /// 在已授权结算单追加证据。
    AppendEvidence,
    /// 当前制单人刷新试算。
    Refresh,
    /// 当前制单人作废草稿。
    Void,
    /// 当前差异处理人登记正式结论。
    ResolveDifference,
    /// 当前制单人提交复核。
    Submit,
}
impl SettlementAction {
    pub(super) fn permission(self) -> Permission {
        match self {
            Self::Submit => supplier_settlement::STATEMENT_SUBMIT_REVIEW,
            Self::AppendEvidence => supplier_settlement::STATEMENT_REFRESH,
            Self::Refresh => internal::SETTLEMENT_REFRESH_PREPARED,
            Self::Void => internal::SETTLEMENT_VOID_PREPARED,
            Self::ResolveDifference => internal::SETTLEMENT_RESOLVE_DIFFERENCE,
        }
    }
}

/// identity 已核验的结算详情动作资格。
#[derive(Default)]
pub struct SettlementActionAccess {
    /// 可以追加证据。
    pub append_evidence: bool,
    /// 可以刷新试算。
    pub refresh: bool,
    /// 可以作废草稿。
    pub void: bool,
    /// 可以登记正式差异结论。
    pub resolve_difference: bool,
    /// 可以提交复核。
    pub submit: bool,
}

pub(crate) fn facts(db: Database) -> Arc<dyn AccessFactsPort> {
    Arc::new(SettlementFacts(db))
}

struct SettlementFacts(Database);
#[async_trait]
impl AccessFactsPort for SettlementFacts {
    async fn load(
        &self,
        _: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<Vec<ObjectFacts>> {
        let mut facts = Vec::new();
        for id in ids.ids() {
            let Some(statement) = self.0.supplier_settlement_statements().find_by_id(id, executor).await?
            else {
                continue;
            };
            facts.push(statement_fact(statement));
        }
        Ok(facts)
    }
}

fn statement_fact(statement: SupplierSettlementStatement) -> ObjectFacts {
    ObjectFacts {
        resource: "supplier_settlement_statement".into(),
        id: statement.base.id.clone(),
        version: statement.base.version.to_string(),
        owner_id: FactValue::Known(crate::authorization::optional_identity(&statement.prepared_by)),
        org_unit_id: FactValue::Known(crate::authorization::optional_identity(
            &statement.business_org_unit_id,
        )),
        assignee_id: FactValue::Known(Some(statement.difference_handler().into())),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::{BusinessDate, Instant};
    use erp_core::ids::{SupplierAccountId, SupplierSettlementStatementId};

    use super::*;
    use crate::entity::supplier_settlement::SupplierSettlementStatementData;
    use crate::service::supplier_settlement::query::settlement_object_actions;

    fn statement() -> SupplierSettlementStatement {
        SupplierSettlementStatement::new(
            SupplierSettlementStatementId::new("statement"),
            SupplierSettlementStatementData {
                statement_no: "ST-1".into(),
                supplier_id: SupplierAccountId::new("supplier"),
                period_start: BusinessDate::from_ymd(2026, 7, 1).unwrap(),
                period_end: BusinessDate::from_ymd(2026, 7, 31).unwrap(),
                period_policy_id: "calendar-month".into(),
                period_policy_version: "1".into(),
                period_timezone: "Asia/Shanghai".into(),
                external_bill_no: None,
                external_bill_version: None,
                erp_amount: "100".parse().unwrap(),
                supplier_amount: "100".parse().unwrap(),
                subject_hash: "a".repeat(64),
                source_as_of: Instant::from_unix_secs(1),
                source_snapshot_at: Instant::from_unix_secs(1),
                source_snapshot_hash: "b".repeat(64),
                refresh_cutoff_policy_id: "cutoff".into(),
                refresh_cutoff_policy_version: "1".into(),
                prepared_by: "preparer".into(),
                business_org_unit_id: "org".into(),
                difference_handler_user_id: String::new(),
            },
        )
        .unwrap()
    }

    #[test]
    fn statement_facts_preserve_real_responsibility_and_version() {
        let mut statement = statement();
        statement.base.version = 9;
        let fallback = statement_fact(statement.clone());
        assert_eq!(fallback.owner_id, FactValue::Known(Some("preparer".into())));
        assert_eq!(fallback.assignee_id, FactValue::Known(Some("preparer".into())));

        statement.difference_handler_user_id = "handler".into();
        let facts = statement_fact(statement);
        assert_eq!(facts.id, "statement");
        assert_eq!(facts.version, "9");
        assert_eq!(facts.owner_id, fallback.owner_id);
        assert_eq!(facts.org_unit_id, FactValue::Known(Some("org".into())));
        assert_eq!(facts.assignee_id, FactValue::Known(Some("handler".into())));
    }

    // 验证旧结算无业务组织时显式无组织，不将供应商或创建人填入内部范围。
    #[test]
    fn empty_statement_organization_remains_known_absent() {
        let mut statement = statement();
        statement.business_org_unit_id.clear();
        assert_eq!(statement_fact(statement).org_unit_id, FactValue::Known(None));
    }

    #[test]
    fn detail_actions_consume_each_identity_command_result() {
        let access = SettlementActionAccess { refresh: true, submit: true, ..Default::default() };
        let (actions, blockers, _) = settlement_object_actions(&statement(), &[], &access);
        assert!(actions.iter().any(|action| action == "REFRESH_TRIAL"));
        assert!(actions.iter().any(|action| action == "SUBMIT_REVIEW"));
        assert!(!actions.iter().any(|action| action == "VOID_DRAFT" || action == "RESOLVE_DIFFERENCE"));
        assert_eq!(blockers.len(), 3);
    }
}
