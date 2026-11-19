//! 文件规范化和静态范围准入。
use std::collections::BTreeSet;

use super::{PolicyDocument, PolicyMode, PolicyScope};
use crate::dto::person_scope::{PersonScopeCommand, PersonScopeGrant, SavePersonScopeRequest};
use crate::entity::access_control::person_scope::PersonScopeExpression;
use crate::entity::access_control::scope_target::ScopeTarget;
use crate::entity::role::{ROOT_ROLE_ID, Role, RoleData};
use crate::{Error, Permission, Result};

impl PolicyDocument {
    /// 在预览和应用前规范化标识和值集合，按管理对象稳定排序。
    /// 拒绝重复对象、重叠范围、非法角色、不可配置动作及超出输入上限的文件。
    pub fn normalized(mut self) -> Result<Self> {
        if self.roles.len() > 100 || self.bindings.len() > 100 || self.data_scopes.len() > 200 {
            return Err(Error::ValidationError("单文件最多100个角色、100个人员绑定和200项业务范围".into()));
        }

        let mut roles = BTreeSet::new();
        for role in &mut self.roles {
            if role.id.as_str() == ROOT_ROLE_ID || !roles.insert(role.id.clone()) {
                return Err(Error::ValidationError("系统角色或重复角色不能写入策略文件".into()));
            }
            role.name = Role::new(role.id.clone(), RoleData::new(&role.name))?.name;
            if role.permissions.len() > 1000 {
                return Err(Error::ValidationError("单个角色最多1000项权限".into()));
            }
            role.permissions.sort();
            role.permissions.dedup();
        }

        let mut users = BTreeSet::new();
        for binding in &mut self.bindings {
            binding.user_id = normalize_id(&binding.user_id)?;
            if !users.insert(binding.user_id.clone()) || binding.role_ids.len() > 100 {
                return Err(Error::ValidationError("重复人员绑定或角色数超过100".into()));
            }
            binding.role_ids.sort();
            binding.role_ids.dedup();
            if binding.role_ids.iter().any(|id| id.as_str() == ROOT_ROLE_ID) {
                return Err(Error::ValidationError("系统角色不能通过策略文件分配".into()));
            }
        }

        self.normalize_scopes()?;

        self.roles.sort_by(|a, b| a.id.cmp(&b.id));
        self.bindings.sort_by(|a, b| a.user_id.cmp(&b.user_id));
        self.data_scopes.sort_by(|a, b| {
            (&a.user_id, &a.resource, &a.actions).cmp(&(&b.user_id, &b.resource, &b.actions))
        });
        Ok(self)
    }

    // 规范化人员范围条目，要求显式替换，并检查同批人员、资源和动作的替换边界互斥。
    fn normalize_scopes(&mut self) -> Result<()> {
        let mut keys = BTreeSet::new();
        for scope in &mut self.data_scopes {
            scope.user_id = normalize_id(&scope.user_id)?;
            if scope.mode != PolicyMode::Replace {
                return Err(Error::ValidationError("人员范围必须显式使用 replace".into()));
            }
            scope.normalize_grants()?;
            for action in &scope.actions {
                if !keys.insert((scope.user_id.clone(), scope.resource.clone(), action.clone())) {
                    return Err(Error::ValidationError("人员资源动作重复，替换边界重叠".into()));
                }
            }
        }
        Ok(())
    }

    /// 对照当前运行版本的权限目录，要求文件中的每项精确权限或通配模式至少覆盖一项登记权限。
    /// 目录为空时拒绝包含任何权限声明的文件。
    pub fn validate_catalog(&self, catalog: &[Permission]) -> Result<()> {
        for permission in self.roles.iter().flat_map(|role| &role.permissions) {
            if !catalog.iter().any(|known| permission.covers(known)) {
                return Err(Error::ValidationError(format!("未登记的权限：{permission}")));
            }
        }
        Ok(())
    }

