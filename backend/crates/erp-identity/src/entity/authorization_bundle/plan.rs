//! 在冻结事实上计算权限上限、批次最终资格和逐项差异。
use std::collections::BTreeMap;

use application_core::CommandReceipt;
use serde::Serialize;
use serde_json::{Value, json};

use super::{PolicyDocument, PolicyMode, PolicyRole, PolicyScope};
use crate::dto::authorization_bundle::{PolicyChange, PolicyPreview};
use crate::entity::access_control::person_scope::{PersonDataScope, PersonScopeExpression};
use crate::entity::role::ROOT_ROLE_ID;
use crate::{Error, Permission, PermissionSet, Result, Role};

#[derive(Clone, Serialize)]
pub(crate) struct PolicyRoleState {
    pub role: Role,
    pub permissions: Vec<Permission>,
    pub affected_user_ids: Vec<String>,
}

#[derive(Clone, Serialize)]
pub(crate) struct PolicyUserState {
    pub version: u64,
    pub role_ids: Vec<String>,
}

#[derive(Default, Serialize)]
pub(crate) struct PolicyFacts {
    pub roles: BTreeMap<String, PolicyRoleState>,
    pub users: BTreeMap<String, PolicyUserState>,
    pub scopes: Vec<PersonDataScope>,
}

pub(crate) struct PlannedRole {
    pub declaration: PolicyRole,
    pub permissions: Vec<Permission>,
    pub existing: Option<Role>,
}

pub(crate) struct PlannedBinding {
    pub user_id: String,
    pub role_ids: Vec<String>,
}

pub(crate) struct PlannedScope {
    pub user_id: String,
    pub resource: String,
    pub action: String,
    pub expression: PersonScopeExpression,
    pub existing: Option<PersonDataScope>,
}

pub(crate) struct PolicyPlan {
    pub preview: PolicyPreview,
    pub roles: Vec<PlannedRole>,
    pub bindings: Vec<PlannedBinding>,
    pub scopes: Vec<PlannedScope>,
}

impl PolicyPlan {
    /// 从同一快照编译最终授权；操作人权限始终取批次开始前的事实。
    /// # 参数
    /// document 为规范化文件，facts 为目标事实，其余参数为冻结的操作人及版本。
    /// # 返回
    /// 可审核差异及服务端使用的写入计划。
    /// # 错误
    /// 越权、无效引用、范围不合规或摘要生成失败时拒绝。
    pub(crate) fn build(
        document: PolicyDocument,
        facts: &PolicyFacts,
        actor_id: &str,
        actor_permissions: &PermissionSet,
        policy_version: u64,
        authorization_version: &str,
    ) -> Result<Self> {
        let mut plan = Self {
            preview: PolicyPreview {
                document: document.clone(),
                policy_version,
                changes: vec![],
                policy_notes: policy_notes(&document)?,
                review_hash: CommandReceipt::from_payload(
                    "preview-", actor_id, "preview", "policy", "preview", &document,
                )?
                .fingerprint()
                .clone(),
            },
            roles: vec![],
            bindings: vec![],
            scopes: vec![],
        };
        for role in &document.roles {
            plan.role(role, facts, actor_permissions)?;
        }
        plan.bindings(&document, facts, actor_permissions)?;
        for scope in &document.data_scopes {
            plan.scope(scope, facts)?;
        }
        let payload = (
            actor_id,
            authorization_version,
            policy_version,
            &document,
            facts,
            &plan.preview.changes,
            &plan.preview.policy_notes,
        );
        plan.preview.review_hash = CommandReceipt::from_payload(
            "preview-",
            actor_id,
            "authorization_policy.preview",
            "authorization_policy",
            "preview",
            &payload,
        )?
        .fingerprint()
        .clone();
        Ok(plan)
    }

