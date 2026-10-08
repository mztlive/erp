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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

    /// 为纯表达式编译器构建临时范围项，不读取旧集合。
    ///
    /// # 参数
    /// * `resource` - 业务资源标识。
    /// * `action` - 单个动作标识。
    /// * `subject` - 临时主体 ID。
    /// * `limit` - 为真时主体类型是用户上限，否则是角色分支。
    ///
    /// # 返回
    /// 返回启用的临时 `DataScope`。身份固定为 `expression`。
    ///
    /// # 错误
    /// 目标模式为 `ManagedOrgs` 时返回校验错误。主体、绑定或目标不合法时传播 [`DataScope::new`] 的错误。
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

impl PersonScopeExpression {
    /// 明确允许使用当前业务负责人作为基础范围的业务。
    /// # 参数
    /// * `resource` - 已注册业务标识。
    /// # 返回
    /// 仅负责人语义明确且不要求其他身份维度的业务返回 `true`；其余业务返回 `false`。
    /// # 错误
    /// 不返回错误。
    pub fn default_self(resource: &str) -> bool {
        matches!(
            resource,
            "customer"
                | "contract"
                | "sales_order"
                | "purchase_order"
                | "cost_entry"
                | "cost_allocation"
                | "sales_selection_booklet"
                | "sales_selection_proposal"
                | "receivable_account"
                | "customer_receipt"
                | "invoice"
                | "sales_invoice_request"
                | "payable_account"
                | "supplier_payment"
                | "purchase_invoice_allocation"
                | "supplier_settlement_statement"
                | "supplier"
                | "product"
                | "supplier_offering"
                | "supplier_fulfillment_order"
                | "integration_error_task"
                | "reconciliation_difference"
        )
    }

    /// 基础本人只加入新模型；旧配置原样保留，不因发布自动扩权。
    fn branches(&self, resource: &str) -> Vec<Vec<PersonScopeTerm>> {
        let mut branches = self.alternatives.clone();
        if self.additive && Self::default_self(resource) {
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
    pub fn resolve(
        &self,
        state: &OrganizationState,
        dimensions: &[ScopeDimension],
        allows_history: bool,
        at: Instant,
    ) -> Result<ResolvedScope> {
        let branches = self.expression.branches(&self.resource);
        let condition = self.expression.read_limit(&branches, dimensions, allows_history)?;
        let mut rules = Vec::new();
        let roles = (0..branches.len()).map(|n| format!("branch-{n}")).collect::<Vec<_>>();
        for (branch, id) in branches.iter().zip(&roles) {
            for term in branch {
                rules.push(term.rule(&self.resource, &self.action, id, false)?);
            }
        }
        if let Some(condition) = &condition {
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
            tree: &tree,
            as_of: at,
        }
        .resolve()?;
        if condition.as_ref().is_some_and(Vec::is_empty) {
            result.user_limit = Some(ScopeClause::default());
        }
        Ok(result)
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
        let scope =
            policy.resolve(&departments(), &[ScopeDimension::InternalOrg], true, Instant::now()).unwrap();
        let mut target = object(false, true);
        assert!(scope.allows(&target, true));
        target.org_unit_id = Some("two");
        assert!(scope.allows(&target, true));
        target.org_unit_id = Some("outside");
        assert!(!scope.allows(&target, true));
        target.owned = true;
        assert!(scope.allows(&target, true));
        policy.expression.alternatives.clear();
        let scope =
            policy.resolve(&departments(), &[ScopeDimension::InternalOrg], true, Instant::now()).unwrap();
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
        assert!(
            policy
                .resolve(&departments(), &[ScopeDimension::InternalOrg], true, Instant::now())
                .unwrap()
                .allows(&target, true)
        );
        policy.expression.alternatives.pop();
        let scope =
            policy.resolve(&departments(), &[ScopeDimension::InternalOrg], true, Instant::now()).unwrap();
        assert!(!scope.allows(&target, true));
        target.org_unit_id = Some("one");
        assert!(scope.allows(&target, true));
        policy.expression.alternatives.clear();
        let scope =
            policy.resolve(&departments(), &[ScopeDimension::InternalOrg], true, Instant::now()).unwrap();
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
        assert!(
            !policy
                .resolve(&departments(), &[ScopeDimension::InternalOrg], true, Instant::now())
                .unwrap()
                .allows(&target, true)
        );
        policy.expression.additive = true;
        policy.expression.history_read = true;
        assert!(
            policy.resolve(&departments(), &[ScopeDimension::InternalOrg], true, Instant::now()).is_err()
        );
        policy.expression.history_read = false;
        policy.expression.condition = Some(vec![]);
        assert!(
            policy.resolve(&departments(), &[ScopeDimension::InternalOrg], false, Instant::now()).is_err()
        );
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
                .resolve(&departments(), &dimensions, false, Instant::now())
                .unwrap()
                .allows(&target, false)
        );
        policy.expression.alternatives =
            vec![vec![warehouse("a"), party("x")], vec![warehouse("b"), party("y")]];
        let scope = policy.resolve(&departments(), &dimensions, false, Instant::now()).unwrap();
        target.warehouse_id = Some("a");
        target.settlement_party_id = Some("x");
        assert!(scope.allows(&target, false));
        target.settlement_party_id = Some("y");
        assert!(!scope.allows(&target, false));
        assert!(policy.resolve(&departments(), &dimensions, true, Instant::now()).is_err());
    }
}
