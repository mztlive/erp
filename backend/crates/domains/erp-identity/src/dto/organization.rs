//! 组织查询展示视图；不暴露原始授权证明或不可见组织。

use serde::Serialize;

use crate::entity::organization::{OrgMembership, OrgUnit};

/// 组织管理页读取配置状态时返回，只展示配置边界内的组织事实与必要授权版本。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OrganizationStateView {
    /// 当前组织配置版本，供后续提交检查并发变更。
    pub version: u64,
    /// 本次配置边界内可见的组织节点。
    pub units: Vec<OrgUnit>,
    /// 配置边界内可见的主属成员关系。
    pub memberships: Vec<OrgMembership>,
    /// 组织页面的人员标签与分派候选，不含联系方式。
    pub people: Vec<OrgPersonView>,
    /// 用于解释管理授权和选择角色的角色资料。
    pub roles: Vec<OrgRoleView>,
    /// 本次解析的配置范围版本，供客户端识别范围变化。
    pub scope_version: String,
    /// 读取组织状态时所用的权限策略版本。
    pub policy_version: u64,
    /// 与 `version` 相同的组织版本，用于客户端缓存失效。
    pub organization_version: u64,
    /// 解析本次配置边界的时点（RFC3339 UTC）。
    pub as_of: String,
    /// 缺少角色范围时为 `no_scope`；有范围但无节点时为空。
    pub empty_reason: Option<&'static str>,
    /// 配置边界的展示说明，不包含内部授权证明。
    pub scope_summary: &'static str,
    /// 配置归属使用的口径标识，不表示业务执行资格。
    pub ownership_basis: &'static str,
}

/// 组织页面展示成员和选择分派人员所需的账号摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OrgPersonView {
    /// 用于引用或分派人员的稳定账号 ID。
    pub id: String,
    /// 组织界面显示的人员名称。
    pub label: String,
    /// 用于在选择时区分同名人员的登录账号。
    pub account: String,
    /// 该账号是否为允许登录的后台账号。
    pub active: bool,
    /// 查询时点在配置边界内可见的主属组织 ID；不可见或无主属时为空。
    pub own_org_unit_id: Option<String>,
}

/// 组织管理授权展示和角色选择所用的角色摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OrgRoleView {
    /// 管理授权引用的稳定角色 ID。
    pub id: String,
    /// 组织界面显示的角色名称。
    pub name: String,
    /// 角色启用标记；配置页只提供启用角色。
    pub enabled: bool,
}

impl OrganizationStateView {
    /// 组织查询返回前组装已裁剪的配置事实、标签和授权版本。
    /// 缺少角色范围只标记 `no_scope`，不补公司范围；配置权不授予业务执行权。
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
            people,
            roles,
            scope_version,
            policy_version,
            organization_version: version,
            as_of,
            empty_reason: no_scope.then_some("no_scope"),
            scope_summary: "组织配置边界内的内部组织与成员",
            ownership_basis: "org_unit_configuration",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::organization_change::OrganizationState;

    // 验证缺范围时响应保留版本与配置口径，标记 `no_scope` 且不暴露内部角色条款。
    #[test]
    fn compose_marks_no_scope_without_inventing_company() {
        let view = OrganizationStateView::compose(
            OrganizationState { version: 4, units: Vec::new(), memberships: Vec::new() },
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
