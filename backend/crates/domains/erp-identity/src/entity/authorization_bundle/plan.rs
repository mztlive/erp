//! 在冻结事实上计算权限上限、批次最终资格和逐项差异。
use std::collections::BTreeMap;

use application_core::CommandReceipt;
use serde::Serialize;
use serde_json::{Value, json};

use super::{PolicyDocument, PolicyMode, PolicyRole, PolicyScope};
use crate::dto::authorization_bundle::{PolicyChange, PolicyPreview};
use crate::dto::person_scope::ensure_legacy_conversion;
use crate::entity::access_control::person_scope::{PersonDataScope, PersonScopeExpression};
use crate::entity::role::ROOT_ROLE_ID;
use crate::{Error, Permission, PermissionSet, Result, Role, RoleId};

/// 同一快照中的角色实体、直接权限和直接绑定影响面，供规划及导出使用。
#[derive(Clone, Serialize)]
pub(crate) struct PolicyRoleState {
    /// 持久化角色实体，提供名称、状态和更新所需版本。
    pub role: Role,
    /// 角色直接授予的功能权限，保留精确权限和通配模式。
    pub permissions: Vec<Permission>,
    /// 直接绑定此角色的后台账号 ID，供预览显示权限变更的影响面。
    pub affected_user_ids: Vec<String>,
}

/// 计划涉及的后台账号版本及直接角色分组快照。
#[derive(Clone, Serialize)]
pub(crate) struct PolicyUserState {
    /// 账号版本，参与审核摘要以识别预览后的账号变更。
    pub version: u64,
    /// 当前直接绑定的角色 ID，排序去重后用于差异比较。
    pub role_ids: Vec<RoleId>,
}

/// 服务端读出的目标授权快照，用于计算差异并约束预览可应用的事实。
#[derive(Default, Serialize)]
pub(crate) struct PolicyFacts {
    /// 按角色 ID 索引的现存角色及直接授权事实。
    pub roles: BTreeMap<String, PolicyRoleState>,
    /// 按后台账号 ID 索引的目标人员事实。
    pub users: BTreeMap<String, PolicyUserState>,
    /// 从快照读取的逐人员、资源和动作范围记录，包含旧表达式。
    pub scopes: Vec<PersonDataScope>,
}

/// 有实际差异的角色创建或更新，供应用事务写入。
pub(crate) struct PlannedRole {
    pub id: RoleId,
    pub name: String,
    /// 按声明模式计算并校验后的完整直接权限集合。
    pub permissions: Vec<Permission>,
    /// 当前角色实体；为空表示新建。
    pub existing: Option<Role>,
}

/// 有实际差异的单个后台人员直接角色绑定更新。
pub(crate) struct PlannedBinding {
    /// 需要写入角色分组的后台账号 ID。
    pub user_id: String,
    /// 该账号在批次完成后应持有的完整直接角色集合。
    pub role_ids: Vec<RoleId>,
}

/// 有实际差异的单个人员、资源和动作范围写入。
pub(crate) struct PlannedScope {
    /// 配置归属的后台账号 ID。
    pub user_id: String,
    /// 范围适用的业务资源代码。
    pub resource: String,
    /// 本次替换的可配置动作。
    pub action: String,
    /// 本次持久化的规范化追加范围表达式，允许空追加项。
    pub expression: PersonScopeExpression,
    /// 当前范围记录及其版本；为空表示需要新建。
    pub existing: Option<PersonDataScope>,
}

/// 有差异的写入步骤，按角色、绑定、范围的依赖顺序执行。
#[derive(Default)]
pub(crate) struct PolicyPlan {
    /// 需创建或更新的角色及其最终直接权限。
    pub roles: Vec<PlannedRole>,
    /// 需更新的人员直接角色绑定。
    pub bindings: Vec<PlannedBinding>,
    /// 需创建或替换的逐动作人员范围。
    pub scopes: Vec<PlannedScope>,
}

