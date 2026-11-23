//! 人员按资源动作保存一份完整范围表达式，不引用角色身份。
mod compile;
use compile::ScopeCompiler;
use entity_core::BaseModel;
use entity_macros::{Entity, id_type};
use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};

use super::{DataScopeType, ResolvedScope, ScopeClause, ScopeDimension, ScopeTargetMode, scope_spec};
use crate::entity::organization::OrgTree;
use crate::entity::organization_change::OrganizationState;
use crate::{Error, Result};

// 声明人员范围记录的专用主键类型，存储时仍使用字符串。
id_type!(PersonDataScopeId);

/// 人员范围分支中的单项条件，无角色引用；组内同维度取并集，不同维度取交集。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct PersonScopeTerm {
    /// 决定按公司、组织、团队、本人负责或协作参与匹配对象。
    pub scope_type: DataScopeType,
    /// 标记目标身份所属维度：内部组织、结算主体或仓库。
    pub target_dimension: ScopeDimension,
    /// 指定组织或团队范围的目标来源；公司、本人及协作范围留空。
    pub target_mode: Option<ScopeTargetMode>,
    /// 内部组织的显式和本人组织模式须声明是否展开下级，其余模式留空。
    pub include_descendants: Option<bool>,
    /// 显式模式保存稳定目标标识；动态模式及无需目标的类型使用空列表。
    pub scope_targets: Vec<String>,
}

/// 单个资源动作的人员附加范围表达式；解析时按动作政策自动补本人基础。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PersonScopeExpression {
    /// 任选其一的正向附加范围分支；解析时只保留独立满足必需维度的非空组。
    pub alternatives: Vec<Vec<PersonScopeTerm>>,
}

/// 按账号、业务资源和动作唯一保存的人员配置；本人基础由动作政策提供。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct PersonDataScope {
    /// 与配置字段平铺存储的记录主键、版本及生命周期元数据。
    #[serde(flatten)]
    pub base: BaseModel,
    /// 配置所属账号，用于解析本人及其有效组织关系。
    pub user_id: String,
    /// 配置适用的业务资源标识，用于查找动作规格。
    pub resource: String,
    /// 资源下的单一动作标识，决定必需维度和历史读取政策。
    pub action: String,
    /// 保存的附加授权表达式。
    pub expression: PersonScopeExpression,
}

impl PersonScopeExpression {
    /// 按登记的资源动作判断是否有本人基础；动作须有明确的负责人语义且不要求其他身份维度。
    /// 工作项只有 `list` 和 `detail` 默认本人，`manage` 须显式配置；未登记规格时返回 `false`。
    pub fn default_self(resource: &str, action: &str) -> bool {
        scope_spec::find_action(resource, action).is_some_and(|spec| spec.default_self())
    }

    // 编译附加授权时按动作政策追加本人负责分支。
    fn branches(&self, resource: &str, action: &str) -> Vec<Vec<PersonScopeTerm>> {
        let mut branches = self.alternatives.clone();
        if Self::default_self(resource, action) {
            branches.push(vec![PersonScopeTerm {
                scope_type: DataScopeType::SelfOwned,
                target_dimension: ScopeDimension::InternalOrg,
                target_mode: None,
                include_descendants: None,
                scope_targets: vec![],
            }]);
        }
        branches
    }

    // 仅在动作支持历史读取时，用内部组织维度的授予并集限制历史补权。
    fn read_limit(
        branches: &[Vec<PersonScopeTerm>],
        dimensions: &[ScopeDimension],
        allows_history: bool,
    ) -> Result<Option<Vec<PersonScopeTerm>>> {
        if !allows_history {
            return Ok(None);
        }
        if dimensions != [ScopeDimension::InternalOrg]
            || branches.iter().flatten().any(|term| term.target_dimension != ScopeDimension::InternalOrg)
        {
            return Err(Error::ValidationError("历史读取消费者不支持跨维度附加授权".into()));
        }
        Ok(Some(branches.iter().flatten().cloned().collect()))
    }
}

impl PersonDataScope {
    /// 按组织事实和统一判断时点解析正向分支与上限；必需维度及历史读取政策固定取自动作登记。
    /// 动作没有范围规格或表达式非法时返回校验错误。
    pub fn resolve(&self, state: &OrganizationState, at: Instant) -> Result<ResolvedScope> {
        let spec = scope_spec::require_action(&self.resource, &self.action)?;
        self.compile(state, spec.dimensions(), spec.allows_history(), at)
    }

