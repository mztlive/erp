//! 人员基础范围与按操作合并的附加授权接口。
use std::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::access_control::{DataScopeType, ScopeBinding, ScopeDimension};
use crate::entity::access_control::person_scope::{PersonDataScope, PersonScopeExpression, PersonScopeTerm};
use crate::entity::access_control::scope_spec;
use crate::entity::access_control::scope_target::ScopeTarget;
use crate::{Error, Result};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavePersonScopeRequest {
    pub resource: String,
    /// 本次完整替换的动作；未选择动作的配置保持原样。
    pub actions: Vec<String>,
    pub grants: Vec<PersonScopeGrant>,
    #[serde(default)]
    pub replace_legacy: bool,
    pub expected_policy_version: u64,
}

/// 已完成静态校验的人员范围保存命令；只提供不可变请求视图。
#[derive(Debug)]
pub struct PersonScopeCommand {
    request: SavePersonScopeRequest,
    targets: Vec<ScopeTarget>,
}
impl Deref for PersonScopeCommand {
    type Target = SavePersonScopeRequest;
    // 向范围配置和文件规范化用例借出已验证请求；不提供可变访问，避免绕过原校验。
    fn deref(&self) -> &Self::Target {
        &self.request
    }
}
impl PersonScopeCommand {
    // 消耗已验证范围命令，导出规范化的协议数据，供授权文件保存或规范化使用。
    pub(crate) fn into_request(self) -> SavePersonScopeRequest {
        self.request
    }

