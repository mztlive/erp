//! 人员范围的查询和保存接口；按动作配置附加授权，基础范围由运行时策略决定。
use std::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::access_control::{DataScopeType, ScopeDimension};
use crate::entity::access_control::person_scope::{PersonDataScope, PersonScopeExpression, PersonScopeTerm};
use crate::entity::access_control::scope_spec;
use crate::entity::access_control::scope_target::ScopeTarget;
use crate::{Error, Result};

/// 替换同一业务所选动作的人员附加授权；拒绝未知请求字段。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavePersonScopeRequest {
    /// 业务资源标识。
    pub resource: String,
    /// 本次完整替换附加授权的动作；其余动作保持原样。
    pub actions: Vec<String>,
    /// 所选动作的新附加授权列表；传空列表可清除这些动作的附加授权。
    pub grants: Vec<PersonScopeGrant>,
    /// 保存时用于检测并发策略变更的预期版本。
    pub expected_policy_version: u64,
}

/// 已通过静态校验的人员范围保存命令；写事务另行核对资格和目标。
#[derive(Debug)]
pub struct PersonScopeCommand {
    // 已规范化的保存请求。
    request: SavePersonScopeRequest,
    // 从保留的授权项解析出、供写事务校验的范围目标。
    targets: Vec<ScopeTarget>,
}
impl Deref for PersonScopeCommand {
    type Target = SavePersonScopeRequest;
    // 以只读方式借出规范化请求，保护已校验的数据。
    fn deref(&self) -> &Self::Target {
        &self.request
    }
}
impl PersonScopeCommand {
    /// 按首次出现顺序借出写事务需要校验的显式目标。
    pub(crate) fn targets(&self) -> impl Iterator<Item = (ScopeDimension, &[String])> + '_ {
        self.targets.iter().filter_map(ScopeTarget::explicit)
    }
}

/// 一条独立的附加授权。同一动作适用的多条授权取并集。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PersonScopeGrant {
    /// 本条授权适用的动作，须从本次保存的动作中选择。
    pub actions: Vec<String>,
    /// 本条授权的范围项。公司范围覆盖全部；其余范围同维度取并集、不同维度取交集。
    pub terms: Vec<PersonScopeTerm>,
}

/// 某个业务的人员范围配置候选，按人员已有的动作资格生成。
#[derive(Serialize)]
pub struct PersonBusinessOption {
    /// 已具备资格且允许独立配置人员范围的动作。
    pub configurable_actions: Vec<String>,
    /// 各动作的策略说明，仅供展示。
    pub policy_description: String,
    /// 业务资源标识。
    pub resource: String,
    /// 人员具备资格的全部候选动作，含不可独立配置的动作。
    pub actions: Vec<String>,
    /// 可配置动作支持的范围维度并集；保存时逐动作校验。
    pub dimensions: Vec<ScopeDimension>,
    /// 至少有一个可配置动作且全部可配置动作均有本人基础时为 `true`。
    pub default_self: bool,
    /// 有本人基础的可配置动作，可用于区分各动作的默认范围。
    pub default_self_actions: Vec<String>,
}
/// 人员范围配置页的查询结果。
#[derive(Serialize)]
pub struct PersonScopeView {
    /// 已退役配置的资源、动作和退役原因。
    pub retired_items: Vec<RetiredPersonScope>,
    /// 已保存的人员范围记录，含保留用于审计的退役项。
    pub items: Vec<PersonDataScope>,
    /// 按业务分组的动作资格和范围配置候选。
    pub businesses: Vec<PersonBusinessOption>,
    /// 本次读取的策略版本，保存时作为预期版本提交。
    pub policy_version: u64,
}

impl PersonBusinessOption {
    /// 根据已确认的资源和动作资格生成配置候选及策略说明。
    /// 动作列表为空或包含未登记动作时返回校验错误。
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

/// 已不支持独立配置的人员范围摘要。原记录保留用于审计。
#[derive(Serialize)]
pub struct RetiredPersonScope {
    /// 原记录的业务资源标识。
    pub resource: String,
    /// 原记录的动作标识。
    pub action: String,
    /// 当前策略下无法继续独立配置的原因。
    pub reason: String,
}

impl RetiredPersonScope {
    /// 按当前策略检查已存储的 `scope`。仍可独立配置时返回 `None`。
    /// 其余记录返回退役说明，未知资源或动作也列为退役。
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
    /// 将请求规范化为一次原子保存命令；任一动作或授权非法时拒绝整批。
    /// 动作去重后须有 1 至 32 个，且均可独立配置；原始授权列表最多 32 条。
    /// 按服务端登记逐条校验并去除等价授权。空授权列表只保留各动作的基础范围。
    pub fn normalized(mut self) -> Result<PersonScopeCommand> {
        let mut targets = Vec::new();
        self.grants = normalize_grants(&self.resource, &mut self.actions, self.grants, Some(&mut targets))?;
        Ok(PersonScopeCommand { request: self, targets })
    }

