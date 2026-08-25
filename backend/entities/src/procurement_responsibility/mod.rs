//! 采购责任规则及确定性优先级解析。
//!
//! 规则只允许指向具体后台账号；账号状态与 RBAC 权限属于跨聚合事实，由 Service
//! 在维护和解析时校验。本模块负责选择器形状、文本规范化与同层唯一命中规则。

mod resolution;
mod rule;

pub use resolution::{
    EligibleProcurementOwner, ProcurementResponsibilityContext, ProcurementResponsibilityResolutionBatch,
    ProcurementResponsibilityResolutionIdentity, ProcurementResponsibilityResolutionLine,
    ProcurementResponsibilityRuleSet,
};
pub use rule::{
    normalize_service_region, ProcurementResponsibilityRule, ProcurementResponsibilityRuleData,
    ProcurementResponsibilityRuleType, ProcurementResponsibilitySelectorReference,
};