impl PolicyPlan {
    /// 从已规范化的文件和同一快照的目标事实生成预览及写入计划，操作人的管理上限固定为批次前权限。
    /// 审核摘要绑定操作人、策略及授权版本、文件、目标事实和预览内容；越权、无效引用、范围不合规或摘要生成失败时拒绝。
    pub(crate) fn build(
        document: PolicyDocument,
        facts: &PolicyFacts,
        actor_id: &str,
        actor_permissions: &PermissionSet,
        policy_version: u64,
        authorization_version: &str,
    ) -> Result<(Self, PolicyPreview)> {
        let mut plan = Self::default();
        for role in &document.roles {
            plan.role(role, facts, actor_permissions)?;
        }
        plan.bindings(&document, facts, actor_permissions)?;
        for scope in &document.data_scopes {
            plan.scope(scope, facts)?;
        }

        let changes = plan.changes(facts);
        let policy_notes = policy_notes(&document);
        let payload =
            (actor_id, authorization_version, policy_version, &document, facts, &changes, &policy_notes);
        let review_hash = CommandReceipt::from_payload(
            "preview-",
            actor_id,
            "authorization_policy.preview",
            "authorization_policy",
            "preview",
            &payload,
        )?
        .fingerprint()
        .clone();
        Ok((plan, PolicyPreview { document, policy_version, review_hash, changes, policy_notes }))
    }

    /// 返回实际写入项数，供应用判断是否推进策略版本。
    pub(crate) fn change_count(&self) -> usize {
        self.roles.len() + self.bindings.len() + self.scopes.len()
    }

    // 审核差异按写入顺序从实际变更及原快照生成。
    fn changes(&self, facts: &PolicyFacts) -> Vec<PolicyChange> {
        let mut changes = Vec::with_capacity(self.change_count());
        for role in &self.roles {
            let current = facts.roles.get(role.id.as_str());
            changes.push(PolicyChange {
                kind: "role".into(),
                target: role.id.to_string(),
                before: current.map_or(Value::Null, |r| {
                    let permissions = merge(PolicyMode::Replace, &[], &r.permissions);
                    json!({"name": r.role.name, "permissions": permissions})
                }),
                after: json!({"name": role.name, "permissions": role.permissions}),
                affected_user_ids: current.map_or_else(Vec::new, |r| r.affected_user_ids.clone()),
            });
        }
        for binding in &self.bindings {
            changes.push(PolicyChange {
                kind: "binding".into(),
                target: binding.user_id.clone(),
                before: json!(facts.users[&binding.user_id].role_ids),
                after: json!(binding.role_ids),
                affected_user_ids: vec![binding.user_id.clone()],
            });
        }
        for scope in &self.scopes {
            changes.push(PolicyChange {
                kind: "data_scope".into(),
                target: format!("{}/{}/{}", scope.user_id, scope.resource, scope.action),
                before: scope.existing.as_ref().map_or(Value::Null, |s| json!(s.expression)),
                after: json!(scope.expression),
                affected_user_ids: vec![scope.user_id.clone()],
            });
        }
        changes
    }

    // 规划角色名称和直接权限的变更；现存角色同时检查原权限和结果权限，新角色要求 `policy-` 前缀。
    fn role(&mut self, requested: &PolicyRole, facts: &PolicyFacts, actor: &PermissionSet) -> Result<()> {
        let current = facts.roles.get(requested.id.as_str());
        if let Some(current) = current {
            ensure_role(&current.role)?;
            ensure_subset(actor, &current.permissions)?;
        } else if !requested.id.as_str().starts_with("policy-") {
            return Err(Error::ValidationError("新角色ID必须以 policy- 开头".into()));
        }

        let before_permissions =
            merge(PolicyMode::Replace, &[], current.map_or(&[], |role| role.permissions.as_slice()));
        let permissions = merge(requested.mode, &before_permissions, &requested.permissions);
        ensure_subset(actor, &permissions)?;

        if current.is_none_or(|state| state.role.name != requested.name || before_permissions != permissions)
        {
            self.roles.push(PlannedRole {
                id: requested.id.clone(),
                name: requested.name.clone(),
                permissions,
                existing: current.map(|r| r.role.clone()),
            });
        }
        Ok(())
    }

