//! 财务责任规则管理、负责人候选与任务生产时的严格解析。

use std::collections::HashMap;

use database::{AccessControlExt, CustomerExt, Executor, NoTransaction, SupplierExt, WorkItemExt};
use entities::catalog::EnableStatus;
use entities::ids::{CustomerAccountId, SupplierAccountId};
use entities::work_item::{
    AvailableWorkItemAccount, FinanceResponsibilityOperation, FinanceResponsibilityRule,
    FinanceResponsibilityRuleData, FinanceResponsibilityRuleSet, FinanceResponsibilityScope,
};
use entities::{AccountKind, Permission, PermissionSet};
use id_generator::next_id;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::WorkItemService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

const AUTHORIZATION_SNAPSHOT_ATTEMPTS: usize = 3;

/// 创建财务责任规则请求。
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateFinanceResponsibilityRuleRequest {
    /// 供应商付款、销项开票或卡券票款复核。
    pub operation: FinanceResponsibilityOperation,
    /// 精确往来方或默认规则。
    pub scope: FinanceResponsibilityScope,
    /// 精确供应商或客户 ID。
    #[validate(length(max = 128, message = "往来方ID过长"))]
    pub counterparty_id: Option<String>,
    /// 具体负责人账号 ID。
    #[validate(length(min = 1, max = 128, message = "财务负责人长度必须在1-128之间"))]
    pub owner_user_id: String,
    /// 启停状态。
    pub status: EnableStatus,
}

impl CreateFinanceResponsibilityRuleRequest {
    fn into_data(self) -> FinanceResponsibilityRuleData {
        FinanceResponsibilityRuleData {
            operation: self.operation,
            scope: self.scope,
            counterparty_id: self.counterparty_id,
            owner_user_id: self.owner_user_id,
            status: self.status,
        }
    }
}

/// 整项更新财务责任规则请求。
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct UpdateFinanceResponsibilityRuleRequest {
    /// 期望乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于0"))]
    pub version: u64,
    /// 供应商付款、销项开票或卡券票款复核。
    pub operation: FinanceResponsibilityOperation,
    /// 精确往来方或默认规则。
    pub scope: FinanceResponsibilityScope,
    /// 精确供应商或客户 ID。
    #[validate(length(max = 128, message = "往来方ID过长"))]
    pub counterparty_id: Option<String>,
    /// 具体负责人账号 ID。
    #[validate(length(min = 1, max = 128, message = "财务负责人长度必须在1-128之间"))]
    pub owner_user_id: String,
    /// 启停状态。
    pub status: EnableStatus,
}

impl UpdateFinanceResponsibilityRuleRequest {
    fn into_parts(self) -> (u64, FinanceResponsibilityRuleData) {
        (
            self.version,
            FinanceResponsibilityRuleData {
                operation: self.operation,
                scope: self.scope,
                counterparty_id: self.counterparty_id,
                owner_user_id: self.owner_user_id,
                status: self.status,
            },
        )
    }
}

/// 财务责任规则管理视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FinanceResponsibilityRuleView {
    /// 规则主键。
    pub id: String,
    /// 供应商付款、销项开票或卡券票款复核。
    pub operation: FinanceResponsibilityOperation,
    /// 精确往来方或默认规则。
    pub scope: FinanceResponsibilityScope,
    /// 精确供应商或客户 ID。
    pub counterparty_id: Option<String>,
    /// 供应商编号或客户编号。
    pub counterparty_no: Option<String>,
    /// 具体负责人账号 ID。
    pub owner_user_id: String,
    /// 负责人姓名。
    pub owner_name: Option<String>,
    /// 启停状态。
    pub status: EnableStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间。
    pub created_at: u64,
    /// 更新时间。
    pub updated_at: u64,
}

impl From<FinanceResponsibilityRule> for FinanceResponsibilityRuleView {
    fn from(rule: FinanceResponsibilityRule) -> Self {
        Self {
            id: rule.base.id,
            operation: rule.operation,
            scope: rule.scope,
            counterparty_id: rule.counterparty_id,
            counterparty_no: None,
            owner_user_id: rule.owner_user_id,
            owner_name: None,
            status: rule.status,
            version: rule.base.version,
            created_at: rule.base.created_at,
            updated_at: rule.base.updated_at,
        }
    }
}

