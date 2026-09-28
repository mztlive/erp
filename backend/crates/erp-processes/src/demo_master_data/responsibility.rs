//! 补齐默认采购调度人和财务付款、开票负责人。
//!
//! 已有启用的同层默认规则时保留现有负责人。删除演示主数据时这些规则保留。

use application_core::AuditActor;
use erp_procurement::dto::procurement_responsibility::CreateProcurementResponsibilityRuleRequest;
use erp_procurement::entity::procurement_responsibility::{
    EnableStatus as ProcurementStatus, ProcurementResponsibilityRuleType,
};
use erp_procurement::repository::procurement_responsibility::ProcurementResponsibilityRuleFilter;
use erp_procurement::repository::{ProcurementResponsibilityExt, ProcurementResponsibilityRuleRepositoryExt};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{
    EnableStatus as FinanceStatus, FinanceResponsibilityOperation, FinanceResponsibilityScope,
};
use erp_workflow::repository::prelude::FinanceResponsibilityRuleRepositoryExt;
use erp_workflow::service::work_item::CreateFinanceResponsibilityRuleRequest;
use persistence_core::NoTransaction;

use super::accounts::PreparedAccounts;
use super::{DemoFoundationReport, DemoMasterDataService, spec};
use crate::adapters::workflow::work_item_service;
use crate::procure_to_pay::responsibility::ProcurementResponsibilityProcess;
use crate::{Error, Result};

const RULE_PAGE_SIZE: u32 = 100;

impl DemoMasterDataService {
    pub(super) async fn ensure_responsibilities(
        &self,
        actor: &AuditActor,
        accounts: &PreparedAccounts,
        report: &mut DemoFoundationReport,
    ) -> Result<()> {
        let spec = spec::foundation_spec();
        let procurement_owner = account_id(accounts, &spec.procurement_responsibility_owner)?;
        self.ensure_procurement_dispatcher(actor, &procurement_owner, report).await?;
        for rule in &spec.finance_responsibilities {
            let owner = account_id(accounts, &rule.owner)?;
            self.ensure_finance_default(actor, rule.operation, &owner, report).await?;
        }
        Ok(())
    }

    async fn ensure_procurement_dispatcher(
        &self,
        actor: &AuditActor,
        owner_user_id: &str,
        report: &mut DemoFoundationReport,
    ) -> Result<()> {
        if should_create_responsibility(&self.procurement_default_flags().await?) {
            self.create_procurement_dispatcher(actor, owner_user_id).await?;
            report.responsibilities_created += 1;
        } else {
            report.responsibilities_existing += 1;
        }
        Ok(())
    }

    async fn ensure_finance_default(
        &self,
        actor: &AuditActor,
        operation: FinanceResponsibilityOperation,
        owner_user_id: &str,
        report: &mut DemoFoundationReport,
    ) -> Result<()> {
        if should_create_responsibility(&self.finance_default_flags(operation).await?) {
            self.create_finance_default(actor, operation, owner_user_id).await?;
            report.responsibilities_created += 1;
        } else {
            report.responsibilities_existing += 1;
        }
        Ok(())
    }

    async fn procurement_default_flags(&self) -> Result<Vec<bool>> {
        let mut page = 1_u64;
        let mut flags = Vec::new();
        loop {
            let found = self
                .db
                .procurement_responsibility_rules()
                .search_procurement_responsibility_rules(&dispatcher_filter(page), &mut NoTransaction)
                .await?;
            let batch = found.items.len();
            flags.extend(found.items.iter().map(|rule| rule.is_active()));
            let counted = i64::try_from(flags.len())
                .map_err(|_| Error::Internal("采购责任规则数量溢出".to_string()))?;
            if counted >= found.total || batch == 0 {
                return Ok(flags);
            }
            page = page.checked_add(1).ok_or_else(|| Error::Internal("采购责任规则分页溢出".to_string()))?;
        }
    }

    async fn finance_default_flags(&self, operation: FinanceResponsibilityOperation) -> Result<Vec<bool>> {
        let rules = self
            .db
            .finance_responsibility_rules()
            .list_finance_responsibility_rules(&mut NoTransaction)
            .await?;
        Ok(rules
            .into_iter()
            .filter(|rule| rule.operation == operation && rule.scope == FinanceResponsibilityScope::Default)
            .map(|rule| rule.is_active())
            .collect())
    }

    async fn create_procurement_dispatcher(&self, actor: &AuditActor, owner_user_id: &str) -> Result<()> {
        ProcurementResponsibilityProcess::new(self.db.clone(), self.rbac.clone())
            .create_rule(
                CreateProcurementResponsibilityRuleRequest {
                    rule_type: ProcurementResponsibilityRuleType::DefaultDispatcher,
                    sku_id: None,
                    category_id: None,
                    service_region: None,
                    product_kind: None,
                    owner_user_id: owner_user_id.to_string(),
                    status: ProcurementStatus::Active,
                },
                actor,
            )
            .await?;
        Ok(())
    }

    async fn create_finance_default(
        &self,
        actor: &AuditActor,
        operation: FinanceResponsibilityOperation,
        owner_user_id: &str,
    ) -> Result<()> {
        work_item_service(self.db.clone(), self.rbac.clone())
            .create_finance_responsibility_rule(
                CreateFinanceResponsibilityRuleRequest {
                    operation,
                    scope: FinanceResponsibilityScope::Default,
                    counterparty_id: None,
                    owner_user_id: owner_user_id.to_string(),
                    status: FinanceStatus::Active,
                },
                actor.clone(),
            )
            .await?;
        Ok(())
    }
}

fn account_id(accounts: &PreparedAccounts, key: &str) -> Result<String> {
    accounts.by_key.get(key).cloned().ok_or_else(|| Error::NotFound(format!("责任人 {key} 尚未建号")))
}

/// 没有启用的同层默认规则时才创建。停用规则不参与解析，因此不阻止新建。
fn should_create_responsibility(active: &[bool]) -> bool {
    !active.contains(&true)
}

fn dispatcher_filter(page: u64) -> ProcurementResponsibilityRuleFilter {
    ProcurementResponsibilityRuleFilter {
        rule_type: Some(ProcurementResponsibilityRuleType::DefaultDispatcher),
        owner_user_id: None,
        status: None,
        page,
        page_size: RULE_PAGE_SIZE,
    }
}

#[cfg(test)]
mod tests {
    use super::should_create_responsibility;

    #[test]
    fn missing_or_disabled_default_rule_is_created() {
        assert!(should_create_responsibility(&[]));
        assert!(should_create_responsibility(&[false, false]));
    }

    #[test]
    fn active_default_rule_keeps_its_owner() {
        assert!(!should_create_responsibility(&[true]));
        assert!(!should_create_responsibility(&[false, true]));
    }
}