    // 规划绑定时同时检查原角色权限和本批最终角色权限，防止通过同批缩权绕过操作人的管理上限。
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
                let role = facts
                    .roles
                    .get(id.as_str())
                    .ok_or_else(|| Error::ValidationError("人员绑定角色缺失".into()))?;
                ensure_role(&role.role)?;
                ensure_subset(actor, &role.permissions)?;
            }

            let role_ids = merge(binding.mode, &user.role_ids, &binding.role_ids);
            for id in &role_ids {
                ensure_subset(actor, self.role_permissions(id.as_str(), facts)?)?;
            }

            if role_ids != user.role_ids {
                self.bindings.push(PlannedBinding { user_id: binding.user_id.clone(), role_ids });
            }
        }
        Ok(())
    }

    // 按本批最终绑定及权限检查动作资格，核对旧范围转换许可，仅为有差异的动作安排写入。
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
            permissions.extend_from_slice(self.role_permissions(id.as_str(), facts)?);
        }
        let permissions = PermissionSet::new(permissions);

        let existing = || facts.scopes.iter().filter(|s| s.user_id == scope.user_id);
        ensure_legacy_conversion(&scope.resource, &scope.actions, scope.replace_legacy, existing())?;

        for action in &scope.actions {
            let required = Permission::parse(format!("{}:{action}", scope.resource))?;
            if !permissions.covers_one(&required) {
                return Err(Error::ValidationError(format!(
                    "人员 {} 在批次完成后缺少 {required}",
                    scope.user_id
                )));
            }

            let current = existing().find(|s| s.resource == scope.resource && s.action == *action);
            let expression = scope.expression(action)?;
            let previous =
                current.map(|s| scope.canonical_existing_expression(action, &s.expression)).transpose()?;
            if previous.as_ref() == Some(&expression)
                || (current.is_none() && expression.alternatives.is_empty())
            {
                continue;
            }

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

    // 优先取批次规划后的直接权限，否则读取现存有效普通角色；无法解析的引用报错。
    fn role_permissions<'a>(&'a self, id: &str, facts: &'a PolicyFacts) -> Result<&'a [Permission]> {
        if let Some(role) = self.roles.iter().find(|r| r.id.as_str() == id) {
            return Ok(&role.permissions);
        }
        let role = facts.roles.get(id).ok_or_else(|| Error::ValidationError(format!("角色不存在：{id}")))?;
        ensure_role(&role.role)?;
        Ok(&role.permissions)
    }
}

// 为预览说明未列条目的保留规则、岗位目录的首次初始化以及清空各动作追加范围后的本人基础。
fn policy_notes(document: &PolicyDocument) -> Vec<String> {
    let mut notes = vec![
        "文件未列对象保持原配置；replace 仅替换条目明确指定的集合。".into(),
        "角色绑定沿用销售和采购岗位目录资格的首次初始化；解绑不自动终止查询资格。".into(),
    ];
    for scope in &document.data_scopes {
        for action in &scope.actions {
            let baseline = if PersonScopeExpression::default_self(&scope.resource, action) {
                "本人负责的数据与追加范围求并，清空追加范围仍保留本人基础"
            } else {
                "无本人基础，清空追加范围后该动作无独立数据范围"
            };
            notes.push(format!("{}/{}:{action}：{baseline}", scope.user_id, scope.resource));
        }
    }
    notes
}

// 合并时取现有值和文件值的并集，替换时仅取文件值；结果统一排序去重。
fn merge<T: Clone + Ord>(mode: PolicyMode, current: &[T], requested: &[T]) -> Vec<T> {
    let mut values = if mode == PolicyMode::Merge { current.to_vec() } else { vec![] };
    values.extend_from_slice(requested);
    values.sort();
    values.dedup();
    values
}

/// 文件规划和导出只接受有效普通角色；持久化角色为系统、停用或已删除状态时返回禁止访问。
pub(crate) fn ensure_role(role: &Role) -> Result<()> {
    if role.base.id == ROOT_ROLE_ID || role.system || role.disabled || role.base.is_deleted() {
        return Err(Error::Forbidden("策略文件不能管理系统、停用或已删除角色".into()));
    }
    Ok(())
}

