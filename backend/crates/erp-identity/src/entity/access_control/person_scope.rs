//! 人员按资源动作保存一份完整范围表达式，不引用角色身份。
use entity_core::BaseModel;
use entity_macros::{Entity, id_type};
use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};

use super::{
    DataScope, DataScopeData, DataScopeId, DataScopeSubjectType, DataScopeType, ResolvedScope, ScopeBinding,
    ScopeClause, ScopeDimension, ScopeResolution, ScopeTargetMode,
};
use crate::entity::organization::OrgTree;
use crate::entity::organization_change::OrganizationState;
use crate::{Error, Result};

id_type!(PersonDataScopeId);

/// 无角色引用的动态范围项；同组内同维度求并，不同维度求交。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonScopeTerm {
    pub scope_type: DataScopeType,
    pub target_dimension: ScopeDimension,
    pub target_mode: Option<ScopeTargetMode>,
    pub include_descendants: Option<bool>,
    pub scope_targets: Vec<String>,
}

/// 一份有效人员配置。condition 属于同一表达式，用于无损保留历史交集。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PersonScopeExpression {
    pub history_read: bool,
    pub alternatives: Vec<Vec<PersonScopeTerm>>,
    pub condition: Option<Vec<PersonScopeTerm>>,
}

/// 唯一人员、业务、动作配置；缺失配置必须失败关闭。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct PersonDataScope {
    #[serde(flatten)]
    pub base: BaseModel,
    pub user_id: String,
    pub resource: String,
    pub action: String,
    pub expression: PersonScopeExpression,
}

impl PersonScopeTerm {
    /// 创建无主体的迁移范围项。
    /// # 参数
    /// * `rule` - 旧范围规则。
    /// # 返回
    /// 不携带旧主体身份的范围项。
    /// # 错误
    /// 管理部门依赖角色，必须人工重设，不允许静态替代。
    pub fn from_legacy(rule: &DataScope) -> Result<Self> {
        if rule.binding.target_mode == Some(ScopeTargetMode::ManagedOrgs) {
            return Err(Error::ValidationError("管理部门范围依赖旧角色，必须显式重设人员范围".into()));
        }
        Ok(Self {
            scope_type: rule.scope_type,
            target_dimension: rule.binding.target_dimension,
            target_mode: rule.binding.target_mode,
            include_descendants: rule.binding.include_descendants,
            scope_targets: rule.scope_targets.clone(),
        })
    }

    /// 为纯表达式编译器构建临时范围项，不读取旧集合。
    pub(crate) fn rule(&self, resource: &str, action: &str, subject: &str, limit: bool) -> Result<DataScope> {
        if self.target_mode == Some(ScopeTargetMode::ManagedOrgs) {
            return Err(Error::ValidationError("人员范围不接受角色管理部门模式".into()));
        }
        Ok(DataScope::new(
            DataScopeId::new("expression"),
            DataScopeData {
                subject_type: if limit { DataScopeSubjectType::User } else { DataScopeSubjectType::Role },
                subject_id: subject.into(),
                scope_type: self.scope_type,
                scope_targets: self.scope_targets.clone(),
                binding: ScopeBinding {
                    schema_version: 2,
                    resource: resource.into(),
                    actions: vec![action.into()],
                    target_dimension: self.target_dimension,
                    target_mode: self.target_mode,
                    include_descendants: self.include_descendants,
                    enabled: true,
                },
            },
        )?)
    }
}

impl PersonDataScope {
    /// 将单份表达式编译为所有业务查询与对象检查共用的范围中间态。
    /// # 参数
    /// * `state` - 当前组织及成员有效期事实。
    /// * `dimensions` - 本业务必需维度。
    /// * `at` - 本次统一判断时点。
    /// # 返回
    /// 分支并集与表达式条件的精确结果。
    /// # 错误
    /// 非法部门或不支持的动态模式拒绝。
    pub fn resolve(
        &self,
        state: &OrganizationState,
        dimensions: &[ScopeDimension],
        allows_history: bool,
        at: Instant,
    ) -> Result<ResolvedScope> {
        if allows_history
            && !self.expression.history_read
            && (self.expression.alternatives.len() != 1 || self.expression.condition.is_some())
        {
            return Err(Error::ValidationError("普通人员范围必须包含一组完整条件".into()));
        }
        let mut rules = Vec::new();
        let roles =
            (0..self.expression.alternatives.len()).map(|n| format!("branch-{n}")).collect::<Vec<_>>();
        for (branch, id) in self.expression.alternatives.iter().zip(&roles) {
            for term in branch {
                rules.push(term.rule(&self.resource, &self.action, id, false)?);
            }
        }
        let condition = if self.expression.history_read || !allows_history {
            self.expression.condition.as_ref()
        } else {
            self.expression.alternatives.first()
        };
        if let Some(condition) = condition {
            for term in condition {
                rules.push(term.rule(&self.resource, &self.action, &self.user_id, true)?);
            }
        }
        let tree = OrgTree::new(&state.units)?;
        // 空分支依然可表达合法历史读取；显式空 condition 表示完全拒绝。
        let ids = if roles.is_empty() { vec!["empty".into()] } else { roles };
        let mut result = ScopeResolution {
            user_id: &self.user_id,
            eligible_role_ids: &ids,
            resource: &self.resource,
            action: &self.action,
            required_dimensions: dimensions,
            rules: &rules,
            memberships: &state.memberships,
            management: &[],
            tree: &tree,
            as_of: at,
        }
        .resolve()?;
        if condition.is_some_and(Vec::is_empty) {
            result.user_limit = Some(ScopeClause::default());
        }
        Ok(result)
    }

