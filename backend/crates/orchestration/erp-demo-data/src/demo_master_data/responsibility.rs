//! 补齐默认采购调度人和财务付款、开票负责人。
//!
//! 已有启用的同层默认规则时保留现有负责人。删除演示主数据时这些规则保留。

use application_core::AuditActor;
use erp_core::assignment::AssignmentStrategy;
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
use erp_workflow::ports::WorkflowAuthorizationPort;
use erp_workflow::repository::prelude::FinanceResponsibilityRuleRepositoryExt;
use erp_workflow::service::work_item::{
    CreateFinanceResponsibilityRuleRequest, FinanceResponsibilityOwnerOptionView,
};
use persistence_core::ReadExecutor;

use super::accounts::PreparedAccounts;
use super::{DemoFoundationReport, DemoMasterDataProcess, spec};
use crate::{Error, Result};

const RULE_PAGE_SIZE: u32 = 100;

impl<A: WorkflowAuthorizationPort> DemoMasterDataProcess<A> {
    // 补齐演示业务所需的默认采购调度、付款和开票责任规则。存在同层启用规则时保留并复验原负责人；岗位无法解析、查询或创建失败时停止。
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
        if should_create_responsibility(&self.finance_default_flags(actor, operation).await?) {
            self.create_finance_default(actor, operation, owner_user_id).await?;
            report.responsibilities_created += 1;
        } else {
            report.responsibilities_existing += 1;
        }
        Ok(())
    }

    // 分页查询采购默认规则，核验启用负责人仍具备正式采购资格，再决定保留还是补建。
    async fn procurement_default_flags(&self) -> Result<Vec<bool>> {
        let mut page = 1_u64;
        let mut flags = Vec::new();
        let responsibility = &self.ports.procurement_responsibility;
        loop {
            let found = self
                .db
                .procurement_responsibility_rules()
                .search_procurement_responsibility_rules(
                    &dispatcher_filter(page),
                    &mut ReadExecutor::default(),
                )
                .await?;
            let batch = found.items.len();

            for rule in found.items.iter().filter(|rule| rule.is_active()) {
                for owner in &rule.owner_user_ids {
                    responsibility.authorize_owner_eligibility(owner).await?;
                }
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

    // 按财务操作读取默认责任，并通过工作项正式资格核验既有负责人；启用规则已失效时阻断准备，不自动换人。
    async fn finance_default_flags(
        &self,
        actor: &AuditActor,
        operation: FinanceResponsibilityOperation,
    ) -> Result<Vec<bool>> {
        let rules = self
            .db
            .finance_responsibility_rules()
            .list_finance_responsibility_rules(&mut ReadExecutor::default())
            .await?;
        let defaults = rules
            .into_iter()
            .filter(|rule| rule.operation == operation && rule.scope == FinanceResponsibilityScope::Default)
            .collect::<Vec<_>>();

        if defaults.iter().any(|rule| rule.is_active()) {
            let options = self.ports.work_items.clone().finance_responsibility_owner_options(actor).await?;
            for rule in defaults.iter().filter(|rule| rule.is_active()) {
                for owner in &rule.owner_user_ids {
                    ensure_finance_owner(operation, owner, &options)?;
                }
            }
        }
        Ok(defaults.iter().map(|rule| rule.is_active()).collect())
    }

    async fn create_procurement_dispatcher(&self, actor: &AuditActor, owner_user_id: &str) -> Result<()> {
        self.ports
            .procurement_responsibility
            .create_rule(
                CreateProcurementResponsibilityRuleRequest {
                    rule_type: ProcurementResponsibilityRuleType::DefaultDispatcher,
                    sku_id: None,
                    category_id: None,
                    product_kind: None,
                    owner_user_ids: vec![owner_user_id.to_string()],
                    strategy: Default::default(),
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
        self.ports
            .work_items
            .clone()
            .create_finance_responsibility_rule(
                CreateFinanceResponsibilityRuleRequest {
                    operation,
                    scope: FinanceResponsibilityScope::Default,
                    counterparty_id: None,
                    owner_user_ids: vec![owner_user_id.to_string()],
                    strategy: AssignmentStrategy::RoundRobin,
                    status: FinanceStatus::Active,
                },
                actor.clone(),
            )
            .await?;
        Ok(())
    }
}

// 用正式候选范围核对启用规则里的财务责任人，只确认其是否可办理当前操作，不替换人工选择。
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

// 仅在同层没有启用默认规则时补建；停用规则不参与责任解析，不能作为跳过补建的理由。
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

    // 验证合格的人工财务负责人能保留，即使并非固定演示账号也不重置规则。
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

    // 验证停用、删除或缺权人员不能作为现行默认负责人，具备另一财务操作权限也不能替代本操作资格。
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
