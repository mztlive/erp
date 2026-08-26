//! 财务执行任务的具体负责人规则。
//!
//! 付款按供应商、销项开票按客户匹配；精确往来方优先于同业务的默认规则。
//! 规则只决定新任务的初始负责人，已经形成的工作项责任事实不随规则更新漂移。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::catalog::EnableStatus;
use crate::errors::{Error, Result};
use crate::validation::{normalize_optional_text, normalize_required_text};

const ID_MAX_LEN: usize = 128;
const COUNTERPARTY_ID_MAX_LEN: usize = 128;
const OWNER_ID_MAX_LEN: usize = 128;
const ACTOR_ID_MAX_LEN: usize = 128;

/// 财务责任规则覆盖的业务操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FinanceResponsibilityOperation {
    /// 供应商付款执行。
    SupplierPayment,
    /// 客户销项开票执行。
    SalesInvoice,
}

impl FinanceResponsibilityOperation {
    /// 返回稳定持久化代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SupplierPayment => "SUPPLIER_PAYMENT",
            Self::SalesInvoice => "SALES_INVOICE",
        }
    }

    /// 返回面向业务人员的操作名称。
    pub fn label(self) -> &'static str {
        match self {
            Self::SupplierPayment => "供应商付款",
            Self::SalesInvoice => "销项开票",
        }
    }

    /// 返回本操作精确规则使用的往来方类型名称。
    pub fn counterparty_label(self) -> &'static str {
        match self {
            Self::SupplierPayment => "供应商",
            Self::SalesInvoice => "客户",
        }
    }
}

/// 财务责任规则的匹配层级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FinanceResponsibilityScope {
    /// 精确往来方规则。
    Counterparty,
    /// 本业务操作唯一默认规则。
    Default,
}

impl FinanceResponsibilityScope {
    /// 返回稳定持久化代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Counterparty => "COUNTERPARTY",
            Self::Default => "DEFAULT",
        }
    }
}

/// 财务责任规则创建或整项更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FinanceResponsibilityRuleData {
    /// 付款或销项开票。
    pub operation: FinanceResponsibilityOperation,
    /// 精确往来方或默认规则。
    pub scope: FinanceResponsibilityScope,
    /// 精确规则对应的供应商或客户 ID；默认规则必须为空。
    pub counterparty_id: Option<String>,
    /// 具体财务负责人账号 ID。
    pub owner_user_id: String,
    /// 启停状态。
    pub status: EnableStatus,
}

/// 可维护的财务责任规则。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct FinanceResponsibilityRule {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 付款或销项开票。
    pub operation: FinanceResponsibilityOperation,
    /// 精确往来方或默认规则。
    pub scope: FinanceResponsibilityScope,
    /// 精确规则对应的供应商或客户 ID。
    pub counterparty_id: Option<String>,
    /// 具体财务负责人账号 ID。
    pub owner_user_id: String,
    /// 启停状态。
    pub status: EnableStatus,
    /// 由业务操作和匹配范围确定性形成的唯一键。
    pub selector_key: String,
    /// 创建人。
    pub created_by: String,
    /// 最近更新人。
    pub updated_by: String,
}

impl FinanceResponsibilityRule {
    /// 创建财务责任规则。
    ///
    /// # 错误
    /// 主键、操作人、负责人或匹配范围不合法时返回错误。
    pub fn new(
        id: impl Into<String>,
        data: FinanceResponsibilityRuleData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let id = normalize_required_text(
            id.into(),
            "财务责任规则ID不能为空",
            ID_MAX_LEN,
            "财务责任规则ID过长",
        )?;
        let created_by = normalize_actor(created_by.into())?;
        let normalized = NormalizedRuleData::try_from(data)?;
        Ok(Self {
            base: BaseModel::new(id),
            operation: normalized.operation,
            scope: normalized.scope,
            counterparty_id: normalized.counterparty_id,
            owner_user_id: normalized.owner_user_id,
            status: normalized.status,
            selector_key: normalized.selector_key,
            updated_by: created_by.clone(),
            created_by,
        })
    }