    /// 角色创建或更新必须同时满足现有及目标权限上限。
    fn role(&mut self, requested: &PolicyRole, facts: &PolicyFacts, actor: &PermissionSet) -> Result<()> {
        let current = facts.roles.get(requested.id.as_str());
        if let Some(current) = current {
            ensure_role(&current.role)?;
            ensure_subset(actor, &current.permissions)?;
        } else if !requested.id.as_str().starts_with("policy-") {
            return Err(Error::ValidationError("新角色ID必须以 policy- 开头".into()));
        }
        let before_permissions =
            merge(PolicyMode::Replace, &[], &current.map(|r| r.permissions.clone()).unwrap_or_default());
        let permissions = merge(requested.mode, &before_permissions, &requested.permissions);
        ensure_subset(actor, &permissions)?;
        let before = current
            .map(|r| json!({"name": r.role.name, "permissions": before_permissions}))
            .unwrap_or(Value::Null);
        let after = json!({"name": requested.name, "permissions": permissions});
        if before != after {
            self.preview.changes.push(PolicyChange {
                kind: "role".into(),
                target: requested.id.to_string(),
                before,
                after,
                affected_user_ids: current.map(|r| r.affected_user_ids.clone()).unwrap_or_default(),
            });
            self.roles.push(PlannedRole {
                declaration: requested.clone(),
                permissions,
                existing: current.map(|r| r.role.clone()),
            });
        }
        Ok(())
    }

    /// 绑定按最终角色事实计算，不能通过先收窄高权限目标绕过管理上限。
    fn bindings(
        &mut self,
        document: &PolicyDocument,
        facts: &PolicyFacts,
        actor: &PermissionSet,
    ) -> Result<()> {
        for binding in &document.bindings {
            let user =
                facts.users.get(&binding.user_id).ok_or_else(|| Error::NotFound("人员不存在".into()))?;
            for id in &user.role_ids {
                let role =
                    facts.roles.get(id).ok_or_else(|| Error::ValidationError("人员绑定角色缺失".into()))?;
                ensure_role(&role.role)?;
                ensure_subset(actor, &role.permissions)?;
            }
            let requested = binding.role_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
            let role_ids = merge(binding.mode, &user.role_ids, &requested);
            for id in &role_ids {
                ensure_subset(actor, &self.role_permissions(id, facts)?)?;
            }
            if role_ids != user.role_ids {
                self.preview.changes.push(PolicyChange {
                    kind: "binding".into(),
                    target: binding.user_id.clone(),
                    before: json!(user.role_ids),
                    after: json!(role_ids),
                    affected_user_ids: vec![binding.user_id.clone()],
                });
                self.bindings.push(PlannedBinding { user_id: binding.user_id.clone(), role_ids });
            }
        }
        Ok(())
    }

    /// 按本批角色及绑定的最终状态验证每个动作，再替换选定范围。
    fn scope(&mut self, scope: &PolicyScope, facts: &PolicyFacts) -> Result<()> {
        let user = facts.users.get(&scope.user_id).ok_or_else(|| Error::NotFound("人员不存在".into()))?;
        let role_ids = self
            .bindings
            .iter()
            .find(|b| b.user_id == scope.user_id)
            .map(|b| &b.role_ids)
            .unwrap_or(&user.role_ids);
        let mut permissions = vec![];
        for id in role_ids {
            permissions.extend(self.role_permissions(id, facts)?);
        }
        let permissions = PermissionSet::new(permissions);
        let request = scope.request(self.preview.policy_version)?;
        let existing =
            facts.scopes.iter().filter(|s| s.user_id == scope.user_id).cloned().collect::<Vec<_>>();
        request.ensure_legacy_conversion(&existing)?;
        for action in &scope.actions {
            let required = Permission::parse(format!("{}:{action}", scope.resource))?;
            if !permissions.covers_one(&required) {
                return Err(Error::ValidationError(format!(
                    "人员 {} 在批次完成后缺少 {required}",
                    scope.user_id
                )));
            }
            let current = existing.iter().find(|s| s.resource == scope.resource && s.action == *action);
            let expression = scope.canonical_expression(action, &request.expression(action))?;
            let previous = current.map(|s| scope.canonical_expression(action, &s.expression)).transpose()?;
            if previous.as_ref() == Some(&expression)
                || (current.is_none() && expression.alternatives.is_empty())
            {
                continue;
            }
            self.preview.changes.push(PolicyChange {
                kind: "data_scope".into(),
                target: format!("{}/{}/{action}", scope.user_id, scope.resource),
                before: current.map(|s| json!(s.expression)).unwrap_or(Value::Null),
                after: json!(expression),
                affected_user_ids: vec![scope.user_id.clone()],
            });
            self.scopes.push(PlannedScope {
                user_id: scope.user_id.clone(),
                resource: scope.resource.clone(),
                action: action.clone(),
                expression,
                existing: current.cloned(),
            });
        }
        Ok(())
    }

