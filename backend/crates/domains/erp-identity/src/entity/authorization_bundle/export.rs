//! 显式配置的无损导出规则。
use super::plan::{PolicyFacts, ensure_role};
use super::{PolicyBinding, PolicyDocument, PolicyMode, PolicyRole, PolicyScope, PolicyVersion};
use crate::dto::authorization_bundle::PolicyExport;
use crate::dto::person_scope::PersonScopeGrant;
use crate::entity::access_control::person_scope::PersonDataScope;
use crate::entity::access_control::scope_spec;
use crate::{Error, Permission, PermissionSet, Result, RoleId};

impl PolicyFacts {
    /// 从同一快照的完整事实和策略版本生成可重新导入的规范化文件，并附上仍由服务端计算的规则说明。
    /// 角色失效、动作资格缺失、旧范围无法无损表达或超出文件上限时拒绝导出。
    pub(crate) fn export(&self, policy_version: u64) -> Result<PolicyExport> {
        let mut document = PolicyDocument {
            version: PolicyVersion::V1,
            roles: vec![],
            bindings: vec![],
            data_scopes: vec![],
        };

        for (id, state) in &self.roles {
            ensure_role(&state.role)?;
            document.roles.push(PolicyRole {
                id: RoleId::parse(id)?,
                name: state.role.name.clone(),
                mode: PolicyMode::Replace,
                permissions: state.permissions.clone(),
            });
        }

        for (id, user) in &self.users {
            if user.role_ids.iter().any(|role| !self.roles.contains_key(role.as_str())) {
                return Err(Error::ValidationError("人员绑定含失效角色，不能生成完整导出".into()));
            }
            document.bindings.push(PolicyBinding {
                user_id: id.clone(),
                mode: PolicyMode::Replace,
                role_ids: user.role_ids.clone(),
            });
        }

        let mut notes = vec![
            "默认本人由业务当前负责人及资源登记计算；无追加项的有效动作显式导出空 grants。".into(),
            "来源继承、任务指派、审批节点、职责分离和业务状态继续执行服务端规则。".into(),
            "角色绑定沿用岗位目录资格初始化；解绑不自动终止已有人员查询资格。".into(),
        ];
        self.export_scopes(&mut document, &mut notes)?;
        self.export_default_scopes(&mut document)?;
        Ok(PolicyExport { document: document.normalized()?, policy_version, policy_notes: notes })
    }

    // 为当前有资格但未配置范围的可配置动作导出空替换项，重导时可清除后来追加的范围。
    fn export_default_scopes(&self, document: &mut PolicyDocument) -> Result<()> {
        for (user_id, user) in &self.users {
            let permissions = PermissionSet::new(
                user.role_ids
                    .iter()
                    .filter_map(|id| self.roles.get(id.as_str()))
                    .flat_map(|role| role.permissions.iter().cloned()),
            );

            for resource in scope_spec::resources() {
                let mut missing = Vec::new();
                for permission in scope_spec::actions(resource) {
                    let action = permission.action();
                    let configured = self.scopes.iter().any(|scope| {
                        scope.user_id == *user_id && scope.resource == resource && scope.action == action
                    });
                    if !configured
                        && scope_spec::is_configurable(resource, action)
                        && permissions.covers_one(permission)
                    {
                        missing.push(action.to_owned());
                    }
                }

                if !missing.is_empty() {
                    document.data_scopes.push(PolicyScope {
                        user_id: user_id.clone(),
                        resource: resource.into(),
                        actions: missing,
                        mode: PolicyMode::Replace,
                        grants: vec![],
                    });
                }
            }
        }
        Ok(())
    }

    // 逐动作导出范围，保留条款内的交集和条款间的并集；已退役配置仅写入说明。
    fn export_scopes(&self, document: &mut PolicyDocument, notes: &mut Vec<String>) -> Result<()> {
        for scope in &self.scopes {
            if !scope_spec::is_configurable(&scope.resource, &scope.action) {
                notes.push(format!(
                    "已退役配置未参与导出：{}/{}/{}",
                    scope.user_id, scope.resource, scope.action
                ));
                continue;
            }
            self.ensure_scope_qualification(scope)?;
            let expression = &scope.expression;

            document.data_scopes.push(PolicyScope {
                user_id: scope.user_id.clone(),
                resource: scope.resource.clone(),
                actions: vec![scope.action.clone()],
                mode: PolicyMode::Replace,
                grants: expression
                    .alternatives
                    .iter()
                    .map(|terms| PersonScopeGrant {
                        actions: vec![scope.action.clone()],
                        terms: terms.clone(),
                    })
                    .collect(),
            });
        }
        Ok(())
    }

    // 导出已有范围前检查绑定角色能否覆盖对应动作，拒绝生成含休眠配置的文件。
    fn ensure_scope_qualification(&self, scope: &PersonDataScope) -> Result<()> {
        let user = self.users.get(&scope.user_id).ok_or_else(|| Error::NotFound("范围人员不存在".into()))?;
        let required = Permission::parse(format!("{}:{}", scope.resource, scope.action))?;
        let permitted = user
            .role_ids
            .iter()
            .filter_map(|id| self.roles.get(id.as_str()))
            .flat_map(|role| &role.permissions)
            .any(|permission| permission.covers(&required));
        if !permitted {
            return Err(Error::ValidationError(format!(
                "人员 {} 已保存的 {required} 范围缺少当前动作资格，请先治理后导出",
                scope.user_id
            )));
        }
        Ok(())
    }
}