    // 借出原命令解析出的完整目标集合，供同一写事务核对引用实际存在性。
    pub(crate) fn targets(&self) -> &[ScopeTarget] {
        &self.targets
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PersonScopeGrant {
    pub actions: Vec<String>,
    pub terms: Vec<PersonScopeTerm>,
}

#[derive(Serialize)]
pub struct PersonBusinessOption {
    pub configurable_actions: Vec<String>,
    pub policy_description: String,
    pub resource: String,
    pub actions: Vec<String>,
    pub dimensions: Vec<ScopeDimension>,
    /// 全部可配置动作是否均包含本人基础；部分动作时使用 default_self_actions。
    pub default_self: bool,
    pub default_self_actions: Vec<String>,
}
#[derive(Serialize)]
pub struct PersonScopeView {
    pub retired_items: Vec<RetiredPersonScope>,
    pub items: Vec<PersonDataScope>,
    pub businesses: Vec<PersonBusinessOption>,
    pub policy_version: u64,
}

impl PersonBusinessOption {
    /// 将已证明的角色动作映射为配置候选，说明按每个动作的实际能力生成。
    /// 可编辑动作子集、维度并集与完整角色动作。
    ///
    /// 空动作或未登记动作返回校验错误。
    ///
    /// `resource`、`actions` 为当前角色已证明的动作。
    pub fn from_granted(resource: &str, actions: Vec<String>) -> Result<Self> {
        if actions.is_empty() {
            return Err(Error::ValidationError("缺少操作权限".into()));
        }

        let policy_description = scope_spec::policy_description(resource, &actions)?;
        let configurable_actions = actions
            .iter()
            .filter(|action| scope_spec::is_configurable(resource, action))
            .cloned()
            .collect::<Vec<_>>();
        let default_self_actions = configurable_actions
            .iter()
            .filter(|action| {
                scope_spec::find_action(resource, action).is_some_and(|spec| spec.default_self())
            })
            .cloned()
            .collect::<Vec<_>>();
        let default_self =
            !configurable_actions.is_empty() && default_self_actions.len() == configurable_actions.len();
        let dimensions = scope_spec::configurable_dimensions(resource, &configurable_actions);

        Ok(Self {
            resource: resource.into(),
            configurable_actions,
            policy_description,
            default_self,
            default_self_actions,
            actions,
            dimensions,
        })
    }
}

/// 保留原记录用于审计，但不能再次编辑已退役策略。
#[derive(Serialize)]
pub struct RetiredPersonScope {
    pub resource: String,
    pub action: String,
    pub reason: String,
}

impl RetiredPersonScope {
    /// 从历史记录取得当前配置政策退役说明。
    /// 不支持独立配置时返回说明；有效配置返回 None。
    ///
    /// 未知资源同样标记不可配置。
    ///
    /// `scope` 为已存储的原始范围记录。
    pub fn from_scope(scope: &PersonDataScope) -> Option<Self> {
        if scope_spec::is_configurable(&scope.resource, &scope.action) {
            return None;
        }
        Some(Self {
            resource: scope.resource.clone(),
            action: scope.action.clone(),
            reason: if scope.resource == "settlement_party" {
                "结算主体目录登录后即可查看，不再配置数据范围。".into()
            } else {
                scope_spec::action_description(&scope.resource, &scope.action)
                    .unwrap_or("当前资源未登记独立范围策略。")
                    .into()
            },
        })
    }
}

impl SavePersonScopeRequest {
    /// 将人员范围保存请求规范化为一次原子命令。去重动作和附加项后，按服务端登记逐条校验完整维度；任一项非法、为空或超限时拒绝整批。空附加项集合只保留基础范围。
    pub fn normalized(mut self) -> Result<PersonScopeCommand> {
        self.actions.sort();
        self.actions.dedup();
        if self.actions.is_empty() || self.actions.len() > 32 || self.grants.len() > 32 {
            return Err(Error::ValidationError("请选择操作，附加授权最多32项".into()));
        }
        for action in &self.actions {
            scope_spec::configurable_action(&self.resource, action)?;
        }
        ScopeBinding {
            schema_version: 2,
            resource: self.resource.clone(),
            actions: self.actions.clone(),
            target_dimension: ScopeDimension::InternalOrg,
            target_mode: None,
            include_descendants: None,
            enabled: true,
        }
        .validate(DataScopeType::Company, &[])?;

        let mut grants = Vec::new();
        let mut targets = Vec::new();
        for mut grant in self.grants {
            let grant_targets = grant.normalize(&self.resource, &self.actions)?;
            if !grants.contains(&grant) {
                targets.extend(grant_targets);
                grants.push(grant);
            }
        }

        self.grants = grants;
        Ok(PersonScopeCommand { request: self, targets })
    }

    /// 写事务按当前配置检查是否需要转换旧完整表达式。未明确确认时拒绝覆盖，避免静默加入本人范围或去掉旧上限。
    pub fn ensure_legacy_conversion(&self, existing: &[PersonDataScope]) -> Result<()> {
        if !self.replace_legacy
            && existing.iter().any(|scope| {
                scope.resource == self.resource
                    && self.actions.contains(&scope.action)
                    && !scope.expression.additive
            })
        {
            return Err(Error::ValidationError("存在旧范围配置，请明确确认转换为基础范围加附加授权".into()));
        }
        Ok(())
    }

    /// 为本次已验证保存动作提取适用的附加条款；未命中时保留空附加列表。本人基础不在这里写死，由运行时政策解析。
    pub fn expression(&self, action: &str) -> PersonScopeExpression {
        PersonScopeExpression {
            additive: true,
            history_read: false,
            alternatives: self
                .grants
                .iter()
                .filter(|grant| grant.actions.iter().any(|value| value == action))
                .map(|grant| grant.terms.clone())
                .collect(),
            condition: None,
        }
    }
}

impl PersonScopeGrant {
    // 保存范围前逐条规范化动作和目标；同一动作的必需维度必须在单条授权内成立，不拼接同批的其他条款。
    fn normalize(&mut self, resource: &str, actions: &[String]) -> Result<Vec<ScopeTarget>> {
        self.actions.sort();
        self.actions.dedup();
        if self.actions.is_empty()
            || self.actions.iter().any(|action| !actions.contains(action))
            || self.terms.is_empty()
            || self.terms.len() > 16
        {
            return Err(Error::ValidationError("附加授权必须选择本次保存的操作和完整范围".into()));
        }

        let mut keyed = Vec::new();
        let mut targets = Vec::new();
        for mut term in self.terms.drain(..) {
            if matches!(term.scope_type, DataScopeType::SelfOwned | DataScopeType::Collaborative)
                && term.target_dimension != ScopeDimension::InternalOrg
            {
                return Err(Error::ValidationError("本人及协作范围只适用于业务负责人维度".into()));
            }
            term.scope_targets.sort();
            term.scope_targets.dedup();
            let target = term.target(resource, &self.actions)?;
            scope_spec::validate_binding(&term.binding(resource, &self.actions))?;
            scope_spec::validate_scope_type(resource, term.scope_type)?;
            term.scope_targets = target.ids().to_vec();
            term.scope_targets.sort();
            if !targets.contains(&target) {
                targets.push(target);
            }
            keyed.push((serde_json::to_string(&term)?, term));
        }

        keyed.sort_by(|left, right| left.0.cmp(&right.0));
        keyed.dedup_by(|left, right| left.0 == right.0);
        self.terms = keyed.into_iter().map(|(_, term)| term).collect();
        self.validate_dimensions(resource)?;
        Ok(targets)
    }

    // 保存授权条目前，按所选动作逐一核对条款内的必需维度；同批其他条款不能补足该条款。
    fn validate_dimensions(&self, resource: &str) -> Result<()> {
        let company = self.terms.iter().any(|term| term.scope_type == DataScopeType::Company);
        for action in &self.actions {
            let spec = scope_spec::configurable_action(resource, action)?;
            if !company
                && spec
                    .required_dimensions()
                    .iter()
                    .any(|dimension| !self.terms.iter().any(|term| term.target_dimension == *dimension))
            {
                tracing::debug!(resource, action, "附加授权缺少必需维度");
                return Err(Error::ValidationError(
                    "附加授权的范围设置不完整，请补齐所选操作要求的范围".into(),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 为范围保存测试建立销售业务的按动作追加授权输入。
    fn request() -> SavePersonScopeRequest {
        SavePersonScopeRequest {
            resource: "sales_order".into(),
            actions: vec!["detail".into(), "update".into(), "detail".into()],
            grants: vec![PersonScopeGrant {
                actions: vec!["detail".into()],
                terms: vec![PersonScopeTerm {
                    scope_type: DataScopeType::SelfOwned,
                    target_dimension: ScopeDimension::InternalOrg,
                    target_mode: None,
                    include_descendants: None,
                    scope_targets: vec![],
                }],
            }],
            replace_legacy: false,
            expected_policy_version: 1,
        }
    }

    #[test]
    fn options_keep_role_actions_but_only_expose_configurable_subset() {
        let option = PersonBusinessOption::from_granted(
            "supplier_settlement_statement",
            vec!["list".into(), "confirm".into()],
        )
        .unwrap();
        assert_eq!(option.actions, vec!["list", "confirm"]);
        assert!(option.configurable_actions.is_empty());
        assert!(!option.default_self);
        assert!(option.policy_description.contains("list"));
        assert!(option.policy_description.contains("岗位共享"));
        assert!(option.policy_description.contains("confirm"));
        assert!(option.policy_description.contains("流程任务"));

        let task =
            PersonBusinessOption::from_granted("approval_instance", vec!["read".into(), "decide".into()])
                .unwrap();
        assert!(task.configurable_actions.is_empty());
        assert!(!task.default_self);
        assert!(task.policy_description.contains("流程任务"));
        assert!(PersonBusinessOption::from_granted("customer", vec![]).is_err());
    }

    #[test]
    fn retired_resource_or_mixed_retired_action_cannot_be_saved() {
        for (resource, actions) in [
            ("product", vec!["list", "create", "update"]),
            ("supplier", vec!["list", "update"]),
            ("supplier_offering", vec!["list", "update"]),
            ("purchase_order", vec!["list", "submit"]),
            ("settlement_party", vec!["list"]),
            ("approval_instance", vec!["decide"]),
            ("contract", vec!["list"]),
            ("supplier_settlement_statement", vec!["submit", "confirm"]),
            ("integration_error_task", vec!["list", "create"]),
        ] {
            let mut input = request();
            input.resource = resource.into();
            input.actions = actions.into_iter().map(String::from).collect();
            input.grants.clear();
            assert!(input.normalized().is_err());
        }

        let retired = PersonDataScope::default_for("alice", "approval_instance", "read");
        let retired = RetiredPersonScope::from_scope(&retired).unwrap();
        assert_eq!(retired.resource, "approval_instance");
        assert!(!retired.reason.is_empty());
        let active = PersonDataScope::default_for("alice", "customer", "list");
        assert!(RetiredPersonScope::from_scope(&active).is_none());
        let shared = PersonDataScope::default_for("alice", "product", "list");
        assert!(RetiredPersonScope::from_scope(&shared).unwrap().reason.contains("岗位共享"));
    }

    #[test]
    fn normalized_save_is_action_specific_and_allows_removing_all_grants() {
        let normalized = request().normalized().unwrap();
        assert_eq!(normalized.actions, vec!["detail", "update"]);
        assert_eq!(normalized.expression("detail").alternatives.len(), 1);
        assert!(normalized.expression("update").alternatives.is_empty());

        let mut empty = request();
        empty.grants.clear();
        assert!(empty.normalized().is_ok());
    }

    #[test]
    fn normalization_rejects_incomplete_branches_and_unselected_actions() {
        let mut incomplete = request();
        incomplete.resource = "warehouse".into();
        incomplete.actions = vec!["list".into()];
        incomplete.grants[0].actions = vec!["list".into()];
        assert!(incomplete.normalized().is_err());

        let mut invalid = request();
        invalid.grants[0].terms.clear();
        assert!(invalid.normalized().is_err());

        let mut invalid = request();
        invalid.grants[0].actions = vec!["delete".into()];
        assert!(invalid.normalized().is_err());

        let mut invalid = request();
        invalid.actions = vec!["*".into()];
        invalid.grants[0].actions = vec!["*".into()];
        assert!(invalid.normalized().is_err());
    }

    // 验证：每个授权条目按自己选择的动作验证完整维度，不能借用同批其他条目补齐。
    #[test]
    fn deposit_grants_require_both_dimensions_per_selected_action() {
        use crate::access_control::ScopeTargetMode;

        let org = PersonScopeTerm {
            scope_type: DataScopeType::Organization,
            target_dimension: ScopeDimension::InternalOrg,
            target_mode: Some(ScopeTargetMode::Explicit),
            include_descendants: Some(false),
            scope_targets: vec!["org-one".into()],
        };
        let party = PersonScopeTerm {
            target_dimension: ScopeDimension::SettlementParty,
            include_descendants: None,
            scope_targets: vec!["party-one".into()],
            ..org.clone()
        };

        let mut input = request();
        input.resource = "supplier_deposit".into();
        input.actions = vec!["list".into(), "resolve".into()];
        input.grants = vec![
            PersonScopeGrant { actions: vec!["list".into()], terms: vec![org.clone(), party.clone()] },
            PersonScopeGrant { actions: vec!["resolve".into()], terms: vec![org] },
        ];

        let mut invalid = input.grants[1].clone();
        let error = invalid.normalize(&input.resource, &input.actions).unwrap_err();
        assert!(error.to_string().contains("supplier_deposit:resolve"));

        input.grants[1].terms.push(party);
        let command = input.normalized().unwrap();
        assert_eq!(command.grants.len(), 2);
        assert!(command.grants.iter().all(|grant| grant.terms.len() == 2));
    }

    #[test]
    fn normalization_deduplicates_equivalent_grants_and_bounds_input() {
        let mut duplicate = request();
        duplicate.grants.push(duplicate.grants[0].clone());
        let command = duplicate.normalized().unwrap();
        assert_eq!(command.grants.len(), 1);
        assert_eq!(command.targets().len(), command.grants[0].terms.len());

        let mut over_limit = request();
        over_limit.grants = vec![over_limit.grants[0].clone(); 33];
        assert!(over_limit.normalized().is_err());
    }

    // 验证：保存表达式与事务校验必须使用相同的规范化身份，协议字段保持原形状。
    #[test]
    fn command_preserves_normalized_targets_and_storage_shape() {
        use crate::access_control::ScopeTargetMode;

        let mut input = request();
        input.grants[0].terms = vec![PersonScopeTerm {
            scope_type: DataScopeType::Organization,
            target_dimension: ScopeDimension::InternalOrg,
            target_mode: Some(ScopeTargetMode::Explicit),
            include_descendants: Some(false),
            scope_targets: vec![" one ".into(), "one".into()],
        }];
        let command = input.normalized().unwrap();
        assert_eq!(
            command.targets(),
            &[ScopeTarget::OrgExplicit { ids: vec!["one".into()], include_descendants: false }]
        );
        let expression = command.expression("detail");
        assert_eq!(expression.alternatives[0][0].scope_targets, ["one"]);
        let json = serde_json::to_value(&expression).unwrap();
        assert_eq!(
            json["alternatives"][0][0],
            serde_json::json!({
                "scope_type": "organization", "target_dimension": "internal_org",
                "target_mode": "explicit", "include_descendants": false,
                "scope_targets": ["one"],
            })
        );
        assert_eq!(serde_json::from_value::<PersonScopeExpression>(json).unwrap(), expression);
    }

    // 验证：服务端登记决定必需维度，调用者不能用空维度声明绕过检查。
    #[test]
    fn command_rejects_wrong_dimensions_and_role_only_mode() {
        use crate::access_control::ScopeTargetMode;

        for (dimension, mode, descendants) in [
            (ScopeDimension::Warehouse, ScopeTargetMode::OwnOrg, Some(false)),
            (ScopeDimension::InternalOrg, ScopeTargetMode::ManagedOrgs, None),
            (ScopeDimension::InternalOrg, ScopeTargetMode::OwnOrg, None),
        ] {
            let mut input = request();
            input.grants[0].terms = vec![PersonScopeTerm {
                scope_type: DataScopeType::Organization,
                target_dimension: dimension,
                target_mode: Some(mode),
                include_descendants: descendants,
                scope_targets: vec![],
            }];
            assert!(input.normalized().is_err());
        }
    }

    #[test]
    fn api_refuses_old_terms_and_untrusted_expression() {
        for extra in ["terms", "role_id", "expression"] {
            let mut value = serde_json::json!({"resource":"sales_order","actions":["detail"],"grants":[],"expected_policy_version":1});
            value[extra] = serde_json::json!([]);
            assert!(serde_json::from_value::<SavePersonScopeRequest>(value).is_err());
        }
    }
    #[test]
    fn legacy_conversion_is_explicit_and_only_checks_selected_operations() {
        let mut save = request();
        let mut old = PersonDataScope::default_for("alice", "sales_order", "detail");
        old.expression.additive = false;
        assert!(save.ensure_legacy_conversion(&[old.clone()]).is_err());
        save.replace_legacy = true;
        assert!(save.ensure_legacy_conversion(&[old.clone()]).is_ok());
        save.replace_legacy = false;
        old.action = "submit".into();
        assert!(save.ensure_legacy_conversion(&[old]).is_ok());
        save.grants.clear();
        save.actions = vec!["*".into()];
        assert!(save.normalized().is_err());
    }
    #[test]
    fn action_specific_grants_resolve_to_independent_unions() {
        use erp_core::common::time::Instant;

        use crate::access_control::{ScopeTargetMode, ScopedObject};
        use crate::entity::organization::{OrgUnit, OrgUnitKind};
        use crate::entity::organization_change::OrganizationState;

        let state = OrganizationState {
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
        };
        let grant = |id: &str, actions: &[&str]| PersonScopeGrant {
            actions: actions.iter().map(|action| (*action).into()).collect(),
            terms: vec![PersonScopeTerm {
                scope_type: DataScopeType::Organization,
                target_dimension: ScopeDimension::InternalOrg,
                target_mode: Some(ScopeTargetMode::Explicit),
                include_descendants: Some(false),
                scope_targets: vec![id.into()],
            }],
        };

        let mut save = request();
        save.actions = vec!["list".into(), "update".into(), "submit".into()];
        save.grants = vec![grant("one", &["list"]), grant("two", &["list", "update"])];
        let save = save.normalized().unwrap();

        for (action, expected) in
            [("list", [true, true]), ("update", [false, true]), ("submit", [false, false])]
        {
            let mut policy = PersonDataScope::default_for("alice", "sales_order", action);
            policy.expression = save.expression(action);
            let scope = policy.resolve(&state, Instant::now()).unwrap();
            let mut target = ScopedObject {
                owned: false,
                collaborating: false,
                historical_read_participant: false,
                org_unit_id: None,
                settlement_party_id: None,
                warehouse_id: None,
            };
            for (department, allowed) in ["one", "two"].into_iter().zip(expected) {
                target.org_unit_id = Some(department);
                assert_eq!(scope.allows(&target, action == "list"), allowed);
            }

            target.org_unit_id = Some("outside");
            target.owned = true;
            assert!(scope.allows(&target, action == "list"));
        }
    }
    // 验证：配置页面必须区分默认本人读取和显式管理范围，不依赖动作排列顺序。
    #[test]
    fn task_scope_options_expose_baseline_for_each_action() {
        for actions in [vec!["manage", "list", "detail"], vec!["detail", "manage", "list"]] {
            let option = PersonBusinessOption::from_granted(
                "work_item",
                actions.into_iter().map(String::from).collect(),
            )
            .unwrap();
            assert!(!option.default_self);
            assert_eq!(option.default_self_actions.len(), 2);
            assert!(option.default_self_actions.contains(&"list".to_owned()));
            assert!(option.default_self_actions.contains(&"detail".to_owned()));
            assert!(!option.default_self_actions.contains(&"manage".to_owned()));
        }
        let reader =
            PersonBusinessOption::from_granted("work_item", vec!["list".into(), "detail".into()]).unwrap();
        assert!(reader.default_self);
    }
}
