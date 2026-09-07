//! 采购责任规则仓储访问器。

use crate::repository::owned::ProcurementResponsibilityRuleRepository;

use mongodb::Database;

use super::super::procurement_responsibility::ProcurementResponsibilityRuleFilter;

/// 采购责任规则仓储访问入口。
pub trait ProcurementResponsibilityExt {
    /// 采购责任规则集合名。
    const PROCUREMENT_RESPONSIBILITY_RULES: &'static str = "procurement_responsibility_rules";

    /// 规则列表筛选条件类型。
    type ProcurementResponsibilityRuleFilter;

    /// 获取采购责任规则 Repository。
    ///
    /// # 返回
    /// 返回绑定 `procurement_responsibility_rules` 集合的 Repository。
    fn procurement_responsibility_rules(&self) -> ProcurementResponsibilityRuleRepository<'_>;
}

impl ProcurementResponsibilityExt for Database {
    type ProcurementResponsibilityRuleFilter = ProcurementResponsibilityRuleFilter;

    /// 获取采购责任规则 Repository。
    ///
    /// # 返回
    /// 返回绑定 `procurement_responsibility_rules` 集合的 Repository。
    fn procurement_responsibility_rules(&self) -> ProcurementResponsibilityRuleRepository<'_> {
        ProcurementResponsibilityRuleRepository::new(self, Self::PROCUREMENT_RESPONSIBILITY_RULES)
    }
}
