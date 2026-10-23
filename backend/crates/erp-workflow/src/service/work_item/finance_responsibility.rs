//! 财务责任规则管理、负责人候选与任务生产时的严格解析。

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_core::assignment::AssignmentStrategy;
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction};
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::WorkItemService;
use crate::entity::work_item::{
    AvailableWorkItemAccount, EnableStatus, FinanceResponsibilityOperation, FinanceResponsibilityRule,
    FinanceResponsibilityRuleData, FinanceResponsibilityRuleSet, FinanceResponsibilityScope, WorkItemType,
};
use crate::error::{Error, Result};
use crate::ports::PreparedWorkflowAudit;
use crate::repository::WorkItemExt;
use crate::repository::prelude::*;
use crate::repository::work_item::finance_rotation;

const AUTHORIZATION_SNAPSHOT_ATTEMPTS: usize = 3;

/// 创建财务责任规则请求。
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateFinanceResponsibilityRuleRequest {
    /// 供应商付款或销项开票；旧复核操作值仅用于历史数据解码。
    pub operation: FinanceResponsibilityOperation,
    /// 精确往来方或默认规则。
    pub scope: FinanceResponsibilityScope,
    /// 精确供应商或客户 ID。
    #[validate(length(max = 128, message = "往来方ID过长"))]
    pub counterparty_id: Option<String>,
    /// 按分配顺序保存的负责人池。
    #[validate(length(min = 1, max = 100, message = "负责人池人数必须在1-100之间"))]
    pub owner_user_ids: Vec<String>,
    /// 负责人池分配策略。
    pub strategy: AssignmentStrategy,
    /// 启停状态。
    pub status: EnableStatus,
}

impl CreateFinanceResponsibilityRuleRequest {
    fn into_data(self) -> FinanceResponsibilityRuleData {
        FinanceResponsibilityRuleData {
            operation: self.operation,
            scope: self.scope,
            counterparty_id: self.counterparty_id,
            owner_user_ids: self.owner_user_ids,
            strategy: self.strategy,
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
    /// 供应商付款或销项开票；旧复核操作值仅用于历史数据解码。
    pub operation: FinanceResponsibilityOperation,
    /// 精确往来方或默认规则。
    pub scope: FinanceResponsibilityScope,
    /// 精确供应商或客户 ID。
    #[validate(length(max = 128, message = "往来方ID过长"))]
    pub counterparty_id: Option<String>,
    /// 按分配顺序保存的负责人池。
    #[validate(length(min = 1, max = 100, message = "负责人池人数必须在1-100之间"))]
    pub owner_user_ids: Vec<String>,
    /// 负责人池分配策略。
    pub strategy: AssignmentStrategy,
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
                owner_user_ids: self.owner_user_ids,
                strategy: self.strategy,
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
    /// 供应商付款或销项开票；旧复核操作值仅用于历史数据解码。
    pub operation: FinanceResponsibilityOperation,
    /// 精确往来方或默认规则。
    pub scope: FinanceResponsibilityScope,
    /// 精确供应商或客户 ID。
    pub counterparty_id: Option<String>,
    /// 供应商编号或客户编号。
    pub counterparty_no: Option<String>,
    /// 按分配顺序保存的负责人池。
    pub owner_user_ids: Vec<String>,
    /// 负责人池分配策略。
    pub strategy: AssignmentStrategy,
    /// 与池成员同序的姓名；缺失账号保留空位。
    pub owner_names: Vec<Option<String>>,
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
            owner_user_ids: rule.owner_user_ids,
            strategy: rule.strategy,
            owner_names: Vec::new(),
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
}

/// 任务生产时冻结的财务责任解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct ResolvedFinanceResponsibility {
    /// 具体负责人账号 ID。
    pub owner_user_id: String,
    /// 写入工作项且后续不随规则更新变化的责任键。
    pub responsibility_key: String,
}

impl<A: crate::ports::WorkflowAuthorizationPort + Clone + Send + Sync + 'static> WorkItemService<A> {
    /// 查询全部财务责任规则。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回除卡资金复核以外的规则视图。
    ///
    /// # 错误
    /// 规则、账号或往来方读取失败时返回错误。
    pub async fn finance_responsibility_rule_list(self) -> Result<Vec<FinanceResponsibilityRuleView>> {
        let rules = self
            .db
            .finance_responsibility_rules()
            .list_finance_responsibility_rules(&mut NoTransaction)
            .await?;
        let rules = rules
            .into_iter()
            .filter(|rule| rule.operation != FinanceResponsibilityOperation::CardFundsReview)
            .collect();
        self.finance_responsibility_views(rules).await
    }