    /// 汇总绑定和范围条目引用的人员 ID，排序去重后供仓储定向查询；标识校验由规范化阶段负责。
    pub fn user_ids(&self) -> Vec<String> {
        self.bindings
            .iter()
            .map(|b| b.user_id.clone())
            .chain(self.data_scopes.iter().map(|s| s.user_id.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

impl PolicyScope {
    // 复用范围保存命令规范化动作和条款，再按条款的序列化值排序，使比较与审核摘要保持稳定。
    fn normalize_grants(&mut self) -> Result<()> {
        let request = self.request(0)?.into_request();
        self.actions = request.actions;
        let mut keyed = request
            .grants
            .into_iter()
            .map(|grant| Ok((serde_json::to_string(&grant)?, grant)))
            .collect::<Result<Vec<_>>>()?;
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        self.grants = keyed.into_iter().map(|(_, grant)| grant).collect();
        Ok(())
    }

    // 按已规范化文件中的授权、条款顺序重建目标；同一授权内去重。
    pub(crate) fn targets(&self) -> Result<Vec<ScopeTarget>> {
        let mut targets = Vec::new();
        for grant in &self.grants {
            let start = targets.len();
            for term in &grant.terms {
                let target = term.target(&self.resource, &grant.actions)?;
                if !targets[start..].contains(&target) {
                    targets.push(target);
                }
            }
        }
        Ok(targets)
    }

    /// 从已规范化的文件生成逐动作表达式。投影后归并相同条款，并沿用单动作授权的序列化顺序。
    pub(super) fn expression(&self, action: &str) -> Result<PersonScopeExpression> {
        action_expression(&self.grants, action)
    }

    /// 对照文件前重新校验已存的追加范围。旧完整表达式及旧历史或交集字段保持原样。
    pub(super) fn canonical_existing_expression(
        &self,
        action: &str,
        expression: &PersonScopeExpression,
    ) -> Result<PersonScopeExpression> {
        if !expression.additive || expression.history_read || expression.condition.is_some() {
            return Ok(expression.clone());
        }
        let command = SavePersonScopeRequest {
            resource: self.resource.clone(),
            actions: vec![action.into()],
            grants: expression
                .alternatives
                .iter()
                .map(|terms| PersonScopeGrant { actions: vec![action.into()], terms: terms.clone() })
                .collect(),
            replace_legacy: self.replace_legacy,
            expected_policy_version: 0,
        }
        .normalized()?;
        action_expression(&command.grants, action)
    }

    /// 用传入的策略版本构造正式的人员范围保存命令，供导入校验和提取逐动作表达式。
    /// 非法动作、缺失必需维度、超限或违反范围准入规则时拒绝。
    pub fn request(&self, version: u64) -> Result<PersonScopeCommand> {
        SavePersonScopeRequest {
            resource: self.resource.clone(),
            actions: self.actions.clone(),
            grants: self.grants.clone(),
            replace_legacy: self.replace_legacy,
            expected_policy_version: version,
        }
        .normalized()
    }
}

fn action_expression(grants: &[PersonScopeGrant], action: &str) -> Result<PersonScopeExpression> {
    // 投影为同一动作后，授权 JSON 中只有 terms 不同，按它排序与原顺序等价。
    let mut keyed = grants
        .iter()
        .filter(|grant| grant.actions.iter().any(|value| value == action))
        .map(|grant| Ok((serde_json::to_string(&grant.terms)?, &grant.terms)))
        .collect::<Result<Vec<_>>>()?;
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    keyed.dedup_by(|a, b| a.0 == b.0);
    Ok(PersonScopeExpression {
        additive: true,
        alternatives: keyed.into_iter().map(|(_, terms)| terms.clone()).collect(),
        ..Default::default()
    })
}

/// 去除外部人员 ID 的首尾空白；结果为空、超过 128 字节或含控制字符时返回校验错误。
pub(super) fn normalize_id(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return Err(Error::ValidationError("人员ID为空、过长或包含控制字符".into()));
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::RoleId;

    // 从文件输入构造带首尾空白和重复集合值的测试文档，供规范化断言使用。
    fn document() -> PolicyDocument {
        serde_json::from_value(json!({
            "version": "1.0",
            "roles": [{"id":"policy-reader","name":"  只读岗位  ","mode":"replace","permissions":["sales_order:list","sales_order:list"]}],
            "bindings": [{"user_id":" alice ","mode":"merge","role_ids":["policy-reader","policy-reader"]}],
            "data_scopes": []
        })).unwrap()
    }

    // 验证规范化裁剪名称和账号 ID、去重集合，并保留显式模式；重复规范化结果一致。
    #[test]
    fn normalize_is_deterministic_and_preserves_explicit_modes() {
        let value = document().normalized().unwrap();
        assert_eq!(value.roles[0].name, "只读岗位");
        assert_eq!(value.roles[0].permissions.len(), 1);
        assert_eq!(value.bindings[0].user_id, "alice");
        assert_eq!(value.bindings[0].role_ids.len(), 1);
        assert_eq!(value.bindings[0].mode, PolicyMode::Merge);
        assert_eq!(value.clone().normalized().unwrap(), value);
    }

    #[test]
    fn normalized_targets_match_save_command_order() {
        let mut value = document();
        value.data_scopes.push(serde_json::from_value(json!({
            "user_id":"alice", "resource":"sales_order", "actions":["list", "detail"], "mode":"replace",
            "grants":[
                {"actions":["list"], "terms":[
                    {"scope_type":"team", "target_dimension":"internal_org", "target_mode":"explicit",
                     "include_descendants":false, "scope_targets":[" west ", "east"]},
                    {"scope_type":"organization", "target_dimension":"internal_org", "target_mode":"explicit",
                     "include_descendants":false, "scope_targets":["east", "west"]}
                ]},
                {"actions":["detail"], "terms":[
                    {"scope_type":"organization", "target_dimension":"internal_org", "target_mode":"explicit",
                     "include_descendants":true, "scope_targets":["north"]}
                ]}
            ]
        })).unwrap());
        let normalized = value.normalized().unwrap();
        let scope = &normalized.data_scopes[0];
        let targets = scope.targets().unwrap();
        assert_eq!(targets, scope.request(0).unwrap().targets());
        assert_eq!(
            targets.iter().map(ScopeTarget::ids).collect::<Vec<_>>(),
            [&["north"][..], &["east", "west"]]
        );
    }

    // 验证重复角色在规范化时失败，绑定期限和拒绝效果等未知字段在反序列化时失败。
    #[test]
    fn duplicate_objects_and_unknown_fields_fail_closed() {
        let mut value = document();
        value.roles.push(value.roles[0].clone());
        assert!(matches!(value.normalized(), Err(Error::ValidationError(_))));

        let mut value = serde_json::to_value(document()).unwrap();
        value["bindings"][0]["effective_to"] = json!(10);
        assert!(serde_json::from_value::<PolicyDocument>(value).is_err());

        let mut value = serde_json::to_value(document()).unwrap();
        value["roles"][0]["effect"] = json!("deny");
        assert!(serde_json::from_value::<PolicyDocument>(value).is_err());
    }

    // 验证精确权限和命中目录的通配模式可用，拼写错误和无匹配通配模式被拒绝。
    #[test]
    fn catalog_requires_registered_permission_or_matching_wildcard() {
        let catalog = vec![Permission::parse("sales_order:list").unwrap()];
        assert!(document().normalized().unwrap().validate_catalog(&catalog).is_ok());
        let mut value = document();
        value.roles[0].permissions = vec![Permission::parse("sales_order:*").unwrap()];
        assert!(value.validate_catalog(&catalog).is_ok());
        value.roles[0].permissions = vec![Permission::parse("unknown:*").unwrap()];
        assert!(matches!(value.validate_catalog(&catalog), Err(Error::ValidationError(_))));
        value.roles[0].permissions = vec![Permission::parse("sales_order:typo").unwrap()];
        assert!(matches!(value.validate_catalog(&catalog), Err(Error::ValidationError(_))));
    }

    // 验证同一人员资源动作的重复替换和不可配置的审批动作都无法通过规范化。
    #[test]
    fn overlapping_scope_actions_and_nonconfigurable_resources_are_rejected() {
        let scope = json!({
            "user_id":"alice","resource":"sales_order","actions":["list"],"mode":"replace","grants":[]
        });
        let mut value = document();
        value.data_scopes =
            vec![serde_json::from_value(scope.clone()).unwrap(), serde_json::from_value(scope).unwrap()];
        assert!(matches!(value.normalized(), Err(Error::ValidationError(_))));

        let mut value = document();
        value.data_scopes = vec![serde_json::from_value(json!({
            "user_id":"alice","resource":"approval_instance","actions":["decide"],"mode":"replace","grants":[]
        })).unwrap()];
        assert!(matches!(value.normalized(), Err(Error::ValidationError(_))));
    }

    // 验证系统根角色、超长人员 ID 和超过单文件上限的角色条目被拒绝。
    #[test]
    fn root_role_and_oversized_input_are_rejected() {
        let mut value = document();
        value.roles[0].id = RoleId::parse("role-root").unwrap();
        assert!(matches!(value.normalized(), Err(Error::ValidationError(_))));

        let mut value = document();
        value.bindings[0].user_id = "x".repeat(129);
        assert!(matches!(value.normalized(), Err(Error::ValidationError(_))));

        let mut value = document();
        value.roles = vec![value.roles[0].clone(); 101];
        assert!(matches!(value.normalized(), Err(Error::ValidationError(_))));
    }
}
