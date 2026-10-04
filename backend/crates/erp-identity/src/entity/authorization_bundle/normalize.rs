//! 文件规范化和静态范围准入。
use std::collections::BTreeSet;

use super::{PolicyDocument, PolicyMode, PolicyScope};
use crate::dto::person_scope::{PersonScopeGrant, SavePersonScopeRequest};
use crate::entity::access_control::person_scope::PersonScopeExpression;
use crate::entity::role::{ROOT_ROLE_ID, Role, RoleData};
use crate::service::access_control::consumers::configurable_registration;
use crate::{Error, Permission, Result};

impl PolicyDocument {
    /// 规范化文件，并拒绝含糊的管理边界。
    /// # 参数
    /// 无。
    /// # 返回
    /// 稳定排序、去重且结构有效的配置文件。
    /// # 错误
    /// 重复对象、重叠范围、非法角色、超限或不可配置动作时拒绝。
    pub fn normalized(mut self) -> Result<Self> {
        if self.roles.len() > 100 || self.bindings.len() > 100 || self.data_scopes.len() > 200 {
            return Err(Error::ValidationError("单文件最多100个角色、100个人员绑定和200项业务范围".into()));
        }
        let mut roles = BTreeSet::new();
        for role in &mut self.roles {
            if role.id.as_str() == ROOT_ROLE_ID || !roles.insert(role.id.clone()) {
                return Err(Error::ValidationError("系统角色或重复角色不能写入策略文件".into()));
            }
            role.name = Role::new(role.id.to_string(), RoleData::new(&role.name))?.name;
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

    /// 按人员、业务、动作拒绝重叠写入。
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

    /// 校验角色权限属于服务端当前注册目录。
    /// # 参数
    /// catalog 为当前运行版本生成的权限目录。
    /// # 返回
    /// 每个精确权限或通配模式均覆盖已登记权限时成功。
    /// # 错误
    /// 未登记权限、无匹配通配符或空目录时拒绝。
    pub fn validate_catalog(&self, catalog: &[Permission]) -> Result<()> {
        for permission in self.roles.iter().flat_map(|role| &role.permissions) {
            if !catalog.iter().any(|known| permission.covers(known)) {
                return Err(Error::ValidationError(format!("未登记的权限：{permission}")));
            }
        }
        Ok(())
    }

    /// 返回文件明确引用的人员，供有界仓储查询。
    /// # 参数
    /// 无。
    /// # 返回
    /// 排序去重后的人员 ID。
    /// # 错误
    /// 无；文件规范化阶段已校验 ID。
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
    /// 使用同一规则规范化动作、条件集合及授权项顺序。
    fn normalize_grants(&mut self) -> Result<()> {
        let request = self.request(0)?;
        self.actions = request.actions;
        let mut keyed = request
            .grants
            .into_iter()
            .map(|grant| {
                serde_json::to_string(&grant)
                    .map(|key| (key, grant))
                    .map_err(|error| Error::Internal(error.to_string()))
            })
            .collect::<Result<Vec<_>>>()?;
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        self.grants = keyed.into_iter().map(|(_, grant)| grant).collect();
        Ok(())
    }

    /// 将已存储和待保存的追加范围转为同一比较形式；保留旧表达式标志。
    /// # 参数
    /// action 为当前动作，expression 为该动作的范围表达式。
    /// # 返回
    /// 追加项、条件和目标均按集合规范化的表达式。
    /// # 错误
    /// 追加配置不满足正式范围模型时拒绝。
    pub(super) fn canonical_expression(
        &self,
        action: &str,
        expression: &PersonScopeExpression,
    ) -> Result<PersonScopeExpression> {
        if !expression.additive || expression.history_read || expression.condition.is_some() {
            return Ok(expression.clone());
        }
        let mut scope = Self {
            actions: vec![action.into()],
            grants: expression
                .alternatives
                .iter()
                .map(|terms| PersonScopeGrant { actions: vec![action.into()], terms: terms.clone() })
                .collect(),
            ..self.clone()
        };
        scope.normalize_grants()?;
        Ok(scope.request(0)?.expression(action))
    }

    /// 复用人员范围正式 DTO 的规范化规则。
    /// # 参数
    /// version 为当前策略版本。
    /// # 返回
    /// 可传入范围保存步骤的规范化命令。
    /// # 错误
    /// 非法动作、缺失维度、超限或范围准入失败时拒绝。
    pub fn request(&self, version: u64) -> Result<SavePersonScopeRequest> {
        let action = self.actions.first().ok_or_else(|| Error::ValidationError("请选择范围操作".into()))?;
        let registration = configurable_registration(&self.resource, action)?;
        SavePersonScopeRequest {
            resource: self.resource.clone(),
            actions: self.actions.clone(),
            grants: self.grants.clone(),
            replace_legacy: self.replace_legacy,
            expected_policy_version: version,
        }
        .normalized(registration.required_dimensions)
    }
}

/// 统一限制外部人员标识，拒绝空白及控制字符。
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

    /// 构造真实文件 DTO，测试执行生产规范化入口。
    fn document() -> PolicyDocument {
        serde_json::from_value(json!({
            "version": "1.0",
            "roles": [{"id":"policy-reader","name":"  只读岗位  ","mode":"replace","permissions":["sales_order:list","sales_order:list"]}],
            "bindings": [{"user_id":" alice ","mode":"merge","role_ids":["policy-reader","policy-reader"]}],
            "data_scopes": []
        })).unwrap()
    }

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

    #[test]
    fn root_role_and_oversized_input_are_rejected() {
        let mut value = document();
        value.roles[0].id = crate::RoleId::parse("role-root").unwrap();
        assert!(matches!(value.normalized(), Err(Error::ValidationError(_))));
        let mut value = document();
        value.bindings[0].user_id = "x".repeat(129);
        assert!(matches!(value.normalized(), Err(Error::ValidationError(_))));
        let mut value = document();
        value.roles = vec![value.roles[0].clone(); 101];
        assert!(matches!(value.normalized(), Err(Error::ValidationError(_))));
    }
}