    /// 取得最终权限；所有引用均须有当前角色或同批新建角色。
    fn role_permissions(&self, id: &str, facts: &PolicyFacts) -> Result<Vec<Permission>> {
        if let Some(role) = self.roles.iter().find(|r| r.declaration.id.as_str() == id) {
            return Ok(role.permissions.clone());
        }
        let role = facts.roles.get(id).ok_or_else(|| Error::ValidationError(format!("角色不存在：{id}")))?;
        ensure_role(&role.role)?;
        Ok(role.permissions.clone())
    }
}

/// 将隐含基础范围与岗位目录初始化明确列入审核结果。
fn policy_notes(document: &PolicyDocument) -> Result<Vec<String>> {
    let mut notes = vec![
        "文件未列对象保持原配置；replace 仅替换条目明确指定的集合。".into(),
        "角色绑定沿用销售和采购岗位目录资格的首次初始化；解绑不自动终止查询资格。".into(),
    ];
    for scope in &document.data_scopes {
        let request = scope.request(0)?;
        let baseline = if PersonScopeExpression::default_self(&request.resource) {
            "本人负责的数据与追加范围求并，清空追加范围仍保留本人基础"
        } else {
            "无本人基础，清空追加范围后该动作无独立数据范围"
        };
        notes.push(format!("{}/{}：{baseline}", scope.user_id, scope.resource));
    }
    Ok(notes)
}

/// 合并模式只增补，替换模式采用完整显式集合。
fn merge<T: Clone + Ord>(mode: PolicyMode, current: &[T], requested: &[T]) -> Vec<T> {
    let mut values = if mode == PolicyMode::Merge { current.to_vec() } else { vec![] };
    values.extend_from_slice(requested);
    values.sort();
    values.dedup();
    values
}

/// 文件入口不得恢复或认领系统及失效角色。
/// # 参数
/// role 为持久化角色事实。
/// # 返回
/// 角色为有效普通角色时成功。
/// # 错误
/// 系统、停用或删除角色返回禁止访问。
pub(crate) fn ensure_role(role: &Role) -> Result<()> {
    if role.base.id == ROOT_ROLE_ID || role.system || role.disabled || role.base.is_deleted() {
        return Err(Error::Forbidden("策略文件不能管理系统、停用或已删除角色".into()));
    }
    Ok(())
}

