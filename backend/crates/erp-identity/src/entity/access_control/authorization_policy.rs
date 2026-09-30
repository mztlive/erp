//! 配置准入与运行消费分离；任务、来源和角色授权不生成独立人员范围。
use serde::Serialize;

use crate::{Error, Result};

/// 资源动作的权限边界；不替代拥有领域对具体对象的授权判断。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationPolicy {
    Business,
    SourceInherited,
    Task,
    Governance,
    Directory,
    Role,
}

impl AuthorizationPolicy {
    /// 取得已知业务动作采用的授权方式。
    /// # 参数
    /// `resource` 与 `action` 为已经由消费者目录校验的资源动作。
    /// # 返回
    /// 明确登记的权限策略；未登记资源失败关闭。
    /// # 错误
    /// 未知资源返回校验错误。
    pub fn for_action(resource: &str, action: &str) -> Result<Self> {
        let policy = match (resource, action) {
            ("approval_instance", _)
            | ("supplier_settlement_statement", "confirm")
            | ("reconciliation_difference", "decide") => Self::Task,
            ("integration_error_task" | "reconciliation_difference", "create") => Self::Role,
            (
                "contract"
                | "sales_selection_proposal"
                | "customer_refund"
                | "supplier_refund"
                | "cost_entry"
                | "cost_allocation"
                | "receivable_account"
                | "customer_receipt"
                | "invoice"
                | "sales_invoice_request"
                | "payable_account"
                | "supplier_payment"
                | "purchase_invoice_allocation",
                _,
            ) => Self::SourceInherited,
            ("work_item" | "org_unit" | "person_query_qualification", _) => Self::Governance,
            (
                "settlement_party" | "warehouse" | "business_person" | "sales_person" | "procurement_person",
                _,
            ) => Self::Directory,
            (
                "customer"
                | "sales_order"
                | "purchase_order"
                | "sales_selection_booklet"
                | "supplier"
                | "product"
                | "supplier_offering"
                | "supplier_fulfillment_order"
                | "stock_adjustment"
                | "stock_balance"
                | "stock_movement"
                | "stock_reservation"
                | "supplier_settlement_statement"
                | "integration_error_task"
                | "reconciliation_difference",
                _,
            ) => Self::Business,
            _ => return Err(Error::ValidationError(format!("{resource} 未登记授权策略"))),
        };
        Ok(policy)
    }

    /// 独立子资源的范围来源；动作权限仍由子资源证明。
    /// # 参数
    /// `resource` 与 `action` 为请求资源动作。
    /// # 返回
    /// 来源资源动作，或无需映射时保持原值。
    /// # 错误
    /// 无；消费者登记由解析入口验证。
    pub fn scope_source<'a>(resource: &'a str, action: &'a str) -> (&'a str, &'a str) {
        match resource {
            "contract" => ("customer", "list"),
            "sales_selection_proposal" => ("sales_selection_booklet", action),
            _ => (resource, action),
        }
    }

    /// 当前策略是否接受独立人员范围配置。
    /// # 参数
    /// 无。
    /// # 返回
    /// 业务责任、治理委派和目录可见范围为 true。
    /// # 错误
    /// 无。
    pub fn configurable(self) -> bool {
        matches!(self, Self::Business | Self::Governance | Self::Directory)
    }

    /// 返回可直接展示的配置边界说明。
    /// # 参数
    /// 无。
    /// # 返回
    /// 静态说明；不表示具体对象已授权。
    /// # 错误
    /// 无。
    pub fn description(self) -> &'static str {
        match self {
            Self::Business => "按业务责任和附加范围授权；仍需具备对应操作权限。",
            Self::SourceInherited => {
                "继承来源业务的可见范围；来源、财务职责或关联任务由业务核验，不单独配置范围。"
            },
            Self::Task => "由流程任务、参与关系及管理资格核验；不单独配置数据范围。",
            Self::Governance => "配置管理委派边界；部门关系和业务数据权限不能替代管理授权。",
            Self::Directory => "仅配置目录候选的可见范围；不授予关联业务的数据或操作权限。",
            Self::Role => "由操作权限及业务责任校验授权；此动作没有独立数据范围配置。",
        }
    }

    /// 补充资源采用的真实来源及配置边界。
    /// # 参数
    /// `resource` 为已登记资源。
    /// # 返回
    /// 资源专用说明，其他资源返回策略通用说明。
    /// # 错误
    /// 无；不产生具体对象的授权结论。
    pub fn description_for(self, resource: &str) -> &'static str {
        match resource {
            "stock_balance" | "stock_movement" | "stock_reservation" => {
                "采用库存仓库范围政策；余额、流水、预占及各操作的已有范围分别保留，不自动合并，也不使用仓库目录的可见范围。"
            },
            "contract" => "继承客户的业务范围，仍需具备合同操作权限；不单独配置合同范围。",
            "sales_selection_proposal" => {
                "继承当前所属选品册的责任范围，仍需具备选品方案操作权限；不单独配置方案范围。"
            },
            _ => self.description(),
        }
    }

    /// 拒绝为任务、继承或角色策略保存独立范围。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前策略允许独立配置时成功。
    /// # 错误
    /// 不可配置策略返回校验错误；停用配置也不能绕过。
    pub fn ensure_configurable(self) -> Result<()> {
        if self.configurable() {
            return Ok(());
        }
        Err(Error::ValidationError(format!("此操作不接受独立数据范围：{}", self.description())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inherited_and_task_configuration_is_rejected() {
        for (resource, action) in [
            ("approval_instance", "decide"),
            ("contract", "list"),
            ("supplier_settlement_statement", "confirm"),
            ("reconciliation_difference", "decide"),
            ("integration_error_task", "create"),
        ] {
            assert!(
                AuthorizationPolicy::for_action(resource, action).unwrap().ensure_configurable().is_err()
            );
        }
        for (resource, action) in [
            ("customer", "list"),
            ("org_unit", "manage"),
            ("warehouse", "list"),
            ("supplier_settlement_statement", "submit"),
            ("reconciliation_difference", "detail"),
        ] {
            assert!(AuthorizationPolicy::for_action(resource, action).unwrap().ensure_configurable().is_ok());
        }
        assert!(AuthorizationPolicy::for_action("unknown", "list").is_err());
    }
}