    /// 整项更新财务责任规则。
    ///
    /// # 错误
    /// 操作人、负责人或匹配范围不合法时返回错误。
    pub fn update(
        &mut self,
        data: FinanceResponsibilityRuleData,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        let updated_by = normalize_actor(updated_by.into())?;
        let normalized = NormalizedRuleData::try_from(data)?;
        self.operation = normalized.operation;
        self.scope = normalized.scope;
        self.counterparty_id = normalized.counterparty_id;
        self.owner_user_id = normalized.owner_user_id;
        self.status = normalized.status;
        self.selector_key = normalized.selector_key;
        self.updated_by = updated_by;
        Ok(())
    }

    /// 判断规则当前是否参与责任解析。
    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }

    /// 返回写入工作项的规则责任键。
    pub fn work_item_responsibility_key(&self) -> String {
        format!("finance:{}:{}", self.operation.as_str(), self.base.id)
    }
}

/// 当前启用规则集；负责执行精确优先、默认兜底和零/多命中失败关闭。
pub struct FinanceResponsibilityRuleSet<'a> {
    rules: &'a [FinanceResponsibilityRule],
}

impl<'a> FinanceResponsibilityRuleSet<'a> {
    /// 创建只读规则集。
    pub fn new(rules: &'a [FinanceResponsibilityRule]) -> Self {
        Self { rules }
    }

    /// 按业务操作和精确往来方解析唯一负责人规则。
    ///
    /// # 错误
    /// 精确层或默认层存在多条启用规则，或两个层级均未配置时返回错误。
    pub fn resolve(
        &self,
        operation: FinanceResponsibilityOperation,
        counterparty_id: &str,
    ) -> Result<&'a FinanceResponsibilityRule> {
        let counterparty_id = normalize_required_text(
            counterparty_id.to_string(),
            "财务责任往来方不能为空",
            COUNTERPARTY_ID_MAX_LEN,
            "财务责任往来方过长",
        )?;
        if let Some(rule) =
            self.unique_match(operation, FinanceResponsibilityScope::Counterparty, |rule| {
                rule.counterparty_id.as_deref() == Some(counterparty_id.as_str())
            })?
        {
            return Ok(rule);
        }
        self.unique_match(operation, FinanceResponsibilityScope::Default, |_| true)?
            .ok_or_else(|| {
                Error::from(format!(
                    "{}未配置{}责任人，也未配置默认负责人",
                    operation.counterparty_label(),
                    operation.label()
                ))
            })
    }

    fn unique_match(
        &self,
        operation: FinanceResponsibilityOperation,
        scope: FinanceResponsibilityScope,
        predicate: impl Fn(&FinanceResponsibilityRule) -> bool,
    ) -> Result<Option<&'a FinanceResponsibilityRule>> {
        let mut matched = self.rules.iter().filter(|rule| {
            rule.is_active() && rule.operation == operation && rule.scope == scope && predicate(rule)
        });
        let first = matched.next();
        if matched.next().is_some() {
            return Err(Error::from(format!(
                "{}存在重复启用责任规则，请先停用冲突规则",
                operation.label()
            )));
        }
        Ok(first)
    }
}

struct NormalizedRuleData {
    operation: FinanceResponsibilityOperation,
    scope: FinanceResponsibilityScope,
    counterparty_id: Option<String>,
    owner_user_id: String,
    status: EnableStatus,
    selector_key: String,
}

impl TryFrom<FinanceResponsibilityRuleData> for NormalizedRuleData {
    type Error = Error;

    fn try_from(data: FinanceResponsibilityRuleData) -> Result<Self> {
        let counterparty_id =
            normalize_optional_text(data.counterparty_id, "财务责任往来方", COUNTERPARTY_ID_MAX_LEN)?;
        match (data.scope, counterparty_id.as_ref()) {
            (FinanceResponsibilityScope::Counterparty, None) => {
                return Err(Error::from(format!(
                    "精确{}责任规则必须选择{}",
                    data.operation.label(),
                    data.operation.counterparty_label()
                )));
            }
            (FinanceResponsibilityScope::Default, Some(_)) => {
                return Err(Error::from("默认财务责任规则不得指定往来方"));
            }
            _ => {}
        }
        let owner_user_id = normalize_required_text(
            data.owner_user_id,
            "财务负责人不能为空",
            OWNER_ID_MAX_LEN,
            "财务负责人过长",
        )?;
        let selector_key = selector_key(data.operation, data.scope, counterparty_id.as_deref());
        Ok(Self {
            operation: data.operation,
            scope: data.scope,
            counterparty_id,
            owner_user_id,
            status: data.status,
            selector_key,
        })
    }
}

