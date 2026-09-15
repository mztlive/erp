//! 域 D03 `work_item` 责任事实仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as WorkItemExt>::WORK_ITEMS`。

use mongodb::Database;

use super::super::work_item::WorkItemFilter;
use crate::repository::owned::{FinanceResponsibilityRuleRepository, WorkItemRepository};

/// 域 D03 仓储访问器。
pub trait WorkItemExt {
    /// `work_item` 集合名。
    const WORK_ITEMS: &'static str = "work_items";
    /// 财务执行负责人规则集合名。
    const FINANCE_RESPONSIBILITY_RULES: &'static str = "finance_responsibility_rules";

    /// 责任队列筛选条件类型（定义见 `repository::work_item`）。
    type WorkItemFilter;

    /// 获取 `work_item` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `WorkItemRepository<'_>`。
    fn work_items(&self) -> WorkItemRepository<'_>;

    /// 获取财务执行负责人规则 Repository。
    ///
    /// # 返回
    /// 返回 `FinanceResponsibilityRuleRepository<'_>`。
    fn finance_responsibility_rules(&self) -> FinanceResponsibilityRuleRepository<'_>;
}

impl WorkItemExt for Database {
    type WorkItemFilter = WorkItemFilter;

    fn work_items(&self) -> WorkItemRepository<'_> {
        WorkItemRepository::new(self, Self::WORK_ITEMS)
    }

    fn finance_responsibility_rules(&self) -> FinanceResponsibilityRuleRepository<'_> {
        FinanceResponsibilityRuleRepository::new(self, Self::FINANCE_RESPONSIBILITY_RULES)
    }
}
