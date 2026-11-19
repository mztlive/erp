//! 人员范围使用的目标维度和目标模式。

use serde::{Deserialize, Serialize};

/// 范围目标的身份维度；各维度独立匹配，同名身份也不能混用。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ScopeDimension {
    /// 匹配内部组织归属；静态与动态组织目标均使用此维度。
    InternalOrg,
    /// 匹配业务对象的结算主体，目标须显式指定。
    SettlementParty,
    /// 匹配业务对象的仓库，目标须显式指定。
    Warehouse,
}

/// 组织或团队范围取得目标身份的方式。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ScopeTargetMode {
    /// 使用配置中明确列出的目标身份。
    Explicit,
    /// 按判断时点的有效成员关系取得本人的内部组织。
    OwnOrg,
    /// 旧角色的管理部门模式；保留解码兼容性，人员范围拒绝使用。
    ManagedOrgs,
}
