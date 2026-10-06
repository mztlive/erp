//! 显式配置的无损导出规则。
use super::plan::{PolicyFacts, ensure_role};
use super::{PolicyBinding, PolicyDocument, PolicyMode, PolicyRole, PolicyScope, PolicyVersion};
use crate::dto::authorization_bundle::PolicyExport;
use crate::dto::person_scope::PersonScopeGrant;
use crate::entity::access_control::person_scope::PersonDataScope;
use crate::service::access_control::consumers::{WIRED_CONSUMERS, configurable_registration};
use crate::{Error, Permission, PermissionSet, Result, RoleId};

impl PolicyFacts {
    /// 从完整事实生成可执行文件；复杂旧表达式不能被静默丢弃。
    /// # 参数
    /// policy_version 为同一快照的授权版本。
    /// # 返回
    /// 规范化文件及不能序列化为附加授权的系统规则说明。
    /// # 错误
    /// 角色失效、缺少动作资格、复杂旧范围或文件超限时拒绝。
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
            if user.role_ids.iter().any(|role| !self.roles.contains_key(role)) {
                return Err(Error::ValidationError("人员绑定含失效角色，不能生成完整导出".into()));
            }
            document.bindings.push(PolicyBinding {
                user_id: id.clone(),
                mode: PolicyMode::Replace,
                role_ids: user.role_ids.iter().map(RoleId::parse).collect::<std::result::Result<_, _>>()?,
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

    /// 显式保存所选人员当前有效动作的空配置，避免重导入保留后来新增的范围。
    fn export_default_scopes(&self, document: &mut PolicyDocument) -> Result<()> {
        for (user_id, user) in &self.users {
            let permissions = PermissionSet::new(
                user.role_ids.iter().filter_map(|id| self.roles.get(id)).flat_map(|r| r.permissions.clone()),
            );
            for (resource, actions, _) in WIRED_CONSUMERS {
                let mut missing = Vec::new();
                for action in *actions {
                    let configured = self
                        .scopes
                        .iter()
                        .any(|s| s.user_id == *user_id && s.resource == *resource && s.action == *action);
                    if !configured
                        && configurable_registration(resource, action).is_ok()
                        && permissions.covers_one(&Permission::parse(format!("{resource}:{action}"))?)
                    {
                        missing.push((*action).to_owned());
                    }
                }
                if !missing.is_empty() {
                    document.data_scopes.push(PolicyScope {
                        user_id: user_id.clone(),
                        resource: (*resource).into(),
                        actions: missing,
                        mode: PolicyMode::Replace,
                        replace_legacy: false,
                        grants: vec![],
                    });
                }
            }
        }
        Ok(())
    }

    /// 每个动作独立导出，保留追加项的并集和维度交集。
    fn export_scopes(&self, document: &mut PolicyDocument, notes: &mut Vec<String>) -> Result<()> {
        for scope in &self.scopes {
            if configurable_registration(&scope.resource, &scope.action).is_err() {
                notes.push(format!(
                    "已退役配置未参与导出：{}/{}/{}",
                    scope.user_id, scope.resource, scope.action
                ));
                continue;
            }
            self.ensure_scope_qualification(scope)?;
            let expression = &scope.expression;
            if !expression.additive || expression.history_read || expression.condition.is_some() {
                return Err(Error::ValidationError(format!(
                    "人员 {} 的 {}:{} 为旧表达式，不能无损导出为1.0配置",
                    scope.user_id, scope.resource, scope.action
                )));
            }
            document.data_scopes.push(PolicyScope {
                user_id: scope.user_id.clone(),
                resource: scope.resource.clone(),
                actions: vec![scope.action.clone()],
                mode: PolicyMode::Replace,
                replace_legacy: false,
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

    /// 保留已撤动作后的休眠配置，但不能将其伪装成可直接重导入的文件。
    fn ensure_scope_qualification(&self, scope: &PersonDataScope) -> Result<()> {
        let user = self.users.get(&scope.user_id).ok_or_else(|| Error::NotFound("范围人员不存在".into()))?;
        let required = Permission::parse(format!("{}:{}", scope.resource, scope.action))?;
        let permitted = user
            .role_ids
            .iter()
            .filter_map(|id| self.roles.get(id))
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
