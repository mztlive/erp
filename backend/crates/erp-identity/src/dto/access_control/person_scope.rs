//! 人员基础范围与按操作合并的附加授权接口。
use serde::{Deserialize, Serialize};

use crate::access_control::{DataScopeType, ScopeBinding, ScopeDimension};
use crate::entity::access_control::person_scope::{PersonDataScope, PersonScopeExpression, PersonScopeTerm};
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PersonScopeGrant {
    pub actions: Vec<String>,
    pub terms: Vec<PersonScopeTerm>,
}

#[derive(Serialize)]
pub struct PersonBusinessOption {
    pub resource: String,
    pub actions: Vec<String>,
    pub dimensions: Vec<ScopeDimension>,
    pub default_self: bool,
}
#[derive(Serialize)]
pub struct PersonScopeView {
    pub items: Vec<PersonDataScope>,
    pub businesses: Vec<PersonBusinessOption>,
    pub policy_version: u64,
}

impl SavePersonScopeRequest {
    /// 规范化动作和附加项，并独立检查每项的全部必需维度。
    /// # 参数
    /// * `dimensions` - 消费者要求的维度。
    /// # 返回
    /// 去重后的原子保存命令；空附加项集合表示仅保留基础范围。
    /// # 错误
    /// 无效动作、超限、空授权条目或任一条目缺少维度时拒绝整次保存。
    pub fn normalized(mut self, dimensions: &[ScopeDimension]) -> Result<Self> {
        self.actions.sort();
        self.actions.dedup();
        if self.actions.is_empty() || self.actions.len() > 32 || self.grants.len() > 32 {
            return Err(Error::ValidationError("请选择操作，附加授权最多32项".into()));
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
        for mut grant in self.grants {
            grant.normalize(&self.resource, &self.actions, dimensions)?;
            if !grants.contains(&grant) {
                grants.push(grant);
            }
        }
        self.grants = grants;
        Ok(self)
    }

    /// 旧完整表达式必须经明确转换确认才允许改为基础加附加授权。
    /// # 参数
    /// * `existing` - 在保存事务中取得的当前配置。
    /// # 返回
    /// 本次不涉及旧配置，或已明确允许转换时成功。
    /// # 错误
    /// 未确认覆盖旧配置时拒绝，避免静默加入本人范围或去除旧上限。
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

    /// 按操作选取附加授权，基础本人由解析时的业务政策计算。
    /// # 参数
    /// * `action` - 已验证属于本次保存的操作。
    /// # 返回
    /// 单个操作的完整新模型表达式。
    /// # 错误
    /// 无；未命中附加项时返回空附加列表。
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
    /// 每个授权条目独立完成规范化，禁止在多个不完整条目之间拼出完整授权。
    fn normalize(&mut self, resource: &str, actions: &[String], dimensions: &[ScopeDimension]) -> Result<()> {
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
        for term in &mut self.terms {
            if matches!(term.scope_type, DataScopeType::SelfOwned | DataScopeType::Collaborative)
                && term.target_dimension != ScopeDimension::InternalOrg
            {
                return Err(Error::ValidationError("本人及协作范围只适用于业务负责人维度".into()));
            }
            term.scope_targets.sort();
            term.scope_targets.dedup();
            for action in &self.actions {
                term.rule(resource, action, "validation", true)?;
            }
            keyed.push((
                serde_json::to_string(term).map_err(|error| Error::Internal(error.to_string()))?,
                term.clone(),
            ));
        }
        keyed.sort_by(|left, right| left.0.cmp(&right.0));
        keyed.dedup_by(|left, right| left.0 == right.0);
        self.terms = keyed.into_iter().map(|(_, term)| term).collect();
        let company = self.terms.iter().any(|term| term.scope_type == DataScopeType::Company);
        if !company && dimensions.iter().any(|d| !self.terms.iter().any(|term| term.target_dimension == *d)) {
            return Err(Error::ValidationError("每项附加授权均须配置此业务全部必需维度".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造销售业务的逐操作附加授权请求。
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
    fn normalized_save_is_action_specific_and_allows_removing_all_grants() {
        let normalized = request().normalized(&[ScopeDimension::InternalOrg]).unwrap();
        assert_eq!(normalized.actions, vec!["detail", "update"]);
        assert_eq!(normalized.expression("detail").alternatives.len(), 1);
        assert!(normalized.expression("update").alternatives.is_empty());
        let mut empty = request();
        empty.grants.clear();
        assert!(empty.normalized(&[ScopeDimension::InternalOrg]).is_ok());
    }

    #[test]
    fn normalization_rejects_incomplete_branches_and_unselected_actions() {
        assert!(request().normalized(&[ScopeDimension::InternalOrg, ScopeDimension::Warehouse]).is_err());
        let mut invalid = request();
        invalid.grants[0].terms.clear();
        assert!(invalid.normalized(&[]).is_err());
        let mut invalid = request();
        invalid.grants[0].actions = vec!["delete".into()];
        assert!(invalid.normalized(&[]).is_err());
        let mut invalid = request();
        invalid.actions = vec!["*".into()];
        invalid.grants[0].actions = vec!["*".into()];
        assert!(invalid.normalized(&[]).is_err());
    }

    #[test]
    fn normalization_deduplicates_equivalent_grants_and_bounds_input() {
        let mut duplicate = request();
        duplicate.grants.push(duplicate.grants[0].clone());
        assert_eq!(duplicate.normalized(&[ScopeDimension::InternalOrg]).unwrap().grants.len(), 1);
        let mut over_limit = request();
        over_limit.grants = vec![over_limit.grants[0].clone(); 33];
        assert!(over_limit.normalized(&[]).is_err());
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
        assert!(save.normalized(&[]).is_err());
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
        let save = save.normalized(&[ScopeDimension::InternalOrg]).unwrap();
        for (action, expected) in
            [("list", [true, true]), ("update", [false, true]), ("submit", [false, false])]
        {
            let mut policy = PersonDataScope::default_for("alice", "sales_order", action);
            policy.expression = save.expression(action);
            let scope = policy
                .resolve(&state, &[ScopeDimension::InternalOrg], action == "list", Instant::now())
                .unwrap();
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
}