    // 按动作的必需维度和历史读取政策编译查询、对象判定共用的范围，丢弃空分支及缺维度分支。
    // 组织和成员关系按同一时点解释；非法组织、目标或动态模式使编译失败。
    fn compile(
        &self,
        state: &OrganizationState,
        dimensions: &[ScopeDimension],
        allows_history: bool,
        at: Instant,
    ) -> Result<ResolvedScope> {
        let branches = self.expression.branches(&self.resource, &self.action);
        let condition = PersonScopeExpression::read_limit(&branches, dimensions, allows_history)?;

        let tree = OrgTree::new(&state.units)?;
        let compiler =
            ScopeCompiler { user: &self.user_id, tree: &tree, memberships: &state.memberships, at };

        let mut role_clauses = Vec::new();
        for branch in &branches {
            let clause = compiler.clause(branch)?;
            if clause != ScopeClause::default() && clause.complete(dimensions) {
                role_clauses.push(clause);
            }
        }

        let user_limit = condition.as_ref().map(|terms| compiler.clause(terms)).transpose()?;
        Ok(ResolvedScope { role_clauses, user_limit })
    }

    /// 账号有效且动作资格成立后，为已登记的资源动作构造未持久化的默认配置。
    /// 配置没有附加授权；动作没有本人基础时，解析得到空的正向范围。
    pub fn default_for(user: &str, resource: &str, action: &str) -> Self {
        Self {
            base: BaseModel::new("default-person-scope".into()),
            user_id: user.into(),
            resource: resource.into(),
            action: action.into(),
            expression: PersonScopeExpression::default(),
        }
    }

    /// 缺少配置且无默认范围时，以空分支和显式空上限关闭普通及历史路径。
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
        let mut policy = PersonDataScope::default_for("alice", "sales_order", "detail");
        policy.expression.alternatives = vec![terms];
        policy
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

    fn departments() -> OrganizationState {
        OrganizationState {
            units: ["one", "two"]
                .into_iter()
                .map(|id| {
                    OrgUnit::new(
                        id.into(),
                        id.into(),
                        None,
                        OrgUnitKind::Department,
                        "system".into(),
                        "test".into(),
                    )
                    .unwrap()
                })
                .collect(),
            ..Default::default()
        }
    }

    fn department(id: &str) -> PersonScopeTerm {
        PersonScopeTerm {
            target_mode: Some(ScopeTargetMode::Explicit),
            include_descendants: Some(false),
            scope_targets: vec![id.into()],
            ..term(DataScopeType::Organization)
        }
    }

    // 验证非法目标被拒绝；显式拒绝关闭两条路径；公司范围须单独授予。
    #[test]
    fn invalid_terms_denied_and_company_scope_behave_as_specified() {
        let state = OrganizationState::default();
        let at = Instant::from_unix_secs(10);
        let policy = config(vec![PersonScopeTerm {
            target_dimension: ScopeDimension::Warehouse,
            target_mode: Some(ScopeTargetMode::Explicit),
            scope_targets: vec!["*".into()],
            ..term(DataScopeType::Organization)
        }]);
        assert!(policy.compile(&state, &[], false, at).is_err());

        let own = PersonDataScope::default_for("alice", "sales_order", "detail").resolve(&state, at).unwrap();
        assert!(own.allows(&object(true, false), true));
        assert!(!own.allows(&object(false, true), true));
        assert!(!PersonDataScope::denied().allows(&object(true, true), true));

        let all = config(vec![term(DataScopeType::Company)]).resolve(&state, at).unwrap();
        assert!(all.allows(&object(false, false), true));
    }