    /// 创建财务责任规则并记录审计。
    ///
    /// # 参数
    /// * `request` - 规则匹配范围与负责人
    /// * `actor` - 当前操作人
    ///
    /// # 返回
    /// 返回经编译器验证为 `Send` 的 future，完成后取得新规则视图。
    ///
    /// # 错误
    /// 匹配范围、往来方、负责人资格、唯一性或事务写入不满足时返回错误。
    #[expect(
        clippy::manual_async_fn,
        reason = "显式 Send 返回边界用于验证嵌套异步调用，避免调用方的高阶生命周期推导失败"
    )]
    pub fn create_finance_responsibility_rule(
        self,
        request: CreateFinanceResponsibilityRuleRequest,
        actor: AuditActor,
    ) -> impl Future<Output = Result<FinanceResponsibilityRuleView>> + Send {
        async move {
            let data = request.into_data();
            let probe = self.validate_finance_rule_data(&data, true, false, &mut NoTransaction).await?;
            let policy_revision = self.authorize_finance_pool(probe.operation, &probe.owner_user_ids).await?;
            let rule = FinanceResponsibilityRule::new(next_id(), data.clone(), actor.id())
                .map_err(|error| Error::ValidationError(error.to_string()))?;
            let audit = PreparedWorkflowAudit::resource(
                actor,
                "finance_responsibility_rule.create",
                "finance_responsibility_rule",
                rule.base.id.clone(),
            )?;
            let db = self.db.clone();
            let rbac = self.auth.clone();
            let facts = Arc::clone(&self.facts);
            let audit_port = Arc::clone(&self.audit);
            let rule = rbac
                .clone()
                .run_authorized_policy_transaction(policy_revision, move |executor| {
                    let service =
                        WorkItemService::with_ports(db.clone(), rbac, facts, Arc::clone(&audit_port));
                    Box::pin(async move {
                        service.validate_finance_rule_data(&data, true, true, executor).await?;
                        db.finance_responsibility_rules().create(&rule, executor).await?;
                        audit_port.persist(&audit, executor).await?;
                        Ok::<FinanceResponsibilityRule, crate::error::Error>(rule)
                    })
                })
                .await?;
            self.single_finance_responsibility_view(rule).await
        }
    }

    /// 整项更新财务责任规则并记录审计。
    ///
    /// # 参数
    /// * `id` - 规则 ID
    /// * `request` - 预期版本与完整规则配置
    /// * `actor` - 当前操作人
    ///
    /// # 返回
    /// 返回经编译器验证为 `Send` 的 future，完成后取得更新后的规则视图。
    ///
    /// # 错误
    /// 规则不存在、版本冲突、往来方或负责人资格不满足时返回错误。
    #[expect(
        clippy::manual_async_fn,
        reason = "显式 Send 返回边界用于验证嵌套异步调用，避免调用方的高阶生命周期推导失败"
    )]
    pub fn update_finance_responsibility_rule(
        self,
        id: String,
        request: UpdateFinanceResponsibilityRuleRequest,
        actor: AuditActor,
    ) -> impl Future<Output = Result<FinanceResponsibilityRuleView>> + Send {
        async move {
            let (version, data) = request.into_parts();
            let policy_revision = self.finance_rule_update_revision(&id, version, &data).await?;
            let updated_by = actor.id().to_string();
            let audit = PreparedWorkflowAudit::resource(
                actor,
                "finance_responsibility_rule.update",
                "finance_responsibility_rule",
                id.to_string(),
            )?;
            let db = self.db.clone();
            let rbac = self.auth.clone();
            let facts = Arc::clone(&self.facts);
            let audit_port = Arc::clone(&self.audit);
            let rule = rbac
                .clone()
                .run_authorized_policy_transaction(policy_revision, move |executor| {
                    let service = WorkItemService::with_ports(db, rbac, facts, Arc::clone(&audit_port));
                    Box::pin(async move {
                        let rule =
                            service.update_finance_rule(&id, version, data, updated_by, executor).await?;
                        audit_port.persist(&audit, executor).await?;
                        Ok::<FinanceResponsibilityRule, crate::error::Error>(rule)
                    })
                })
                .await?;
            self.single_finance_responsibility_view(rule).await
        }
    }

    /// 校验更新请求，并取得事务必须重验的授权策略版本。
    ///
    /// # 错误
    /// 规则不存在、版本冲突、资格校验或授权查询失败时返回错误。
    async fn finance_rule_update_revision(
        &self,
        id: &str,
        version: u64,
        data: &FinanceResponsibilityRuleData,
    ) -> Result<u64> {
        let probe = self.validate_finance_rule_data(data, false, false, &mut NoTransaction).await?;
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
            || probe.owner_user_ids != current.owner_user_ids
            || probe.operation != current.operation;
        self.validate_finance_rule_data(
            data,
            counterparty_must_be_eligible,
            owner_must_be_eligible,
            &mut NoTransaction,
        )
        .await?;
        if owner_must_be_eligible {
            self.authorize_finance_pool(probe.operation, &probe.owner_user_ids).await
        } else {
            self.auth.current_policy_revision().await
        }
    }

    /// 在调用方事务内重验版本与资格，并更新财务责任规则。
    ///
    /// # 错误
    /// 规则不存在、版本冲突、资格校验或仓储更新失败时返回错误。
    async fn update_finance_rule(
        &self,
        id: &str,
        version: u64,
        data: FinanceResponsibilityRuleData,
        updated_by: String,
        executor: &mut dyn Executor,
    ) -> Result<FinanceResponsibilityRule> {
        let mut rule = self
            .db
            .finance_responsibility_rules()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("财务责任规则不存在".to_string()))?;
        if rule.base.version != version {
            return Err(Error::ConflictError("财务责任规则版本已变化".to_string()));
        }
        let probe = self.validate_finance_rule_data(&data, false, false, executor).await?;
        let counterparty_must_be_eligible = probe.status.is_active()
            || probe.operation != rule.operation
            || probe.scope != rule.scope
            || probe.counterparty_id != rule.counterparty_id;
        let owner_must_be_eligible = probe.status.is_active()
            || probe.owner_user_ids != rule.owner_user_ids
            || probe.operation != rule.operation;
        self.validate_finance_rule_data(
            &data,
            counterparty_must_be_eligible,
            owner_must_be_eligible,
            executor,
        )
        .await?;
        let current = finance_rotation::rotation(&self.db, id, executor).await?;
        finance_rotation::advance(&self.db, id, &current, current.last_owner_user_id.as_deref(), executor)
            .await?;
        rule.update(data, updated_by).map_err(|error| Error::ValidationError(error.to_string()))?;
        self.db.finance_responsibility_rules().update(&mut rule, executor).await?;
        Ok(rule)
    }

    /// 列出可作为付款或销项开票负责人的有效管理账号。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回至少具备付款或销项开票所需权限的有效管理账号，按显示名与账号 ID 排序。
    ///
    /// # 错误
    /// 账号或权限数据读取失败时返回错误。
    pub async fn finance_responsibility_owner_options(
        self,
    ) -> Result<Vec<FinanceResponsibilityOwnerOptionView>> {
        let payment = required_finance_permissions(FinanceResponsibilityOperation::SupplierPayment)?;
        let invoice = required_finance_permissions(FinanceResponsibilityOperation::SalesInvoice)?;
        let accounts = self.auth.list_accounts_by_kind(AccountKind::Admin, &mut NoTransaction).await?;
        let mut options = Vec::new();
        for account in accounts {
            if AvailableWorkItemAccount::from_account_kind(&account, AccountKind::Admin).is_err() {
                continue;
            }
            let granted = self.auth.permission_codes(account.kind, account.id.as_str()).await?;
            let covers = |required: &[String]| {
                required
                    .iter()
                    .all(|code| granted.iter().any(|owned| crate::ports::permission_covers(owned, code)))
            };
            let supplier_payment_eligible = covers(&payment);
            let sales_invoice_eligible = covers(&invoice);
            if !supplier_payment_eligible && !sales_invoice_eligible {
                continue;
            }
            options.push(FinanceResponsibilityOwnerOptionView {
                user_id: account.id,
                display_name: account.display_name,
                account: account.login_account,
                supplier_payment_eligible,
                sales_invoice_eligible,
            });
        }
        options.sort_by(|left, right| {
            left.display_name.cmp(&right.display_name).then_with(|| left.user_id.cmp(&right.user_id))
        });
        Ok(options)
    }

    async fn authorize_finance_pool(
        &self,
        operation: FinanceResponsibilityOperation,
        owner_user_ids: &[String],
    ) -> Result<u64> {
        for _ in 0..AUTHORIZATION_SNAPSHOT_ATTEMPTS {
            let before = self.auth.current_policy_revision().await?;
            for owner in owner_user_ids {
                self.ensure_finance_owner_eligible(operation, owner.clone()).await?;
            }
            let after = self.auth.current_policy_revision().await?;
            if before == after {
                return Ok(before);
            }
        }
        Err(Error::Rbac(format!("{}负责人授权策略持续变化，请重试", operation.label())))
    }

    /// 在任务创建事务内匹配负责人池、重验选中人员并推进一次轮次。
    ///
    /// # 参数
    /// * `operation` - 财务责任操作
    /// * `counterparty_id` - 往来方 ID
    /// * `executor` - 业务事务执行器
    ///
    /// # 返回
    /// 返回冻结的负责人账号与责任键；后续失败由调用方回滚原事务。
    ///
    /// # 错误
    /// 往来方失效、规则零/多命中、账号不可用或权限不足时失败关闭。
    pub async fn resolve_finance_responsibility(
        &self,
        operation: FinanceResponsibilityOperation,
        counterparty_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<ResolvedFinanceResponsibility> {
        self.ensure_counterparty(operation, counterparty_id, executor).await?;
        let rules = self
            .db
            .finance_responsibility_rules()
            .list_active_finance_responsibility_rules(operation, executor)
            .await?;
        let rule = FinanceResponsibilityRuleSet::new(&rules)
            .resolve(operation, counterparty_id)
            .map_err(|error| Error::BusinessLogicError(format!("{}，请先维护财务责任配置", error)))?;
        let current = finance_rotation::rotation(&self.db, &rule.base.id, executor).await?;
        let owner = rule
            .strategy
            .select(&rule.owner_user_ids, current.last_owner_user_id.as_deref())
            .map_err(|error| Error::ValidationError(error.to_string()))?
            .to_owned();
        self.ensure_finance_owner_eligible(operation, owner.clone()).await?;
        finance_rotation::advance(&self.db, &rule.base.id, &current, Some(&owner), executor).await?;
        Ok(ResolvedFinanceResponsibility {
            owner_user_id: owner,
            responsibility_key: rule.work_item_responsibility_key(),
        })
    }

    /// 沿用供应商付款或来源客户开票责任，重验退货财务完整权限。
    /// # 参数
    /// * `task_type` - 退款或进项红票任务。
    /// * `counterparty_id` - 退款用供应商，红票用来源客户。
    /// * `executor` - 原业务事务。
    /// # 返回
    /// 已冻结且可执行本任务的个人责任。
    /// # 错误
    /// 规则缺失、账号失效或缺少退货执行权限时拒绝。
    pub async fn resolve_return_finance_responsibility(
        &self,
        task_type: WorkItemType,
        counterparty_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<ResolvedFinanceResponsibility> {
        let operation = match task_type {
            WorkItemType::PurchaseReturnRefund => FinanceResponsibilityOperation::SupplierPayment,
            WorkItemType::PurchaseReturnInvoice => FinanceResponsibilityOperation::SalesInvoice,
            _ => return Err(Error::ValidationError("不是退货财务任务".into())),
        };
        let resolved = self.resolve_finance_responsibility(operation, counterparty_id, executor).await?;
        let required = task_type
            .required_execution_permissions("purchase_order")
            .ok_or_else(|| Error::BusinessLogicError("退货财务权限未登记".into()))?;
        let granted = self.auth.permission_codes(AccountKind::Admin, &resolved.owner_user_id).await?;
        if !required
            .iter()
            .all(|code| granted.iter().any(|owned| crate::ports::permission_covers(owned, code)))
        {
            return Err(Error::BusinessLogicError(format!(
                "{}负责人缺少退货处理权限，请调整财务责任配置",
                operation.label()
            )));
        }
        Ok(resolved)
    }

    async fn validate_finance_rule_data(
        &self,
        data: &FinanceResponsibilityRuleData,
        validate_counterparty: bool,
        validate_owner: bool,
        executor: &mut dyn Executor,
    ) -> Result<FinanceResponsibilityRule> {
        let probe = FinanceResponsibilityRule::new("validation", data.clone(), "validation")
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        if validate_counterparty && let Some(counterparty_id) = probe.counterparty_id.as_deref() {
            self.ensure_counterparty(probe.operation, counterparty_id, executor).await?;
        }
        if validate_owner {
            for owner in &probe.owner_user_ids {
                self.ensure_finance_owner_eligible(probe.operation, owner.clone()).await?;
            }
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
                if !self.facts.counterparty_is_active("supplier", counterparty_id, executor).await? {
                    return Err(Error::ValidationError(
                        "付款责任规则引用的供应商不存在或已停用，请重新选择".to_string(),
                    ));
                }
                Ok(counterparty_id.to_string())
            },
            FinanceResponsibilityOperation::SalesInvoice
            | FinanceResponsibilityOperation::CardFundsReview => {
                if !self.facts.counterparty_is_active("customer", counterparty_id, executor).await? {
                    return Err(Error::ValidationError(
                        "开票责任规则引用的客户不存在或已停用，请重新选择".to_string(),
                    ));
                }
                Ok(counterparty_id.to_string())
            },
        }
    }

    fn ensure_finance_owner_eligible(
        &self,
        operation: FinanceResponsibilityOperation,
        owner_user_id: String,
    ) -> impl Future<Output = Result<()>> + Send + 'static {
        let auth = self.auth.clone();
        async move {
            let account = auth.load_account(&owner_user_id, &mut NoTransaction).await?.ok_or_else(|| {
                Error::BusinessLogicError(format!("{}负责人账号不存在，请重新选择", operation.label()))
            })?;
            AvailableWorkItemAccount::from_account_kind(&account, AccountKind::Admin).map_err(|_| {
                Error::BusinessLogicError(format!("{}负责人账号不可用，请重新选择", operation.label()))
            })?;
            let granted = auth.permission_codes(account.kind, account.id.as_str()).await?;
            let required = required_finance_permissions(operation)?;
            if required
                .iter()
                .all(|code| granted.iter().any(|owned| crate::ports::permission_covers(owned, code)))
            {
                return Ok(());
            }
            Err(Error::BusinessLogicError(format!(
                "{}负责人缺少完整执行权限，请先调整财务角色或重新选择",
                operation.label()
            )))
        }
    }

    async fn single_finance_responsibility_view(
        &self,
        rule: FinanceResponsibilityRule,
    ) -> Result<FinanceResponsibilityRuleView> {
        let mut views = self.finance_responsibility_views(vec![rule]).await?;
        views.pop().ok_or_else(|| Error::Internal("财务责任规则视图未形成".to_string()))
    }

    async fn finance_responsibility_views(
        &self,
        rules: Vec<FinanceResponsibilityRule>,
    ) -> Result<Vec<FinanceResponsibilityRuleView>> {
        let owner_ids = rules.iter().flat_map(|rule| rule.owner_user_ids.iter().cloned()).collect::<Vec<_>>();
        let owner_names = self
            .auth
            .load_accounts(&owner_ids, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|account| (account.id, account.display_name))
            .collect::<HashMap<_, _>>();
        let supplier_ids = rules
            .iter()
            .filter(|rule| rule.operation == FinanceResponsibilityOperation::SupplierPayment)
            .filter_map(|rule| rule.counterparty_id.clone())
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
            .filter_map(|rule| rule.counterparty_id.clone())
            .collect::<Vec<_>>();
        let supplier_numbers =
            self.facts.counterparty_numbers("supplier", &supplier_ids, &mut NoTransaction).await?;
        let customer_numbers =
            self.facts.counterparty_numbers("customer", &customer_ids, &mut NoTransaction).await?;
        let mut views = Vec::with_capacity(rules.len());
        for rule in rules {
            let counterparty_no =
                rule.counterparty_id.as_ref().and_then(|counterparty_id| match rule.operation {
                    FinanceResponsibilityOperation::SupplierPayment => {
                        supplier_numbers.get(counterparty_id).cloned()
                    },
                    FinanceResponsibilityOperation::SalesInvoice
                    | FinanceResponsibilityOperation::CardFundsReview => {
                        customer_numbers.get(counterparty_id).cloned()
                    },
                });
            let names = rule.owner_user_ids.iter().map(|id| owner_names.get(id).cloned()).collect();
            let mut view = FinanceResponsibilityRuleView::from(rule);
            view.counterparty_no = counterparty_no;
            view.owner_names = names;
            views.push(view);
        }
        Ok(views)
    }
}

fn required_finance_permissions(operation: FinanceResponsibilityOperation) -> Result<Vec<String>> {
    let codes = operation
        .required_permission_codes()
        .ok_or_else(|| Error::Internal("财务执行权限合同未注册".to_string()))?;
    Ok(codes.iter().map(|code| (*code).to_string()).collect())
}

#[cfg(test)]
mod tests {
    use super::required_finance_permissions;
    use crate::entity::work_item::FinanceResponsibilityOperation;

    #[test]
    fn operation_permissions_cover_formal_execution_actions() {
        assert!(
            required_finance_permissions(FinanceResponsibilityOperation::SupplierPayment)
                .unwrap()
                .iter()
                .any(|code| crate::ports::permission_covers(code, "purchase_payment_request:pay"))
        );
        assert!(
            required_finance_permissions(FinanceResponsibilityOperation::SalesInvoice)
                .unwrap()
                .iter()
                .any(|code| crate::ports::permission_covers(code, "invoice:post"))
        );
        assert!(required_finance_permissions(FinanceResponsibilityOperation::CardFundsReview).is_err());
    }
}