    /// 为本次保存的指定动作生成附加表达式；无适用授权时附加列表为空。本人基础由运行时策略解析。
    pub fn expression(&self, action: &str) -> PersonScopeExpression {
        PersonScopeExpression {
            alternatives: self
                .grants
                .iter()
                .filter(|grant| grant.actions.iter().any(|value| value == action))
                .map(|grant| grant.terms.clone())
                .collect(),
        }
    }
}

// 文件、已有配置比较和交互保存共用静态校验；只有交互保存需保留原输入的校验顺序。
pub(crate) fn normalize_grants(
    resource: &str,
    actions: &mut Vec<String>,
    grants: Vec<PersonScopeGrant>,
    mut targets: Option<&mut Vec<ScopeTarget>>,
) -> Result<Vec<PersonScopeGrant>> {
    actions.sort();
    actions.dedup();
    if actions.is_empty() || actions.len() > 32 || grants.len() > 32 {
        return Err(Error::ValidationError("请选择操作，附加授权最多32项".into()));
    }
    for action in actions.iter() {
        scope_spec::configurable_action(resource, action)?;
    }

    let mut normalized = Vec::new();
    for mut grant in grants {
        let grant_targets = grant.normalize(resource, actions, targets.is_some())?;
        if !normalized.contains(&grant) {
            if let Some(targets) = targets.as_deref_mut() {
                targets.extend(grant_targets);
            }
            normalized.push(grant);
        }
    }
    Ok(normalized)
}

impl PersonScopeGrant {
    // 动作去重后须为本次保存动作的非空子集；每条授权须有 1 至 16 个原始范围项。
    // 逐项校验范围类型、动作支持的维度和目标形态，拒绝角色专用的管理部门模式。
    // 本人及协作范围仅用于内部组织维度。范围项规范化并去重后，校验必需维度并返回目标。
    fn normalize(
        &mut self,
        resource: &str,
        actions: &[String],
        keep_target_order: bool,
    ) -> Result<Vec<ScopeTarget>> {
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
            let ids = std::mem::take(&mut term.scope_targets);
            let target = ScopeTarget::parse_with_ids(&term, ids)?;
            scope_spec::validate_target_dimension(resource, &self.actions, term.target_dimension)?;
            scope_spec::validate_scope_type(resource, term.scope_type)?;
            term.scope_targets = if let Some((_, ids)) = target.explicit().filter(|_| keep_target_order) {
                let stored_ids = ids.to_vec();
                if !targets.contains(&target) {
                    targets.push(target);
                }
                stored_ids
            } else {
                target.into_ids()
            };
            term.scope_targets.sort();
            keyed.push((serde_json::to_string(&term)?, term));
        }

        keyed.sort_by(|left, right| left.0.cmp(&right.0));
        keyed.dedup_by(|left, right| left.0 == right.0);
        self.terms = keyed.into_iter().map(|(_, term)| term).collect();
        self.validate_dimensions(resource)?;
        Ok(targets)
    }

    // 按动作检查本条授权是否覆盖必需维度；含公司范围时免查，各条授权独立校验。
    fn validate_dimensions(&self, resource: &str) -> Result<()> {
        let company = self.terms.iter().any(|term| term.scope_type == DataScopeType::Company);
        for action in &self.actions {
            let spec = scope_spec::configurable_action(resource, action)?;
            if !company
                && spec
                    .dimensions()
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

    // 构造销售单保存请求：重复选择 `detail`，本人附加授权仅适用于 `detail`。
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
            expected_policy_version: 1,
        }
    }

    // 角色动作全部保留供展示；岗位共享和流程任务动作不可独立编辑，空动作列表报错。
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

    // 选中不可配置动作时拒绝保存；旧记录显示退役原因，仍可配置的记录不列为退役。
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

    // 重复动作会去重；只有 `detail` 获得附加授权，传空授权列表也可保存。
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

    // 拒绝缺少仓库维度、无范围项、授权动作未被选中及使用通配动作的请求。
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

    // 验证供应商预存款的 `list` 和 `resolve` 各需完整的内部组织与结算主体维度，不能借用其他授权项。
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
        let error = invalid.normalize(&input.resource, &input.actions, false).unwrap_err();
        assert!(matches!(
            error,
            Error::ValidationError(message) if message == "附加授权的范围设置不完整，请补齐所选操作要求的范围"
        ));