/// 财务责任负责人候选。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FinanceResponsibilityOwnerOptionView {
    /// 账号 ID。
    pub user_id: String,
    /// 姓名。
    pub display_name: String,
    /// 登录账号。
    pub account: String,
    /// 是否具备供应商付款完整执行权限。
    pub supplier_payment_eligible: bool,
    /// 是否具备销项开票完整执行权限。
    pub sales_invoice_eligible: bool,
    /// 是否具备卡券票款复核完整执行权限。
    pub card_funds_review_eligible: bool,
}

/// 任务生产时冻结的财务责任解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedFinanceResponsibility {
    /// 具体负责人账号 ID。
    pub owner_user_id: String,
    /// 写入工作项且后续不随规则更新变化的责任键。
    pub responsibility_key: String,
}

impl WorkItemService {
    /// 查询全部财务责任规则。
    ///
    /// # 错误
    /// 规则、账号或往来方读取失败时返回错误。
    pub async fn finance_responsibility_rule_list(&self) -> Result<Vec<FinanceResponsibilityRuleView>> {
        let rules = self
            .db
            .finance_responsibility_rules()
            .list_finance_responsibility_rules(&mut NoTransaction)
            .await?;
        self.finance_responsibility_views(rules).await
    }

    /// 创建财务责任规则并记录审计。
    ///
    /// # 错误
    /// 匹配范围、往来方、负责人资格、唯一性或事务写入不满足时返回错误。
    pub async fn create_finance_responsibility_rule(
        &self,
        request: CreateFinanceResponsibilityRuleRequest,
        actor: &AuditActor,
    ) -> Result<FinanceResponsibilityRuleView> {
        let data = request.into_data();
        let probe = self
            .validate_finance_rule_data(&data, true, false, &mut NoTransaction)
            .await?;
        let policy_revision = self
            .authorize_finance_owner_eligibility(probe.operation, &probe.owner_user_id)
            .await?;
        let rule =
            FinanceResponsibilityRule::new(next_id(), data.clone(), actor.id()).map_err(Error::Logic)?;
        let audit = actor.clone().resource_log(
            "finance_responsibility_rule.create",
            "finance_responsibility_rule",
            rule.base.id.clone(),
        )?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let rule = rbac
            .clone()
            .run_authorized_policy_transaction(policy_revision, move |session| {
                let service = WorkItemService::new(db.clone(), rbac.clone());
                let data = data.clone();
                let rule = rule.clone();
                let audit = audit.clone();
                Box::pin(async move {
                    service
                        .validate_finance_rule_data(&data, true, true, session)
                        .await?;
                    db.finance_responsibility_rules().create(&rule, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<FinanceResponsibilityRule, crate::errors::Error>(rule)
                })
            })
            .await?;
        self.single_finance_responsibility_view(rule).await
    }

    /// 整项更新财务责任规则并记录审计。
    ///
    /// # 错误
    /// 规则不存在、版本冲突、往来方或负责人资格不满足时返回错误。
    pub async fn update_finance_responsibility_rule(
        &self,
        id: &str,
        request: UpdateFinanceResponsibilityRuleRequest,
        actor: &AuditActor,
    ) -> Result<FinanceResponsibilityRuleView> {
        let (version, data) = request.into_parts();
        let probe = self
            .validate_finance_rule_data(&data, false, false, &mut NoTransaction)
            .await?;
        let current = self
            .db
            .finance_responsibility_rules()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("财务责任规则不存在".to_string()))?;
        if current.base.version != version {
            return Err(Error::ConflictError("财务责任规则版本已变化".to_string()));
        }
        let counterparty_must_be_eligible = probe.status.is_active()
            || probe.operation != current.operation
            || probe.scope != current.scope
            || probe.counterparty_id != current.counterparty_id;
        let owner_must_be_eligible = probe.status.is_active()
            || probe.owner_user_id != current.owner_user_id
            || probe.operation != current.operation;
        self.validate_finance_rule_data(
            &data,
            counterparty_must_be_eligible,
            owner_must_be_eligible,
            &mut NoTransaction,
        )
        .await?;
        let policy_revision = if owner_must_be_eligible {
            self.authorize_finance_owner_eligibility(probe.operation, &probe.owner_user_id)
                .await?
        } else {
            self.rbac.current_policy_revision().await?
        };
        let audit = actor.clone().resource_log(
            "finance_responsibility_rule.update",
            "finance_responsibility_rule",
            id.to_string(),
        )?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let id = id.to_string();
        let updated_by = actor.id().to_string();
        let rule = rbac
            .clone()
            .run_authorized_policy_transaction(policy_revision, move |session| {
                let service = WorkItemService::new(db.clone(), rbac.clone());
                let data = data.clone();
                let id = id.clone();
                let updated_by = updated_by.clone();
                let audit = audit.clone();
                Box::pin(async move {
                    let mut rule = db
                        .finance_responsibility_rules()
                        .find_by_id(&id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("财务责任规则不存在".to_string()))?;
                    if rule.base.version != version {
                        return Err(Error::ConflictError("财务责任规则版本已变化".to_string()));
                    }
                    let probe = service
                        .validate_finance_rule_data(&data, false, false, session)
                        .await?;
                    let counterparty_must_be_eligible = probe.status.is_active()
                        || probe.operation != rule.operation
                        || probe.scope != rule.scope
                        || probe.counterparty_id != rule.counterparty_id;
                    let owner_must_be_eligible = probe.status.is_active()
                        || probe.owner_user_id != rule.owner_user_id
                        || probe.operation != rule.operation;
                    service
                        .validate_finance_rule_data(
                            &data,
                            counterparty_must_be_eligible,
                            owner_must_be_eligible,
                            session,
                        )
                        .await?;
                    rule.update(data, updated_by).map_err(Error::Logic)?;
                    db.finance_responsibility_rules()
                        .update(&mut rule, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<FinanceResponsibilityRule, crate::errors::Error>(rule)
                })
            })
            .await?;
        self.single_finance_responsibility_view(rule).await
    }

    /// 列出可作为付款或销项开票负责人的有效管理账号。
    ///
    /// # 错误
    /// 账号或权限数据读取失败时返回错误。
    pub async fn finance_responsibility_owner_options(
        &self,
    ) -> Result<Vec<FinanceResponsibilityOwnerOptionView>> {
        let payment = required_finance_permissions(FinanceResponsibilityOperation::SupplierPayment)?;
        let invoice = required_finance_permissions(FinanceResponsibilityOperation::SalesInvoice)?;
        let card_funds = required_finance_permissions(FinanceResponsibilityOperation::CardFundsReview)?;
        let accounts = self
            .db
            .accounts()
            .list_by_kind(AccountKind::Admin, &mut NoTransaction)
            .await?;
        let mut options = Vec::new();
        for account in accounts {
            if AvailableWorkItemAccount::from_account_kind(&account, AccountKind::Admin).is_err() {
                continue;
            }
            let granted = PermissionSet::new(
                self.rbac
                    .permissions(account.kind, account.base.id.as_str())
                    .await?,
            );
            let supplier_payment_eligible = granted.covers(&payment);
            let sales_invoice_eligible = granted.covers(&invoice);
            let card_funds_review_eligible = granted.covers(&card_funds);
            if !supplier_payment_eligible && !sales_invoice_eligible && !card_funds_review_eligible {
                continue;
            }
            options.push(FinanceResponsibilityOwnerOptionView {
                user_id: account.base.id,
                display_name: account.name,
                account: account.secret.account().to_string(),
                supplier_payment_eligible,
                sales_invoice_eligible,
                card_funds_review_eligible,
            });
        }
        options.sort_by(|left, right| {
            left.display_name
                .cmp(&right.display_name)
                .then_with(|| left.user_id.cmp(&right.user_id))
        });
        Ok(options)
    }

    async fn authorize_finance_owner_eligibility(
        &self,
        operation: FinanceResponsibilityOperation,
        owner_user_id: &str,
    ) -> Result<u64> {
        for _ in 0..AUTHORIZATION_SNAPSHOT_ATTEMPTS {
            let before = self.rbac.current_policy_revision().await?;
            self.ensure_finance_owner_eligible(operation, owner_user_id, &mut NoTransaction)
                .await?;
            let after = self.rbac.current_policy_revision().await?;
            if before == after {
                return Ok(before);
            }
        }
        Err(Error::Rbac(format!(
            "{}负责人授权策略持续变化，请重试",
            operation.label()
        )))
    }

    /// 在业务事务内按精确往来方、默认规则顺序解析并重验具体负责人。
    ///
    /// # 错误
    /// 往来方失效、规则零/多命中、账号不可用或权限不足时失败关闭。
    pub(crate) async fn resolve_finance_responsibility(
        &self,
        operation: FinanceResponsibilityOperation,
        counterparty_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<ResolvedFinanceResponsibility> {
        self.ensure_counterparty(operation, counterparty_id, executor)
            .await?;
        let rules = self
            .db
            .finance_responsibility_rules()
            .list_active_finance_responsibility_rules(operation, executor)
            .await?;
        let rule = FinanceResponsibilityRuleSet::new(&rules)
            .resolve(operation, counterparty_id)
            .map_err(|error| Error::BusinessLogicError(format!("{}，请先维护财务责任配置", error)))?;
        self.ensure_finance_owner_eligible(operation, &rule.owner_user_id, executor)
            .await?;
        Ok(ResolvedFinanceResponsibility {
            owner_user_id: rule.owner_user_id.clone(),
            responsibility_key: rule.work_item_responsibility_key(),
        })
    }

    async fn validate_finance_rule_data(
        &self,
        data: &FinanceResponsibilityRuleData,
        validate_counterparty: bool,
        validate_owner: bool,
        executor: &mut dyn Executor,
    ) -> Result<FinanceResponsibilityRule> {
        let probe =
            FinanceResponsibilityRule::new("validation", data.clone(), "validation").map_err(Error::Logic)?;
        if validate_counterparty {
            if let Some(counterparty_id) = probe.counterparty_id.as_deref() {
                self.ensure_counterparty(probe.operation, counterparty_id, executor)
                    .await?;
            }
        }
        if validate_owner {
            self.ensure_finance_owner_eligible(probe.operation, &probe.owner_user_id, executor)
                .await?;
        }
        Ok(probe)
    }

    async fn ensure_counterparty(
        &self,
        operation: FinanceResponsibilityOperation,
        counterparty_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        match operation {
            FinanceResponsibilityOperation::SupplierPayment => {
                let supplier = self
                    .db
                    .supplier_accounts()
                    .find_by_id(&SupplierAccountId::new(counterparty_id), executor)
                    .await?
                    .ok_or_else(|| Error::ValidationError("付款责任规则引用的供应商不存在".to_string()))?;
                if !supplier.is_active() {
                    return Err(Error::ValidationError(
                        "付款责任规则引用的供应商已停用，请重新选择".to_string(),
                    ));
                }
                Ok(supplier.supplier_no)
            }
            FinanceResponsibilityOperation::SalesInvoice
            | FinanceResponsibilityOperation::CardFundsReview => {
                let customer = self
                    .db
                    .customer_accounts()
                    .find_by_id(&CustomerAccountId::new(counterparty_id), executor)
                    .await?
                    .ok_or_else(|| Error::ValidationError("开票责任规则引用的客户不存在".to_string()))?;
                if !customer.is_active() {
                    return Err(Error::ValidationError(
                        "开票责任规则引用的客户已停用，请重新选择".to_string(),
                    ));
                }
                Ok(customer.customer_no)
            }
        }
    }

    async fn ensure_finance_owner_eligible(
        &self,
        operation: FinanceResponsibilityOperation,
        owner_user_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let account = self
            .db
            .accounts()
            .find_work_item_account(owner_user_id, executor)
            .await?
            .ok_or_else(|| {
                Error::BusinessLogicError(format!("{}负责人账号不存在，请重新选择", operation.label()))
            })?;
        AvailableWorkItemAccount::from_account_kind(&account, AccountKind::Admin).map_err(|_| {
            Error::BusinessLogicError(format!("{}负责人账号不可用，请重新选择", operation.label()))
        })?;
        let granted = PermissionSet::new(
            self.rbac
                .permissions(account.kind, account.base.id.as_str())
                .await?,
        );
        let required = required_finance_permissions(operation)?;
        if granted.covers(&required) {
            return Ok(());
        }
        Err(Error::BusinessLogicError(format!(
            "{}负责人缺少完整执行权限，请先调整财务角色或重新选择",
            operation.label()
        )))
    }

    async fn single_finance_responsibility_view(
        &self,
        rule: FinanceResponsibilityRule,
    ) -> Result<FinanceResponsibilityRuleView> {
        let mut views = self.finance_responsibility_views(vec![rule]).await?;
        views
            .pop()
            .ok_or_else(|| Error::Internal("财务责任规则视图未形成".to_string()))
    }

    async fn finance_responsibility_views(
        &self,
        rules: Vec<FinanceResponsibilityRule>,
    ) -> Result<Vec<FinanceResponsibilityRuleView>> {
        let owner_ids = rules
            .iter()
            .map(|rule| rule.owner_user_id.clone())
            .collect::<Vec<_>>();
        let owner_names = self
            .db
            .accounts()
            .list_work_item_party_accounts(&owner_ids, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|account| (account.base.id, account.name))
            .collect::<HashMap<_, _>>();
        let supplier_ids = rules
            .iter()
            .filter(|rule| rule.operation == FinanceResponsibilityOperation::SupplierPayment)
            .filter_map(|rule| rule.counterparty_id.as_deref())
            .map(SupplierAccountId::new)
            .collect::<Vec<_>>();
        let customer_ids = rules
            .iter()
            .filter(|rule| {
                matches!(
                    rule.operation,
                    FinanceResponsibilityOperation::SalesInvoice
                        | FinanceResponsibilityOperation::CardFundsReview
                )
            })
            .filter_map(|rule| rule.counterparty_id.as_deref())
            .map(CustomerAccountId::new)
            .collect::<Vec<_>>();
        let supplier_numbers = self
            .db
            .supplier_accounts()
            .supplier_numbers_by_ids(&supplier_ids, &mut NoTransaction)
            .await?;
        let customer_numbers = self
            .db
            .customer_accounts()
            .customer_numbers_by_ids(&customer_ids, &mut NoTransaction)
            .await?;
        let mut views = Vec::with_capacity(rules.len());
        for rule in rules {
            let counterparty_no =
                rule.counterparty_id
                    .as_ref()
                    .and_then(|counterparty_id| match rule.operation {
                        FinanceResponsibilityOperation::SupplierPayment => {
                            supplier_numbers.get(counterparty_id).cloned()
                        }
                        FinanceResponsibilityOperation::SalesInvoice
                        | FinanceResponsibilityOperation::CardFundsReview => {
                            customer_numbers.get(counterparty_id).cloned()
                        }
                    });
            let owner_name = owner_names.get(&rule.owner_user_id).cloned();
            let mut view = FinanceResponsibilityRuleView::from(rule);
            view.counterparty_no = counterparty_no;
            view.owner_name = owner_name;
            views.push(view);
        }
        Ok(views)
    }
}

fn required_finance_permissions(operation: FinanceResponsibilityOperation) -> Result<PermissionSet> {
    let codes = operation
        .required_permission_codes()
        .ok_or_else(|| Error::Internal("财务执行权限合同未注册".to_string()))?;
    let permissions = codes
        .iter()
        .map(|code| Permission::parse(code).map_err(|error| Error::Internal(error.to_string())))
        .collect::<Result<Vec<_>>>()?;
    Ok(PermissionSet::new(permissions))
}

#[cfg(test)]
mod tests {
    use super::required_finance_permissions;
    use entities::work_item::FinanceResponsibilityOperation;
    use entities::Permission;

    #[test]
    fn operation_permissions_cover_formal_execution_actions() {
        assert!(
            required_finance_permissions(FinanceResponsibilityOperation::SupplierPayment)
                .unwrap()
                .covers(&entities::PermissionSet::new(vec![Permission::parse(
                    "supplier_payment:commit"
                )
                .unwrap(),]))
        );
        assert!(
            required_finance_permissions(FinanceResponsibilityOperation::SalesInvoice)
                .unwrap()
                .covers(&entities::PermissionSet::new(vec![Permission::parse(
                    "invoice:post"
                )
                .unwrap(),]))
        );
        assert!(
            required_finance_permissions(FinanceResponsibilityOperation::CardFundsReview)
                .unwrap()
                .covers(&entities::PermissionSet::new(vec![Permission::parse(
                    "receivable_funds_review:complete"
                )
                .unwrap(),]))
        );
    }
}
