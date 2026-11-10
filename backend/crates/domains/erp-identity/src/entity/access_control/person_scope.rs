//! 人员按资源动作保存一份完整范围表达式，不引用角色身份。
mod compile;
use compile::ScopeCompiler;
use entity_core::BaseModel;
use entity_macros::{Entity, id_type};
use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};

use super::consumer_spec::registration;
use super::scope_target::{BoundScopeTarget, ScopeTarget};
use super::{
    DataScope, DataScopeType, ResolvedScope, ScopeBinding, ScopeClause, ScopeDimension, ScopeTargetMode,
};
use crate::entity::organization::OrgTree;
use crate::entity::organization_change::OrganizationState;
use crate::{Error, Result};

id_type!(PersonDataScopeId);

/// 无角色引用的动态范围项；同组内同维度求并，不同维度求交。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct PersonScopeTerm {
    pub scope_type: DataScopeType,
    pub target_dimension: ScopeDimension,
    pub target_mode: Option<ScopeTargetMode>,
    pub include_descendants: Option<bool>,
    pub scope_targets: Vec<String>,
}

/// 一份有效人员配置。condition 属于同一表达式，用于无损保留历史交集。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PersonScopeExpression {
    /// 新配置仅保存附加项；旧配置缺少此字段时保持原表达式。
    #[serde(default)]
    pub additive: bool,
    pub history_read: bool,
    pub alternatives: Vec<Vec<PersonScopeTerm>>,
    pub condition: Option<Vec<PersonScopeTerm>>,
}

/// 唯一人员、业务、动作配置；基础范围由明确的业务负责人政策提供。
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

    /// 将原始人员范围解析为已验证目标，拒绝角色专用动态模式。
    /// # 参数
    /// resource、actions 为所属业务和操作。
    /// # 返回
    /// 不包含角色管理部门模式的人员目标。
    /// # 错误
    /// 绑定、目标或人员动态模式非法时拒绝。
    pub(crate) fn target(&self, resource: &str, actions: &[String]) -> Result<ScopeTarget> {
        let target = self.binding(resource, actions).target(self.scope_type, &self.scope_targets)?;
        match target {
            BoundScopeTarget::Personal(target) => Ok(target),
            BoundScopeTarget::ManagedOrgs => {
                Err(Error::ValidationError("人员范围不接受角色管理部门模式".into()))
            },
        }
    }

    /// 生成持久化协议绑定，仅在输入和历史数据边界使用。
    /// # 参数
    /// resource、actions 为所属业务和操作。
    /// # 返回
    /// 与原始范围字段对应的绑定，尚未证明有效性。
    /// # 错误
    /// 不返回错误。
    pub(crate) fn binding(&self, resource: &str, actions: &[String]) -> ScopeBinding {
        ScopeBinding {
            schema_version: 2,
            resource: resource.into(),
            actions: actions.to_vec(),
            target_dimension: self.target_dimension,
            target_mode: self.target_mode,
            include_descendants: self.include_descendants,
            enabled: true,
        }
    }
}

impl PersonScopeExpression {
    /// 按资源动作判定本人基础；任务仅 list/detail 默认本人，manage 必须显式授权。
    /// # 参数
    /// * `resource` - 已注册业务标识。
    /// * `action` - 已登记的资源动作。
    /// # 返回
    /// 仅负责人语义明确且不要求其他身份维度的业务返回 `true`；其余业务返回 `false`。
    /// # 错误
    /// 不返回错误。
    pub fn default_self(resource: &str, action: &str) -> bool {
        registration(resource, action).is_ok_and(|spec| spec.default_self())
    }

