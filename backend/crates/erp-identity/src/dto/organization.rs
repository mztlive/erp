//! 组织查询展示视图；不暴露原始授权证明或不可见组织。

use serde::Serialize;

use crate::entity::organization::{OrgManagementAssignment, OrgMembership, OrgUnit};

/// 组织配置查询展示视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OrganizationStateView {
    /// 当前组织拓扑版本。
    pub version: u64,
    /// 当前配置边界内的组织节点。
    pub units: Vec<OrgUnit>,
    /// 当前配置边界内的主属成员关系。
    pub memberships: Vec<OrgMembership>,
    /// 当前配置边界内的管理授权。
    pub management: Vec<OrgManagementAssignment>,
    /// 人员展示与分派候选；不含联系方式。
    pub people: Vec<OrgPersonView>,
    /// 管理授权展示与可选角色。
    pub roles: Vec<OrgRoleView>,
    /// 本次解析的范围版本。
    pub scope_version: String,
    /// 当前权限策略版本。
    pub policy_version: u64,
    /// 与 `version` 相同的组织版本，供客户端缓存失效。
    pub organization_version: u64,
    /// 解析时点（RFC3339 UTC）。
    pub as_of: String,
    /// 角色缺范围时为 `no_scope`；有规则但无节点时为空。
    pub empty_reason: Option<&'static str>,
    /// 面向客户端的范围摘要，不含内部证明。
    pub scope_summary: &'static str,
    /// 组织配置归属口径。
    pub ownership_basis: &'static str,
}

/// 组织页面人员展示；值使用稳定账号 ID。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OrgPersonView {
    /// 账号 ID。
    pub id: String,
    /// 显示名。
    pub label: String,
    /// 登录账号。
    pub account: String,
    /// 是否为有效后台账号。
    pub active: bool,
    /// 当前主属组织；无主属时为空。
    pub own_org_unit_id: Option<String>,
}

/// 组织页面角色展示；值使用稳定角色 ID。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OrgRoleView {
    /// 角色 ID。
    pub id: String,
    /// 角色名称。
    pub name: String,
    /// 角色是否启用。
    pub enabled: bool,
}

impl OrganizationStateView {
    /// 组合组织查询展示字段。
    ///
    /// # 参数
    /// * `visible` - 已按配置边界裁剪的组织事实
    /// * `scope_version` - 本次解析范围版本
    /// * `policy_version` - 权限策略版本
    /// * `as_of` - 解析时点
    /// * `no_scope` - 角色是否缺少该动作范围
    /// * `people` - 人员展示
    /// * `roles` - 角色展示
    ///
    /// # 返回
    /// 含范围版本、空集原因和展示标签的查询视图。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 缺范围标记 `no_scope`，不得补 Company；组织配置权不授予业务执行权。
    pub fn compose(
        visible: crate::entity::organization_change::OrganizationState,
        scope_version: String,
        policy_version: u64,
        as_of: String,
        no_scope: bool,
        people: Vec<OrgPersonView>,
        roles: Vec<OrgRoleView>,
    ) -> Self {
        let version = visible.version;
        Self {
            version,
            units: visible.units,
            memberships: visible.memberships,
            management: visible.management,
            people,
            roles,
            scope_version,
            policy_version,
            organization_version: version,
            as_of,
            empty_reason: no_scope.then_some("no_scope"),
            scope_summary: "组织配置边界内的内部组织、成员与管理关系",
            ownership_basis: "org_unit_configuration",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::organization_change::OrganizationState;

    #[test]
    fn compose_marks_no_scope_without_inventing_company() {
        let view = OrganizationStateView::compose(
            OrganizationState {
                version: 4,
                units: Vec::new(),
                memberships: Vec::new(),
                management: Vec::new(),
            },
            "scope-v".into(),
            9,
            "2026-09-15T00:00:00Z".into(),
            true,
            Vec::new(),
            Vec::new(),
        );
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["empty_reason"], "no_scope");
        assert_eq!(json["organization_version"], 4);
        assert_eq!(json["policy_version"], 9);
        assert_eq!(json["scope_version"], "scope-v");
        assert_eq!(json["ownership_basis"], "org_unit_configuration");
        assert!(json.get("role_clauses").is_none());
    }
}
