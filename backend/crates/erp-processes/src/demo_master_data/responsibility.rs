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
use erp_workflow::service::work_item::{
    CreateFinanceResponsibilityRuleRequest, FinanceResponsibilityOwnerOptionView,
};
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

    /// 分页读取默认规则，并使用正式采购资格核验保留的启用负责人。
    async fn procurement_default_flags(&self) -> Result<Vec<bool>> {
        let mut page = 1_u64;
        let mut flags = Vec::new();
        let responsibility = ProcurementResponsibilityProcess::new(self.db.clone(), self.rbac.clone());
        loop {
            let found = self
                .db
                .procurement_responsibility_rules()
                .search_procurement_responsibility_rules(&dispatcher_filter(page), &mut NoTransaction)
                .await?;
            let batch = found.items.len();
            for rule in found.items.iter().filter(|rule| rule.is_active()) {
                responsibility.authorize_owner_eligibility(&rule.owner_user_id).await?;
            }
            flags.extend(found.items.iter().map(|rule| rule.is_active()));
            let counted = i64::try_from(flags.len())
                .map_err(|_| Error::Internal("采购责任规则数量溢出".to_string()))?;
            if counted >= found.total || batch == 0 {
                return Ok(flags);
            }
            page = page.checked_add(1).ok_or_else(|| Error::Internal("采购责任规则分页溢出".to_string()))?;
        }
    }

    /// 保留既有负责人前复用工作项服务的完整执行资格，失效规则阻断准备。
    async fn finance_default_flags(&self, operation: FinanceResponsibilityOperation) -> Result<Vec<bool>> {
        let rules = self
            .db
            .finance_responsibility_rules()
            .list_finance_responsibility_rules(&mut NoTransaction)
            .await?;
        let defaults = rules
            .into_iter()
            .filter(|rule| rule.operation == operation && rule.scope == FinanceResponsibilityScope::Default)
            .collect::<Vec<_>>();
        if defaults.iter().any(|rule| rule.is_active()) {
            let options = work_item_service(self.db.clone(), self.rbac.clone())
                .finance_responsibility_owner_options()
                .await?;
            for rule in defaults.iter().filter(|rule| rule.is_active()) {
                ensure_finance_owner(operation, &rule.owner_user_id, &options)?;
            }
        }
        Ok(defaults.iter().map(|rule| rule.is_active()).collect())
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

/// 根据正式候选资格核验已启用默认规则，不替换人工选定的负责人。
fn ensure_finance_owner(
    operation: FinanceResponsibilityOperation,
    owner_user_id: &str,
    options: &[FinanceResponsibilityOwnerOptionView],
) -> Result<()> {
    let eligible = options.iter().any(|option| {
        option.user_id == owner_user_id
            && match operation {
                FinanceResponsibilityOperation::SupplierPayment => option.supplier_payment_eligible,
                FinanceResponsibilityOperation::SalesInvoice => option.sales_invoice_eligible,
                FinanceResponsibilityOperation::CardFundsReview => false,
            }
    });
    if eligible {
        return Ok(());
    }
    Err(Error::ValidationError(format!(
        "已启用的默认{}责任人 {owner_user_id} 不可用或缺少完整执行权限，请维护财务责任配置后重新生成",
        operation.label()
    )))
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
    use super::*;

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

    /// 正式候选允许自定义负责人，不要求负责人等于种子账号。
    #[test]
    fn finance_default_keeps_qualified_custom_owner() {
        let options = vec![FinanceResponsibilityOwnerOptionView {
            user_id: "custom-payer".into(),
            display_name: "财务经办".into(),
            account: "custom".into(),
            supplier_payment_eligible: true,
            sales_invoice_eligible: false,
        }];
        ensure_finance_owner(FinanceResponsibilityOperation::SupplierPayment, "custom-payer", &options)
            .unwrap();
        assert!(!should_create_responsibility(&[true]));
    }

    /// 停用、删除或缺权人员不在正式候选中；另一财务操作资格不能替代所需操作。
    #[test]
    fn finance_default_rejects_missing_owner_and_wrong_operation() {
        let options = vec![FinanceResponsibilityOwnerOptionView {
            user_id: "invoice-only".into(),
            display_name: "开票经办".into(),
            account: "invoice-only".into(),
            supplier_payment_eligible: false,
            sales_invoice_eligible: true,
        }];
        for owner in ["unavailable", "invoice-only"] {
            let error =
                ensure_finance_owner(FinanceResponsibilityOperation::SupplierPayment, owner, &options)
                    .unwrap_err();
            assert!(
                matches!(error, Error::ValidationError(message) if message.contains(owner) && message.contains("供应商付款") && message.contains("财务责任配置"))
            );
        }
        ensure_finance_owner(FinanceResponsibilityOperation::SalesInvoice, "invoice-only", &options).unwrap();
    }
}