    /// 基础本人只加入新模型；旧配置原样保留，不因发布自动扩权。
    fn branches(&self, resource: &str, action: &str) -> Vec<Vec<PersonScopeTerm>> {
        let mut branches = self.alternatives.clone();
        if self.additive && Self::default_self(resource, action) {
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

    /// 读取的上限精确匹配授权并集，防止历史参与在范围外补权。
    fn read_limit(
        &self,
        branches: &[Vec<PersonScopeTerm>],
        dimensions: &[ScopeDimension],
        allows_history: bool,
    ) -> Result<Option<Vec<PersonScopeTerm>>> {
        if !self.additive {
            if allows_history && !self.history_read && (branches.len() != 1 || self.condition.is_some()) {
                return Err(Error::ValidationError("普通人员范围必须包含一组完整条件".into()));
            }
            return Ok(if self.history_read || !allows_history {
                self.condition.clone()
            } else {
                branches.first().cloned()
            });
        }
        if self.history_read || self.condition.is_some() {
            return Err(Error::ValidationError("附加授权不能携带历史读取或旧交集条件".into()));
        }
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
    /// 按资源动作的唯一规格解析人员范围，调用方不能覆盖维度或历史读取能力。
    /// # 参数
    /// `state` 为组织事实；`at` 为本次统一判断时点。
    /// # 返回
    /// 当前动作的分支并集与表达式条件。
    /// # 错误
    /// 未登记动作或非法范围表达式返回校验错误。
    pub fn resolve(&self, state: &OrganizationState, at: Instant) -> Result<ResolvedScope> {
        let spec = registration(&self.resource, &self.action)?;
        self.compile(state, spec.required_dimensions(), spec.allows_history(), at)
    }

    /// 将单份表达式编译为所有业务查询与对象检查共用的范围中间态。
    /// # 参数
    /// * `state` - 当前组织及成员有效期事实。
    /// * `dimensions` - 本业务必需维度。
    /// * `allows_history` - 该业务是否允许用历史参与补读取。
    /// * `at` - 本次统一判断时点。
    /// # 返回
    /// 分支并集与表达式条件的精确结果。
    /// # 错误
    /// 非法部门或不支持的动态模式拒绝。
    fn compile(
        &self,
        state: &OrganizationState,
        dimensions: &[ScopeDimension],
        allows_history: bool,
        at: Instant,
    ) -> Result<ResolvedScope> {
        let branches = self.expression.branches(&self.resource, &self.action);
        let condition = self.expression.read_limit(&branches, dimensions, allows_history)?;
        let tree = OrgTree::new(&state.units)?;
        let compiler = ScopeCompiler {
            user: &self.user_id,
            resource: &self.resource,
            action: &self.action,
            tree: &tree,
            memberships: &state.memberships,
            at,
        };
        let mut role_clauses = Vec::new();
        for branch in &branches {
            let clause = compiler.clause(branch)?;
            if clause != ScopeClause::default() && clause.complete(dimensions) {
                role_clauses.push(clause);
            }
        }
        // 显式空条件编译为空条款，始终拒绝普通和历史读取。
        let user_limit = condition.as_ref().map(|terms| compiler.clause(terms)).transpose()?;
        Ok(ResolvedScope { role_clauses, user_limit })
    }

    /// 构造尚未持久化的基础范围；必须先完成账号与动作资格检查。
    /// # 参数
    /// * `user` - 人员账号 ID。
    /// * `resource` - 已注册业务标识。
    /// * `action` - 操作标识。
    /// # 返回
    /// 仅含默认政策、没有附加授权的临时配置。无基础政策的业务留待解析时得到空范围。
    /// # 错误
    /// 不返回错误。
    pub fn default_for(user: &str, resource: &str, action: &str) -> Self {
        Self {
            base: BaseModel::new("default-person-scope".into()),
            user_id: user.into(),
            resource: resource.into(),
            action: action.into(),
            expression: PersonScopeExpression { additive: true, ..Default::default() },
        }
    }

    /// 缺配置时同时拒绝普通与历史路径。
    /// # 参数
    /// 无。
    /// # 返回
    /// 空范围。
    /// # 错误
    /// 不返回错误。
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
                additive: false,
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
    fn invalid_terms_and_empty_branches_remain_closed() {
        let state = OrganizationState::default();
        let mut policy = config(vec![PersonScopeTerm {
            target_mode: Some(ScopeTargetMode::ManagedOrgs),
            ..term(DataScopeType::Organization)
        }]);
        assert!(policy.compile(&state, &[], false, Instant::now()).is_err());
        policy.expression.alternatives = vec![vec![PersonScopeTerm {
            target_dimension: ScopeDimension::Warehouse,
            target_mode: Some(ScopeTargetMode::Explicit),
            scope_targets: vec!["*".into()],
            ..term(DataScopeType::Organization)
        }]];
        assert!(policy.compile(&state, &[], false, Instant::now()).is_err());
        policy.expression.alternatives.clear();
        policy.expression.history_read = true;
        let scope = policy.compile(&state, &[], true, Instant::now()).unwrap();
        assert!(!scope.has_role_scope());
        assert!(!scope.allows(&object(true, false), false));
        assert!(scope.allows(&object(false, true), true));
        policy.expression.condition = Some(vec![PersonScopeTerm {
            target_mode: Some(ScopeTargetMode::OwnOrg),
            include_descendants: Some(false),
            ..term(DataScopeType::Organization)
        }]);
        assert!(
            !policy.compile(&state, &[], true, Instant::now()).unwrap().allows(&object(false, true), true)
        );
    }

    #[test]
    fn explicit_self_does_not_add_history_and_company_is_explicit() {
        let state = OrganizationState::default();
        let at = Instant::from_unix_secs(10);
        let own = config(vec![term(DataScopeType::SelfOwned)]).resolve(&state, at).unwrap();
        assert!(own.allows(&object(true, false), true));
        assert!(!own.allows(&object(false, true), true));
        assert!(!PersonDataScope::denied().allows(&object(true, true), true));
        let all = config(vec![term(DataScopeType::Company)]).resolve(&state, at).unwrap();
        assert!(all.allows(&object(false, false), true));
    }
    #[test]
    fn migrated_history_and_write_intersection_remain_exact() {
        let mut policy = config(vec![term(DataScopeType::Company)]);
        policy.expression.history_read = true;
        policy.expression.condition = Some(vec![term(DataScopeType::SelfOwned)]);
        let scope = policy.resolve(&OrganizationState::default(), Instant::now()).unwrap();
        assert!(!scope.allows(&object(false, true), true));
        assert!(scope.allows(&object(true, true), true));
        policy.expression.history_read = false;
        policy.action = "update".into();
        let scope = policy.resolve(&OrganizationState::default(), Instant::now()).unwrap();
        assert!(!scope.allows(&object(false, false), false));
        assert!(scope.allows(&object(true, false), false));
        policy.expression.condition = Some(vec![]);
        assert!(
            !policy
                .compile(&OrganizationState::default(), &[], false, Instant::now())
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
                    .resolve(&state, Instant::from_unix_secs(time))
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
            .compile(
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
    /// 建立两个有效部门用于验证实际组织范围解析。
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

    /// 生成明确部门范围，包含下级的选择不能省略。
    fn department(id: &str) -> PersonScopeTerm {
        PersonScopeTerm {
            target_mode: Some(ScopeTargetMode::Explicit),
            include_descendants: Some(false),
            scope_targets: vec![id.into()],
            ..term(DataScopeType::Organization)
        }
    }

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

    #[test]
    fn legacy_records_do_not_gain_self_and_additive_rejects_legacy_conditions() {
        let mut policy = config(vec![department("one")]);
        let mut serialized = serde_json::to_value(&policy).unwrap();
        serialized["expression"].as_object_mut().unwrap().remove("additive");
        policy = serde_json::from_value(serialized).unwrap();
        assert!(!policy.expression.additive);
        let mut target = object(true, false);
        target.org_unit_id = Some("outside");
        assert!(!policy.resolve(&departments(), Instant::now()).unwrap().allows(&target, true));
        policy.expression.additive = true;
        policy.expression.history_read = true;
        assert!(policy.resolve(&departments(), Instant::now()).is_err());
        policy.expression.history_read = false;
        policy.expression.condition = Some(vec![]);
        assert!(policy.resolve(&departments(), Instant::now()).is_err());
    }

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
    /// 无人员配置时只为任务读取提供本人基础，不能取得他人任务或管理范围。
    #[test]
    fn task_read_defaults_to_self_without_granting_manage() {
        for action in ["list", "detail", "manage"] {
            let policy = PersonDataScope::default_for("alice", "work_item", action);
            let scope = policy.resolve(&departments(), Instant::now()).unwrap();
            assert_eq!(scope.allows(&object(true, false), false), action != "manage");
            assert!(!scope.allows(&object(false, true), false));
        }
    }

    /// 追加组织范围不会替代本人读取；旧显式限制与显式拒绝不被新默认绕过。
    #[test]
    fn task_self_baseline_respects_explicit_scope_contract() {
        let mut policy = PersonDataScope::default_for("alice", "work_item", "detail");
        policy.expression.alternatives = vec![vec![department("two")]];
        let resolve = |policy: &PersonDataScope| policy.resolve(&departments(), Instant::now()).unwrap();
        assert!(resolve(&policy).allows(&object(true, false), false));
        policy.expression.additive = false;
        assert!(!resolve(&policy).allows(&object(true, false), false));
        policy.expression.alternatives = vec![vec![term(DataScopeType::SelfOwned)]];
        policy.expression.condition = Some(vec![]);
        assert!(!resolve(&policy).allows(&object(true, false), false));
        policy.action = "manage".into();
        policy.expression.condition = None;
        assert!(resolve(&policy).allows(&object(true, false), false));
    }
    /// 公开入口使用动作规格；默认本人和历史读取不能由调用者覆盖。
    #[test]
    fn registered_resolution_uses_one_default_and_history_contract() {
        let state = departments();
        for (resource, action, expected_self, expected_history) in [
            ("sales_order", "list", true, true),
            ("sales_order", "update", true, false),
            ("work_item", "list", true, false),
            ("work_item", "manage", false, false),
            ("product", "list", false, false),
            ("reconciliation_difference", "create", false, false),
        ] {
            let spec = registration(resource, action).unwrap();
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