        input.grants[1].terms.push(party);
        let command = input.normalized().unwrap();
        assert_eq!(command.grants.len(), 2);
        assert!(command.grants.iter().all(|grant| grant.terms.len() == 2));
    }

    // 等价授权合并为一条；即使 33 条授权完全相同，原始输入仍超限。
    #[test]
    fn normalization_deduplicates_equivalent_grants_and_bounds_input() {
        let mut duplicate = request();
        duplicate.grants.push(duplicate.grants[0].clone());
        let command = duplicate.normalized().unwrap();
        assert_eq!(command.grants.len(), 1);
        assert_eq!(command.targets().count(), 0);

        let mut over_limit = request();
        over_limit.grants = vec![over_limit.grants[0].clone(); 33];
        assert!(over_limit.normalized().is_err());

        let mut over_limit = request();
        over_limit.grants[0].terms = vec![over_limit.grants[0].terms[0].clone(); 17];
        assert!(over_limit.normalized().is_err());
    }

    // 事务校验和保存表达式共用规范化目标 ID；表达式按原协议字段往返序列化。
    #[test]
    fn command_preserves_normalized_targets_and_storage_shape() {
        use crate::access_control::ScopeTargetMode;

        let mut input = request();
        let org = PersonScopeTerm {
            scope_type: DataScopeType::Organization,
            target_dimension: ScopeDimension::InternalOrg,
            target_mode: Some(ScopeTargetMode::Explicit),
            include_descendants: Some(false),
            scope_targets: vec![" two ".into(), "one".into(), "two".into()],
        };
        input.grants[0].terms.extend([org.clone(), org]);
        let command = input.normalized().unwrap();
        let targets = command.targets().collect::<Vec<_>>();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].0, ScopeDimension::InternalOrg);
        assert_eq!(targets[0].1, ["two", "one"]);
        let expression = command.expression("detail");
        assert_eq!(expression.alternatives[0].len(), 2);
        assert_eq!(expression.alternatives[0][0].scope_targets, ["one", "two"]);
        assert_eq!(expression.alternatives[0][1].scope_type, DataScopeType::SelfOwned);
        let json = serde_json::to_value(&expression).unwrap();
        assert_eq!(
            json["alternatives"][0][0],
            serde_json::json!({
                "scope_type": "organization", "target_dimension": "internal_org",
                "target_mode": "explicit", "include_descendants": false,
                "scope_targets": ["one", "two"],
            })
        );
        assert_eq!(serde_json::from_value::<PersonScopeExpression>(json).unwrap(), expression);
    }

    // 合法的显式仓库目标也不能用于不支持仓库维度的动作。
    #[test]
    fn command_checks_the_target_dimension_against_the_grant_actions() {
        let mut input = request();
        input.grants[0].terms = vec![PersonScopeTerm {
            scope_type: DataScopeType::Organization,
            target_dimension: ScopeDimension::Warehouse,
            target_mode: Some(crate::access_control::ScopeTargetMode::Explicit),
            include_descendants: None,
            scope_targets: vec!["warehouse-one".into()],
        }];
        assert!(matches!(
            input.normalized(),
            Err(Error::ValidationError(message)) if message == "所选操作不支持按仓库设置数据范围，请调整授权范围"
        ));
    }

    // 拒绝错误维度及缺少下级标记的本人组织模式。
    #[test]
    fn command_rejects_wrong_dimensions_and_missing_descendants() {
        use crate::access_control::ScopeTargetMode;

        for (dimension, mode, descendants) in [
            (ScopeDimension::Warehouse, ScopeTargetMode::OwnOrg, Some(false)),
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

    // 反序列化保存请求时拒绝顶层 `terms`、`role_id`、`expression` 和 `replace_legacy` 字段。
    #[test]
    fn api_refuses_old_terms_and_untrusted_expression() {
        for extra in ["terms", "role_id", "expression", "replace_legacy"] {
            let mut value = serde_json::json!({"resource":"sales_order","actions":["detail"],"grants":[],"expected_policy_version":1});
            value[extra] = serde_json::json!([]);
            assert!(serde_json::from_value::<SavePersonScopeRequest>(value).is_err());
        }
    }
    // 销售单的 `list`、`update`、`submit` 分别合并适用部门授权，均保留本人基础。
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
    // 工作项 `list`、`detail` 默认本人，`manage` 须显式配置范围；汇总结果与动作顺序无关。
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