    // 验证本人组织范围在成员关系生效时纳入部门，到失效时点即移除。
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
                    .resolve(&state, Instant::from_unix_secs(time))
                    .unwrap()
                    .allows(&object(false, true), true),
                expected
            );
        }
    }

    // 验证本人基础与附加部门取并集，历史参与受并集限制；移除附加项后只剩本人。
    #[test]
    fn additive_reads_union_self_and_departments_without_history_leak() {
        let mut policy = PersonDataScope::default_for("alice", "sales_order", "detail");
        policy.expression.alternatives = vec![vec![department("one")], vec![department("two")]];
        let scope = policy.resolve(&departments(), Instant::now()).unwrap();
        let mut target = object(false, true);
        assert!(scope.allows(&target, true));
        target.org_unit_id = Some("two");
        assert!(scope.allows(&target, true));
        target.org_unit_id = Some("outside");
        assert!(!scope.allows(&target, true));
        target.owned = true;
        assert!(scope.allows(&target, true));

        policy.expression.alternatives.clear();
        let scope = policy.resolve(&departments(), Instant::now()).unwrap();
        assert!(scope.allows(&target, true));
        target.owned = false;
        target.org_unit_id = Some("one");
        assert!(!scope.allows(&target, true));
    }

    // 验证先移除公司级授权后恢复部门范围，再清除部门授权后只保留本人基础。
    #[test]
    fn removing_company_and_other_grants_restores_remaining_scope() {
        let mut policy = PersonDataScope::default_for("alice", "sales_order", "detail");
        policy.expression.alternatives = vec![vec![department("one")], vec![term(DataScopeType::Company)]];
        let mut target = object(false, false);
        target.org_unit_id = Some("outside");
        assert!(policy.resolve(&departments(), Instant::now()).unwrap().allows(&target, true));

        policy.expression.alternatives.pop();
        let scope = policy.resolve(&departments(), Instant::now()).unwrap();
        assert!(!scope.allows(&target, true));
        target.org_unit_id = Some("one");
        assert!(scope.allows(&target, true));

        policy.expression.alternatives.clear();
        let scope = policy.resolve(&departments(), Instant::now()).unwrap();
        assert!(!scope.allows(&target, true));
        target.owned = true;
        assert!(scope.allows(&target, true));
    }

    // 以无本人基础的动作验证多维附加项保持原组合，启用历史补权时拒绝该维度组合。
    #[test]
    fn multiple_dimension_grants_remain_separate_and_have_no_default_self() {
        let mut policy = PersonDataScope::default_for("alice", "approval_instance", "read");
        let warehouse = |id: &str| PersonScopeTerm {
            scope_type: DataScopeType::Organization,
            target_dimension: ScopeDimension::Warehouse,
            target_mode: Some(ScopeTargetMode::Explicit),
            include_descendants: None,
            scope_targets: vec![id.into()],
        };
        let party =
            |id: &str| PersonScopeTerm { target_dimension: ScopeDimension::SettlementParty, ..warehouse(id) };
        let dimensions = [ScopeDimension::Warehouse, ScopeDimension::SettlementParty];
        let mut target = object(true, false);
        assert!(
            !policy
                .compile(&departments(), &dimensions, false, Instant::now())
                .unwrap()
                .allows(&target, false)
        );

        policy.expression.alternatives =
            vec![vec![warehouse("a"), party("x")], vec![warehouse("b"), party("y")]];
        let scope = policy.compile(&departments(), &dimensions, false, Instant::now()).unwrap();
        target.warehouse_id = Some("a");
        target.settlement_party_id = Some("x");
        assert!(scope.allows(&target, false));
        target.settlement_party_id = Some("y");
        assert!(!scope.allows(&target, false));
        assert!(policy.compile(&departments(), &dimensions, true, Instant::now()).is_err());
    }

    // 验证无人员配置时任务列表和详情只匹配本人，管理动作仍为空范围；管理范围可显式授予。
    #[test]
    fn task_read_defaults_to_self_without_granting_manage() {
        let state = departments();
        for action in ["list", "detail", "manage"] {
            let policy = PersonDataScope::default_for("alice", "work_item", action);
            let scope = policy.resolve(&state, Instant::now()).unwrap();
            assert_eq!(scope.allows(&object(true, false), false), action != "manage");
            assert!(!scope.allows(&object(false, true), false));
        }

        let mut detail = PersonDataScope::default_for("alice", "work_item", "detail");
        detail.expression.alternatives = vec![vec![department("two")]];
        assert!(detail.resolve(&state, Instant::now()).unwrap().allows(&object(true, false), false));

        let mut manage = PersonDataScope::default_for("alice", "work_item", "manage");
        manage.expression.alternatives = vec![vec![term(DataScopeType::SelfOwned)]];
        assert!(manage.resolve(&state, Instant::now()).unwrap().allows(&object(true, false), false));
    }

    // 验证解析沿用各动作登记的本人基础和历史读取政策，未知动作返回错误。
    #[test]
    fn registered_resolution_uses_one_default_and_history_contract() {
        let state = departments();
        for (resource, action, expected_self, expected_history) in [
            ("sales_order", "list", true, true),
            ("sales_order", "update", true, false),
            ("work_item", "list", true, false),
            ("work_item", "manage", false, false),
            ("product", "list", false, false),
        ] {
            let spec = scope_spec::require_action(resource, action).unwrap();
            assert_eq!(spec.default_self(), expected_self);
            assert_eq!(spec.allows_history(), expected_history);
            assert_eq!(PersonScopeExpression::default_self(resource, action), expected_self);
            let policy = PersonDataScope::default_for("alice", resource, action);
            let scope = policy.resolve(&state, Instant::now()).unwrap();
            assert_eq!(scope.allows(&object(true, false), false), expected_self);
            assert!(!scope.allows(&object(false, true), spec.allows_history()));
        }
        let unknown = PersonDataScope::default_for("alice", "sales_order", "unknown");
        assert!(unknown.resolve(&state, Instant::now()).is_err());
    }
}