fn selector_key(
    operation: FinanceResponsibilityOperation,
    scope: FinanceResponsibilityScope,
    counterparty_id: Option<&str>,
) -> String {
    match scope {
        FinanceResponsibilityScope::Counterparty => format!(
            "{}:{}:{}",
            operation.as_str(),
            scope.as_str(),
            counterparty_id.expect("精确规则形状已校验")
        ),
        FinanceResponsibilityScope::Default => {
            format!("{}:{}", operation.as_str(), scope.as_str())
        }
    }
}

fn normalize_actor(value: String) -> Result<String> {
    normalize_required_text(
        value,
        "财务责任规则操作人不能为空",
        ACTOR_ID_MAX_LEN,
        "财务责任规则操作人过长",
    )
}

#[cfg(test)]
mod tests {
    use super::{
        FinanceResponsibilityOperation, FinanceResponsibilityRule, FinanceResponsibilityRuleData,
        FinanceResponsibilityRuleSet, FinanceResponsibilityScope,
    };
    use crate::catalog::EnableStatus;

    fn rule(
        id: &str,
        operation: FinanceResponsibilityOperation,
        scope: FinanceResponsibilityScope,
        counterparty_id: Option<&str>,
        owner: &str,
    ) -> FinanceResponsibilityRule {
        FinanceResponsibilityRule::new(
            id,
            FinanceResponsibilityRuleData {
                operation,
                scope,
                counterparty_id: counterparty_id.map(str::to_string),
                owner_user_id: owner.to_string(),
                status: EnableStatus::Active,
            },
            "admin-1",
        )
        .unwrap()
    }

    #[test]
    fn selector_shape_is_strict_and_key_is_deterministic() {
        let exact = rule(
            "r-1",
            FinanceResponsibilityOperation::SupplierPayment,
            FinanceResponsibilityScope::Counterparty,
            Some("supplier-1"),
            "finance-1",
        );
        assert_eq!(exact.selector_key, "SUPPLIER_PAYMENT:COUNTERPARTY:supplier-1");
        assert!(FinanceResponsibilityRule::new(
            "r-2",
            FinanceResponsibilityRuleData {
                operation: FinanceResponsibilityOperation::SalesInvoice,
                scope: FinanceResponsibilityScope::Default,
                counterparty_id: Some("customer-1".to_string()),
                owner_user_id: "finance-1".to_string(),
                status: EnableStatus::Active,
            },
            "admin-1",
        )
        .is_err());
    }

    #[test]
    fn exact_rule_wins_and_missing_default_fails_closed() {
        let rules = vec![
            rule(
                "default",
                FinanceResponsibilityOperation::SalesInvoice,
                FinanceResponsibilityScope::Default,
                None,
                "finance-default",
            ),
            rule(
                "exact",
                FinanceResponsibilityOperation::SalesInvoice,
                FinanceResponsibilityScope::Counterparty,
                Some("customer-1"),
                "finance-exact",
            ),
        ];
        let set = FinanceResponsibilityRuleSet::new(&rules);
        assert_eq!(
            set.resolve(FinanceResponsibilityOperation::SalesInvoice, "customer-1")
                .unwrap()
                .owner_user_id,
            "finance-exact"
        );
        assert_eq!(
            set.resolve(FinanceResponsibilityOperation::SalesInvoice, "customer-2")
                .unwrap()
                .owner_user_id,
            "finance-default"
        );
        assert!(FinanceResponsibilityRuleSet::new(&[])
            .resolve(FinanceResponsibilityOperation::SupplierPayment, "supplier-1")
            .is_err());
    }
}