    /// 缺配置时同时拒绝普通与历史路径。
    /// # 参数
    /// 无。
    /// # 返回
    /// 空范围。
    /// # 错误
    /// 无。
    pub fn denied() -> ResolvedScope {
        ResolvedScope { role_clauses: vec![], user_limit: Some(ScopeClause::default()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access_control::ScopedObject;
    use crate::entity::organization::{OrgMembership, OrgUnit, OrgUnitKind, OrgValidity};

    fn term(kind: DataScopeType) -> PersonScopeTerm {
        PersonScopeTerm {
            scope_type: kind,
            target_dimension: ScopeDimension::InternalOrg,
            target_mode: None,
            include_descendants: None,
            scope_targets: vec![],
        }
    }
    fn config(terms: Vec<PersonScopeTerm>) -> PersonDataScope {
        PersonDataScope {
            base: BaseModel::fake(),
            user_id: "alice".into(),
            resource: "sales_order".into(),
            action: "detail".into(),
            expression: PersonScopeExpression {
                history_read: false,
                alternatives: vec![terms],
                condition: None,
            },
        }
    }
    fn object(owned: bool, history: bool) -> ScopedObject<'static> {
        ScopedObject {
            owned,
            collaborating: false,
            historical_read_participant: history,
            org_unit_id: Some("one"),
            settlement_party_id: None,
            warehouse_id: None,
        }
    }
    #[test]
    fn explicit_self_does_not_add_history_and_company_is_explicit() {
        let state = OrganizationState::default();
        let at = Instant::from_unix_secs(10);
        let own = config(vec![term(DataScopeType::SelfOwned)])
            .resolve(&state, &[ScopeDimension::InternalOrg], true, at)
            .unwrap();
        assert!(own.allows(&object(true, false), true));
        assert!(!own.allows(&object(false, true), true));
        assert!(!PersonDataScope::denied().allows(&object(true, true), true));
        let all = config(vec![term(DataScopeType::Company)])
            .resolve(&state, &[ScopeDimension::InternalOrg], true, at)
            .unwrap();
        assert!(all.allows(&object(false, false), true));
    }
    #[test]
    fn migrated_history_and_write_intersection_remain_exact() {
        let mut policy = config(vec![term(DataScopeType::Company)]);
        policy.expression.history_read = true;
        policy.expression.condition = Some(vec![term(DataScopeType::SelfOwned)]);
        let scope = policy
            .resolve(&OrganizationState::default(), &[ScopeDimension::InternalOrg], true, Instant::now())
            .unwrap();
        assert!(!scope.allows(&object(false, true), true));
        assert!(scope.allows(&object(true, true), true));
        policy.expression.history_read = false;
        policy.action = "update".into();
        let scope = policy
            .resolve(&OrganizationState::default(), &[ScopeDimension::InternalOrg], false, Instant::now())
            .unwrap();
        assert!(!scope.allows(&object(false, false), false));
        assert!(scope.allows(&object(true, false), false));
        policy.expression.condition = Some(vec![]);
        assert!(
            !policy
                .resolve(&OrganizationState::default(), &[], false, Instant::now())
                .unwrap()
                .allows(&object(true, true), true)
        );
    }
    #[test]
    fn own_department_preserves_membership_validity() {
        let department = OrgUnit::new(
            "one".into(),
            "One".into(),
            None,
            OrgUnitKind::Department,
            "system".into(),
            "test".into(),
        )
        .unwrap();
        let state = OrganizationState {
            units: vec![department],
            memberships: vec![OrgMembership {
                base: BaseModel::fake(),
                user_id: "alice".into(),
                org_unit_id: "one".into(),
                validity: OrgValidity {
                    valid_from: Instant::from_unix_secs(10),
                    valid_to: Some(Instant::from_unix_secs(20)),
                },
                changed_by: "admin".into(),
                reason: "test".into(),
            }],
            ..Default::default()
        };
        let policy = config(vec![PersonScopeTerm {
            scope_type: DataScopeType::Organization,
            target_mode: Some(ScopeTargetMode::OwnOrg),
            include_descendants: Some(false),
            ..term(DataScopeType::Organization)
        }]);
        for (time, expected) in [(9, false), (10, true), (19, true), (20, false)] {
            assert_eq!(
                policy
                    .resolve(&state, &[ScopeDimension::InternalOrg], true, Instant::from_unix_secs(time))
                    .unwrap()
                    .allows(&object(false, true), true),
                expected
            );
        }
    }
    #[test]
    fn dimension_combinations_do_not_cross_between_branches() {
        let warehouse = |id: &str| PersonScopeTerm {
            scope_type: DataScopeType::Organization,
            target_dimension: ScopeDimension::Warehouse,
            target_mode: Some(ScopeTargetMode::Explicit),
            include_descendants: None,
            scope_targets: vec![id.into()],
        };
        let party =
            |id: &str| PersonScopeTerm { target_dimension: ScopeDimension::SettlementParty, ..warehouse(id) };
        let mut policy = config(vec![]);
        policy.expression.alternatives =
            vec![vec![warehouse("a"), party("x")], vec![warehouse("b"), party("y")]];
        let scope = policy
            .resolve(
                &OrganizationState::default(),
                &[ScopeDimension::Warehouse, ScopeDimension::SettlementParty],
                false,
                Instant::now(),
            )
            .unwrap();
        let mut target = object(false, false);
        target.warehouse_id = Some("a");
        target.settlement_party_id = Some("x");
        assert!(scope.allows(&target, false));
        target.settlement_party_id = Some("y");
        assert!(!scope.allows(&target, false));
    }
}