// 逐项用操作人的权限集合校验管理上限，沿用 `PermissionSet` 的精确及通配覆盖规则。
fn ensure_subset(actor: &PermissionSet, required: &[Permission]) -> Result<()> {
    if !required.iter().all(|permission| actor.covers_one(permission)) {
        return Err(Error::Forbidden("不能管理或授予超出自身权限的角色".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use serde_json::json;

    use super::*;
    use crate::entity::authorization_bundle::PolicyVersion;
    use crate::{RoleData, RoleId};

    // 用固定元数据和指定权限构造测试角色，确保审核摘要不受时钟影响。
    fn role(id: &str, permissions: &[&str]) -> PolicyRoleState {
        let mut role = Role::new(RoleId::parse(id).unwrap(), RoleData::new("只读岗位")).unwrap();
        role.base = BaseModel::fake();
        role.base.id = id.into();
        PolicyRoleState {
            role,
            permissions: permissions.iter().map(|p| Permission::parse(p).unwrap()).collect(),
            affected_user_ids: vec!["alice".into()],
        }
    }

    // 提供绑定只读角色的后台人员快照，初始无显式人员范围。
    fn facts() -> PolicyFacts {
        let mut facts = PolicyFacts::default();
        facts.roles.insert("reader".into(), role("reader", &["sales_order:list", "sales_order:detail"]));
        facts.users.insert(
            "alice".into(),
            PolicyUserState { version: 1, role_ids: vec![RoleId::parse("reader").unwrap()] },
        );
        facts
    }

    // 构造无管理条目的 1.0 文件，供测试逐项加入变更。
    fn document() -> PolicyDocument {
        PolicyDocument { version: PolicyVersion::V1, roles: vec![], bindings: vec![], data_scopes: vec![] }
    }

    // 为指定销售单动作构造替换条目，各动作共享东区组织追加范围。
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

    // 以固定操作人、策略版本及授权版本规划规范化文件，默认给予操作人全量权限。
    fn plan(document: PolicyDocument, facts: &PolicyFacts) -> Result<(PolicyPlan, PolicyPreview)> {
        PolicyPlan::build(
            document.normalized()?,
            facts,
            "operator",
            &PermissionSet::new(vec![Permission::parse("*:*").unwrap()]),
            4,
            "organization-v1",
        )
    }

    // 验证空集合、重复权限和同级通配可通过上限校验，跨资源或更宽通配被拒绝。
    #[test]
    fn permission_ceiling_keeps_empty_duplicate_and_wildcard_semantics() {
        let actor = PermissionSet::new([Permission::parse("sales_order:*").unwrap()]);
        ensure_subset(&actor, &[]).unwrap();
        let read = Permission::parse("sales_order:list").unwrap();
        ensure_subset(&actor, &[read.clone(), read]).unwrap();
        ensure_subset(&actor, &[Permission::parse("sales_order:*").unwrap()]).unwrap();
        for required in ["*:list", "customer:list"] {
            assert!(matches!(
                ensure_subset(&actor, &[Permission::parse(required).unwrap()]),
                Err(Error::Forbidden(message)) if message == "不能管理或授予超出自身权限的角色"
            ));
        }
    }

    // 验证同批新建角色、绑定账号后，范围规划能使用新增角色授予的动作资格。
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

        let (plan, preview) = plan(value, &facts).unwrap();
        assert_eq!(
            preview.changes.iter().map(|c| c.kind.as_str()).collect::<Vec<_>>(),
            ["role", "binding", "data_scope"]
        );
        assert_eq!(plan.bindings[0].role_ids, [RoleId::parse("policy-new").unwrap()]);
        assert_eq!(plan.scopes[0].action, "list");
        assert_eq!(plan.scopes[0].expression.alternatives[0][0].scope_targets, ["east"]);
    }

    // 验证替换仅收回指定角色的权限并展示受影响账号，合并同一子集没有差异。
    #[test]
    fn replacing_permissions_revokes_only_selected_role_and_exposes_affected_users() {
        let mut value = document();
        value.roles.push(
            serde_json::from_value(json!({
                "id":"reader","name":"只读岗位","mode":"replace","permissions":["sales_order:list"]
            }))
            .unwrap(),
        );
        let (plan, preview) = plan(value.clone(), &facts()).unwrap();
        assert_eq!(plan.roles[0].permissions, vec![Permission::parse("sales_order:list").unwrap()]);
        assert_eq!(preview.changes[0].affected_user_ids, ["alice"]);
        assert!(plan.bindings.is_empty());

        value.roles[0].mode = PolicyMode::Merge;
        assert!(super::tests::plan(value, &facts()).unwrap().1.changes.is_empty());
    }

    // 验证追加人员范围无法补足角色缺失的更新动作资格。
    #[test]
    fn adding_scope_cannot_grant_a_missing_action() {
        let mut value = document();
        value.data_scopes.push(scope(&["update"]));
        assert!(matches!(plan(value, &facts()), Err(Error::ValidationError(_))));
    }

    // 验证同批已收回的动作资格无法继续用于范围替换。
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

    // 验证操作人即使要清空角色权限，仍须覆盖角色变更前的全部权限。
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
            value.normalized().unwrap(),
            &facts(),
            "operator",
            &PermissionSet::new(vec![Permission::parse("sales_order:list").unwrap()]),
            4,
            "v",
        );
        assert!(matches!(result, Err(Error::Forbidden(_))));
    }

    // 验证待替换的旧绑定引用系统、停用、删除或缺失角色时，计划拒绝生成。
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

    // 验证重设相同范围无差异，清空追加项仍保留追加模型及其本人基础规则。
    #[test]
    fn repeated_desired_scope_is_noop_and_clearing_keeps_additive_base() {
        let mut value = document();
        value.data_scopes.push(scope(&["list"]));
        let mut facts = facts();
        let (first, _) = plan(value.clone(), &facts).unwrap();
        let mut row = PersonDataScope::default_for("alice", "sales_order", "list");
        row.expression = first.scopes[0].expression.clone();
        facts.scopes.push(row);
        assert!(plan(value.clone(), &facts).unwrap().1.changes.is_empty());

        value.data_scopes[0].grants.clear();
        let (cleared, _) = plan(value, &facts).unwrap();
        assert!(cleared.scopes[0].expression.additive);
        assert!(cleared.scopes[0].expression.alternatives.is_empty());
        assert!(cleared.bindings.is_empty());
    }

    // 验证旧完整表达式须显式确认转换，且计划仅写入选定的列表动作。
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
        let (result, preview) = plan(value, &facts).unwrap();
        assert_eq!(result.scopes.len(), 1);
        assert_eq!(result.scopes[0].action, "list");
        assert_eq!(preview.changes.len(), result.change_count());
    }

    // 验证相同输入的审核摘要稳定，操作人、策略版本、授权版本和目标账号版本变化会改变摘要。
    #[test]
    fn review_hash_changes_with_actor_version_payload_and_target_facts() {
        let mut value = document();
        value.data_scopes.push(scope(&["list"]));
        let value = value.normalized().unwrap();
        let facts = facts();
        let first = plan(value.clone(), &facts).unwrap().1.review_hash;
        assert_eq!(first, plan(value.clone(), &facts).unwrap().1.review_hash);

        let actor = PermissionSet::new(vec![Permission::parse("*:*").unwrap()]);
        for (who, version, organization) in
            [("other", 4, "organization-v1"), ("operator", 5, "organization-v1"), ("operator", 4, "changed")]
        {
            let result =
                PolicyPlan::build(value.clone(), &facts, who, &actor, version, organization).unwrap();
            assert_ne!(first, result.1.review_hash);
        }

        let mut changed = facts;
        changed.users.get_mut("alice").unwrap().version += 1;
        assert_ne!(first, plan(value, &changed).unwrap().1.review_hash);
    }

    #[test]
    fn multi_action_scope_keeps_the_review_hash_and_branch_order() {
        let mut value = document();
        let mut scope = scope(&["list", "detail"]);
        let mut detail_only = scope.grants[0].clone();
        detail_only.actions = vec!["detail".into()];
        detail_only.terms[0].scope_targets = vec![" west ".into(), "east".into(), "east".into()];
        let mut duplicate = scope.grants[0].clone();
        duplicate.actions = vec!["detail".into()];
        scope.grants.extend([detail_only, duplicate]);
        value.data_scopes.push(scope);

        let (_, preview) = plan(value, &facts()).unwrap();
        assert_eq!(
            preview.changes.iter().map(|change| change.target.as_str()).collect::<Vec<_>>(),
            ["alice/sales_order/detail", "alice/sales_order/list"]
        );
        assert_eq!(preview.changes[0].after["alternatives"].as_array().unwrap().len(), 2);
        assert_eq!(
            preview.review_hash.as_str(),
            "sha256-v1:c78de8ed3986c04b891ddc79a0db77d732a82d4f149182082cdf8610adfd7e39"
        );
    }

    // 验证重导显式配置无差异，无法无损表达的旧完整范围拒绝导出。
    #[test]
    fn export_and_reimport_keep_current_explicit_authorization_unchanged() {
        let mut facts = facts();
        let mut row = PersonDataScope::default_for("alice", "sales_order", "list");
        row.expression = scope(&["list"]).request(4).unwrap().expression("list");
        facts.scopes.push(row);
        let exported = facts.export(4).unwrap();
        assert_eq!(exported.document.bindings[0].role_ids[0].as_str(), "reader");
        assert!(plan(exported.document, &facts).unwrap().1.changes.is_empty());

        facts.scopes[0].expression.additive = false;
        assert!(matches!(facts.export(4), Err(Error::ValidationError(_))));
    }

    // 验证已丧失动作资格的范围阻止导出，原有角色权限保持不变。
    #[test]
    fn export_refuses_dormant_scopes_instead_of_implicitly_restoring_action_permission() {
        let mut facts = facts();
        facts.scopes.push(PersonDataScope::default_for("alice", "sales_order", "update"));
        assert!(matches!(facts.export(4), Err(Error::ValidationError(_))));
        assert!(
            !facts.roles["reader"].permissions.contains(&Permission::parse("sales_order:update").unwrap())
        );
    }

    // 验证空替换清除指定角色的权限和人员绑定，未列出的人员范围保持原样。
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
        let (result, preview) = plan(value, &facts).unwrap();
        assert!(result.roles[0].permissions.is_empty());
        assert!(result.bindings[0].role_ids.is_empty());
        assert!(result.scopes.is_empty());
        assert_eq!(preview.changes.len(), 2);
    }

    // 验证重导空追加范围可清除导出之后新添的条款，并保留追加模型。
    #[test]
    fn exported_default_scopes_revoke_grants_added_after_export() {
        let mut facts = facts();
        let exported = facts.export(4).unwrap();
        let mut row = PersonDataScope::default_for("alice", "sales_order", "list");
        row.expression = scope(&["list"]).request(4).unwrap().expression("list");
        facts.scopes.push(row);
        let (restored, _) = plan(exported.document, &facts).unwrap();
        assert_eq!(restored.scopes.len(), 1);
        assert_eq!(restored.scopes[0].action, "list");
        assert!(restored.scopes[0].expression.additive);
        assert!(restored.scopes[0].expression.alternatives.is_empty());
    }

    // 验证条款和目标重排或重复时，规范化后的重导计划无差异。
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
        assert!(plan(exported.document, &facts).unwrap().1.changes.is_empty());
    }

    // 验证无本人基础的库存动作导出空追加项，任务和来源继承动作省略，重导无差异。
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
        assert!(!PersonScopeExpression::default_self(&scope.resource, "list"));
        assert!(plan(exported.document, &facts).unwrap().1.changes.is_empty());
    }

    // 验证共享资源保留角色权限，已退役范围仅记入说明，文件无法重新启用该范围。
    #[test]
    fn shared_scope_export_keeps_role_permissions_and_retires_old_limits() {
        let mut facts = facts();
        facts.roles.insert(
            "reader".into(),
            role("reader", &["product:list", "purchase_order:list", "sales_order:list"]),
        );
        let mut legacy = PersonDataScope::default_for("alice", "product", "list");
        legacy.expression.additive = false;
        legacy.expression.alternatives.clear();
        facts.scopes.push(legacy);
        let exported = facts.export(4).unwrap();
        assert_eq!(exported.document.data_scopes.len(), 1);
        assert_eq!(exported.document.data_scopes[0].resource, "sales_order");
        assert!(exported.policy_notes.iter().any(|note| note.contains("alice/product/list")));
        assert!(exported.document.roles[0].permissions.contains(&Permission::parse("product:list").unwrap()));
        assert!(plan(exported.document, &facts).unwrap().1.changes.is_empty());

        let mut invalid = document();
        let mut shared = scope(&["list"]);
        shared.resource = "product".into();
        invalid.data_scopes.push(shared);
        assert!(matches!(plan(invalid, &facts), Err(Error::ValidationError(_))));
    }
}