/// 复用 PermissionSet 的真实覆盖规则。
fn ensure_subset(actor: &PermissionSet, required: &[Permission]) -> Result<()> {
    if !actor.covers(&PermissionSet::new(required.to_vec())) {
        return Err(Error::Forbidden("不能管理或授予超出自身权限的角色".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use serde_json::json;

    use super::*;
    use crate::RoleData;
    use crate::entity::authorization_bundle::PolicyVersion;

    /// 测试使用固定元数据，避免时间影响审核摘要。
    fn role(id: &str, permissions: &[&str]) -> PolicyRoleState {
        let mut role = Role::new(id.into(), RoleData::new("只读岗位")).unwrap();
        role.base = BaseModel::fake();
        role.base.id = id.into();
        PolicyRoleState {
            role,
            permissions: permissions.iter().map(|p| Permission::parse(p).unwrap()).collect(),
            affected_user_ids: vec!["alice".into()],
        }
    }

    fn facts() -> PolicyFacts {
        let mut facts = PolicyFacts::default();
        facts.roles.insert("reader".into(), role("reader", &["sales_order:list", "sales_order:detail"]));
        facts.users.insert("alice".into(), PolicyUserState { version: 1, role_ids: vec!["reader".into()] });
        facts
    }

    fn document() -> PolicyDocument {
        PolicyDocument { version: PolicyVersion::V1, roles: vec![], bindings: vec![], data_scopes: vec![] }
    }

    fn scope(actions: &[&str]) -> PolicyScope {
        serde_json::from_value(json!({
            "user_id":"alice", "resource":"sales_order", "actions":actions, "mode":"replace",
            "grants":[{"actions":actions,"terms":[{
                "scope_type":"organization","target_dimension":"internal_org","target_mode":"explicit",
                "include_descendants":false,"scope_targets":["east"]
            }]}]
        }))
        .unwrap()
    }

    fn plan(document: PolicyDocument, facts: &PolicyFacts) -> Result<PolicyPlan> {
        PolicyPlan::build(
            document.normalized()?,
            facts,
            "operator",
            &PermissionSet::new(vec![Permission::parse("*:*").unwrap()]),
            4,
            "organization-v1",
        )
    }

    #[test]
    fn new_role_binding_and_scope_use_final_batch_qualification() {
        let mut facts = facts();
        facts.users.get_mut("alice").unwrap().role_ids.clear();
        let value = serde_json::from_value(json!({
            "version":"1.0",
            "roles":[{"id":"policy-new","name":"新销售","mode":"replace","permissions":["sales_order:list"]}],
            "bindings":[{"user_id":"alice","mode":"merge","role_ids":["policy-new"]}],
            "data_scopes":[scope(&["list"])]
        }))
        .unwrap();
        let plan = plan(value, &facts).unwrap();
        assert_eq!(
            plan.preview.changes.iter().map(|c| c.kind.as_str()).collect::<Vec<_>>(),
            ["role", "binding", "data_scope"]
        );
        assert_eq!(plan.bindings[0].role_ids, ["policy-new"]);
        assert_eq!(plan.scopes[0].action, "list");
        assert_eq!(plan.scopes[0].expression.alternatives[0][0].scope_targets, ["east"]);
    }

    #[test]
    fn replacing_permissions_revokes_only_selected_role_and_exposes_affected_users() {
        let mut value = document();
        value.roles.push(
            serde_json::from_value(json!({
                "id":"reader","name":"只读岗位","mode":"replace","permissions":["sales_order:list"]
            }))
            .unwrap(),
        );
        let plan = plan(value.clone(), &facts()).unwrap();
        assert_eq!(plan.roles[0].permissions, vec![Permission::parse("sales_order:list").unwrap()]);
        assert_eq!(plan.preview.changes[0].affected_user_ids, ["alice"]);
        assert!(plan.bindings.is_empty());
        value.roles[0].mode = PolicyMode::Merge;
        assert!(super::tests::plan(value, &facts()).unwrap().preview.changes.is_empty());
    }

    #[test]
    fn adding_scope_cannot_grant_a_missing_action() {
        let mut value = document();
        value.data_scopes.push(scope(&["update"]));
        assert!(matches!(plan(value, &facts()), Err(Error::ValidationError(_))));
    }

    #[test]
    fn same_batch_action_revocation_cannot_be_masked_by_old_permissions() {
        let mut value = document();
        value.roles.push(
            serde_json::from_value(json!({
                "id":"reader","name":"只读岗位","mode":"replace","permissions":[]
            }))
            .unwrap(),
        );
        value.data_scopes.push(scope(&["list"]));
        assert!(matches!(plan(value, &facts()), Err(Error::ValidationError(_))));
    }

    #[test]
    fn narrow_operator_cannot_reduce_then_manage_a_stronger_role() {
        let mut value = document();
        value.roles.push(
            serde_json::from_value(json!({
                "id":"reader","name":"只读岗位","mode":"replace","permissions":[]
            }))
            .unwrap(),
        );
        let result = PolicyPlan::build(
            value,
            &facts(),
            "operator",
            &PermissionSet::new(vec![Permission::parse("sales_order:list").unwrap()]),
            4,
            "v",
        );
        assert!(matches!(result, Err(Error::Forbidden(_))));
    }

    #[test]
    fn system_disabled_deleted_and_missing_roles_are_not_restored_or_bound() {
        for state in ["system", "disabled", "deleted", "missing"] {
            let mut facts = facts();
            if state == "missing" {
                facts.roles.clear();
            } else {
                let role = &mut facts.roles.get_mut("reader").unwrap().role;
                match state {
                    "system" => role.system = true,
                    "disabled" => role.disabled = true,
                    _ => role.base.deleted_at = 1,
                }
            }
            let mut value = document();
            value.bindings.push(
                serde_json::from_value(json!({
                    "user_id":"alice","mode":"replace","role_ids":[]
                }))
                .unwrap(),
            );
            assert!(plan(value, &facts).is_err(), "{state}");
        }
    }

    #[test]
    fn repeated_desired_scope_is_noop_and_clearing_keeps_additive_base() {
        let mut value = document();
        value.data_scopes.push(scope(&["list"]));
        let mut facts = facts();
        let first = plan(value.clone(), &facts).unwrap();
        let mut row = PersonDataScope::default_for("alice", "sales_order", "list");
        row.expression = first.scopes[0].expression.clone();
        facts.scopes.push(row);
        assert!(plan(value.clone(), &facts).unwrap().preview.changes.is_empty());
        value.data_scopes[0].grants.clear();
        let cleared = plan(value, &facts).unwrap();
        assert!(cleared.scopes[0].expression.additive);
        assert!(cleared.scopes[0].expression.alternatives.is_empty());
        assert!(cleared.bindings.is_empty());
    }

    #[test]
    fn legacy_conversion_requires_explicit_opt_in_and_preserves_unselected_actions() {
        let mut facts = facts();
        let mut legacy = PersonDataScope::default_for("alice", "sales_order", "list");
        legacy.expression.additive = false;
        facts.scopes.push(legacy);
        let mut value = document();
        value.data_scopes.push(scope(&["list"]));
        assert!(matches!(plan(value.clone(), &facts), Err(Error::ValidationError(_))));
        value.data_scopes[0].replace_legacy = true;
        let result = plan(value, &facts).unwrap();
        assert_eq!(result.scopes.len(), 1);
        assert_eq!(result.scopes[0].action, "list");
    }

    #[test]
    fn review_hash_changes_with_actor_version_payload_and_target_facts() {
        let mut value = document();
        value.data_scopes.push(scope(&["list"]));
        let facts = facts();
        let first = plan(value.clone(), &facts).unwrap().preview.review_hash;
        assert_eq!(first, plan(value.clone(), &facts).unwrap().preview.review_hash);
        let actor = PermissionSet::new(vec![Permission::parse("*:*").unwrap()]);
        for (who, version, organization) in
            [("other", 4, "organization-v1"), ("operator", 5, "organization-v1"), ("operator", 4, "changed")]
        {
            let result =
                PolicyPlan::build(value.clone(), &facts, who, &actor, version, organization).unwrap();
            assert_ne!(first, result.preview.review_hash);
        }
        let mut changed = facts;
        changed.users.get_mut("alice").unwrap().version += 1;
        assert_ne!(first, plan(value, &changed).unwrap().preview.review_hash);
    }

    #[test]
    fn export_and_reimport_keep_current_explicit_authorization_unchanged() {
        let mut facts = facts();
        let mut row = PersonDataScope::default_for("alice", "sales_order", "list");
        row.expression = scope(&["list"]).request(4).unwrap().expression("list");
        facts.scopes.push(row);
        let exported = facts.export(4).unwrap();
        assert_eq!(exported.document.bindings[0].role_ids[0].as_str(), "reader");
        assert!(plan(exported.document, &facts).unwrap().preview.changes.is_empty());
        facts.scopes[0].expression.additive = false;
        assert!(matches!(facts.export(4), Err(Error::ValidationError(_))));
    }

    #[test]
    fn export_refuses_dormant_scopes_instead_of_implicitly_restoring_action_permission() {
        let mut facts = facts();
        facts.scopes.push(PersonDataScope::default_for("alice", "sales_order", "update"));
        assert!(matches!(facts.export(4), Err(Error::ValidationError(_))));
        assert!(
            !facts.roles["reader"].permissions.contains(&Permission::parse("sales_order:update").unwrap())
        );
    }

    #[test]
    fn empty_replacements_revoke_bindings_and_permissions_without_touching_omitted_scopes() {
        let value = serde_json::from_value(json!({
            "version":"1.0",
            "roles":[{"id":"reader","name":"只读岗位","mode":"replace","permissions":[]}],
            "bindings":[{"user_id":"alice","mode":"replace","role_ids":[]}],
            "data_scopes":[]
        }))
        .unwrap();
        let mut facts = facts();
        facts.scopes.push(PersonDataScope::default_for("alice", "sales_order", "list"));
        let result = plan(value, &facts).unwrap();
        assert!(result.roles[0].permissions.is_empty());
        assert!(result.bindings[0].role_ids.is_empty());
        assert!(result.scopes.is_empty());
        assert_eq!(result.preview.changes.len(), 2);
    }

    #[test]
    fn exported_default_scopes_revoke_grants_added_after_export() {
        let mut facts = facts();
        let exported = facts.export(4).unwrap();
        let mut row = PersonDataScope::default_for("alice", "sales_order", "list");
        row.expression = scope(&["list"]).request(4).unwrap().expression("list");
        facts.scopes.push(row);
        let restored = plan(exported.document, &facts).unwrap();
        assert_eq!(restored.scopes.len(), 1);
        assert_eq!(restored.scopes[0].action, "list");
        assert!(restored.scopes[0].expression.additive);
        assert!(restored.scopes[0].expression.alternatives.is_empty());
    }

    #[test]
    fn export_reimport_ignores_grant_and_target_order_and_duplicates() {
        let mut facts = facts();
        let mut row = PersonDataScope::default_for("alice", "sales_order", "list");
        row.expression = scope(&["list"]).request(4).unwrap().expression("list");
        row.expression.alternatives[0][0].scope_targets = vec!["west".into(), "east".into(), "east".into()];
        let mut other = row.expression.alternatives[0].clone();
        other[0].scope_targets = vec!["north".into()];
        row.expression.alternatives.insert(0, other.clone());
        row.expression.alternatives.push(other);
        facts.scopes.push(row);
        let exported = facts.export(4).unwrap();
        assert!(plan(exported.document, &facts).unwrap().preview.changes.is_empty());
    }

    #[test]
    fn export_keeps_empty_warehouse_scope_and_omits_task_and_source_inherited_scopes() {
        let mut facts = facts();
        facts.roles.insert(
            "reader".into(),
            role("reader", &["stock_balance:list", "approval_instance:read", "contract:list"]),
        );
        let exported = facts.export(4).unwrap();
        assert_eq!(exported.document.data_scopes.len(), 1);
        let scope = &exported.document.data_scopes[0];
        assert_eq!(scope.resource, "stock_balance");
        assert!(scope.grants.is_empty());
        assert!(!PersonScopeExpression::default_self(&scope.resource));
        assert!(plan(exported.document, &facts).unwrap().preview.changes.is_empty());
    }
}
