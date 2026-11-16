use std::collections::{HashMap, HashSet};
use std::result::Result as StdResult;
use std::sync::Arc;

use application_core::AuditActor;
use casbin::{CoreApi, DefaultModel, Enforcer, MemoryAdapter, RbacApi};
use erp_core::AccountKind;
use erp_permissions::admin;
use persistence_core::{Executor, Transactional};

use super::policy::{
    collect_role_ids, collect_role_permissions, implicit_permissions_for_role, parse_policy_permissions,
    permissions_for_actor, role_ids_for_account, role_key,
};
use super::{RBAC_MODEL, RbacService, subject};
use crate::dto::RoleItem;
use crate::entity::rbac::{PermissionSet, RoleId, RoleIdSet};
use crate::entity::role::Role;
use crate::error::Result;
use crate::repository::prelude::*;
use crate::service::iam::management::ManagementAccess;
use crate::{AccessControlExt, AuthorizationService, MongoCasbinAdapter};

/// 由批量直接权限装配角色响应项（两个列表入口共用该映射）。
///
/// # 参数
/// * `roles` - ID 不重复的待装配角色
/// * `permissions` - 包含全部待装配角色 ID 的直接权限；命中项会被取出
///
/// # 返回
/// 与输入角色一一对应的响应项；没有权限的角色使用映射中已有的空权限集合。
///
/// # Panics
/// 角色 ID 重复或权限映射缺少待装配角色时触发，表示调用方破坏了批量装配不变式。
fn role_items_from_map(roles: Vec<Role>, permissions: &mut HashMap<String, PermissionSet>) -> Vec<RoleItem> {
    roles
        .into_iter()
        .map(|role| {
            let role_permissions =
                permissions.remove(&role.base.id).expect("权限映射由全部角色 ID 构造，且每个角色仅装配一次");
            RoleItem::from_role(role, role_permissions.into_vec())
        })
        .collect()
}

impl RbacService {
    /// 在调用方事务中读取仍启用的直接账号角色。
    /// # 参数
    /// account_kind、account_id 为账号身份，executor 为原事务执行器。
    /// # 返回
    /// 返回排序去重且仍启用的角色 ID。
    /// # 错误
    /// 角色身份非法或数据库读取失败时返回错误。
    pub async fn role_ids_with_executor(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<RoleId>> {
        let keys = MongoCasbinAdapter::new(self.db.clone())
            .subject_roles(&subject(account_kind, account_id), executor)
            .await?;
        let mut ids = keys
            .iter()
            .filter_map(|key| key.strip_prefix("role:"))
            .map(RoleId::parse)
            .collect::<StdResult<Vec<_>, _>>()?;
        ids.sort();
        ids.dedup();
        let enabled = self.enabled_role_ids(&ids, executor).await?;
        ids.retain(|id| enabled.contains(id));
        Ok(ids)
    }

    /// 保留调用方快照中仍启用的角色。
    /// # 参数
    /// ids 为候选角色，executor 沿用调用方事务。
    /// # 返回
    /// 返回当前启用的类型化角色 ID；空候选不查询数据库。
    /// # 错误
    /// 角色数据非法或数据库读取失败时返回错误。
    pub async fn enabled_role_ids(&self, ids: &[RoleId], executor: &mut dyn Executor) -> Result<Vec<RoleId>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .db
            .roles()
            .enabled_roles(ids, executor)
            .await?
            .into_iter()
            .map(|role| RoleId::parse(role.base.id))
            .collect::<StdResult<Vec<_>, _>>()?)
    }

    /// 查询全部角色及其直接权限。
    ///
    /// # 参数
    /// actor 为当前身份，authorization 为应用注入的共享授权服务。
    /// 无。
    ///
    /// # 返回
    /// 返回带直接权限的角色列表；没有角色时为空列表。
    ///
    /// # 错误
    /// 当 MongoDB 或 Casbin policy 查询失败时返回错误。
    pub async fn role_list(
        self: &Arc<Self>,
        actor: &AuditActor,
        authorization: Arc<AuthorizationService>,
    ) -> Result<Vec<RoleItem>> {
        let service = self.clone();
        let access = ManagementAccess::new(authorization, actor.clone(), &admin::ROLE_LIST_ROLES);
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    access.require(executor).await?;
                    let roles = service.db.roles().list_all(executor).await?;
                    let policy = service.role_query_policy(&roles, None, executor).await?;
                    role_items(roles, &policy)
                })
            })
            .await
    }

    /// 查询全部可分配角色及其直接权限。
    ///
    /// # 参数
    /// actor 为当前身份，authorization 为应用注入的共享授权服务。
    /// * `actor` - 已认证操作人，用于按自身权限过滤。
    ///
    /// # 返回
    /// 返回操作人权限能够覆盖、且允许普通分配的角色。
    ///
    /// # 错误
    /// 当 MongoDB 或 Casbin policy 查询失败时返回错误。
    pub async fn assignable_role_list(
        self: &Arc<Self>,
        actor: &AuditActor,
        authorization: Arc<AuthorizationService>,
    ) -> Result<Vec<RoleItem>> {
        let service = self.clone();
        let actor = actor.clone();
        let access = ManagementAccess::new(authorization, actor.clone(), &admin::ROLE_LIST_ASSIGNABLE_ROLES);
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    access.require(executor).await?;
                    let roles = service
                        .db
                        .roles()
                        .list_enabled(executor)
                        .await?
                        .into_iter()
                        .filter(|role| role.ensure_assignable().is_ok())
                        .collect::<Vec<_>>();
                    let policy = service.role_query_policy(&roles, Some(&actor), executor).await?;
                    assignable_role_items(roles, &actor, &policy)
                })
            })
            .await
    }

    /// 沿原执行器读取当前批次及其继承策略，临时计算不修改共享 Enforcer。
    async fn role_query_policy(
        &self,
        roles: &[Role],
        actor: Option<&AuditActor>,
        executor: &mut dyn Executor,
    ) -> Result<Enforcer> {
        self.ensure_policy_consistency_known()?;
        let subjects = roles.iter().map(|role| role_key(&role.base.id));
        let mut policy = QueryPolicy::new(subjects.collect()).await?;
        if roles.is_empty() {
            return Ok(policy.enforcer);
        }
        if let Some(actor) = actor {
            policy.pending.push(subject(actor.kind(), actor.id()));
        }
        loop {
            let subjects = policy.next_subjects();
            if subjects.is_empty() {
                break;
            }
            let mut rows = self.policy_store.subject_policies(&subjects, executor).await?;
            for subject in subjects {
                let row = rows.remove(&subject).unwrap_or_default();
                policy.include(&subject, row.roles, row.permissions).await?;
            }
        }
        Ok(policy.enforcer)
    }

    /// 查询账号直接绑定的角色 ID。
    ///
    /// # 参数
    /// * `account_kind` - 账号类型。
    /// * `account_id` - 账号 ID。
    ///
    /// # 返回
    /// 返回去掉 `role:` 前缀的直接角色 ID；未绑定时为空列表。
    ///
    /// # 错误
    /// 当 Casbin policy 加载失败时返回错误。
    pub async fn role_ids(&self, account_kind: AccountKind, account_id: &str) -> Result<Vec<RoleId>> {
        let enforcer = self.fresh_enforcer().await?.read().await;
        role_ids_for_account(&enforcer, account_kind, account_id)
    }

    /// 读取直接绑定某稳定角色的后台账号 ID。
    ///
    /// # 参数
    /// * `role_id` - 稳定角色 ID，不含 `role:` 前缀
    ///
    /// # 返回
    /// 返回排序后的后台账号 ID。非后台主体被忽略。
    ///
    /// # 错误
    /// 策略加载失败时返回错误。
    ///
    /// # 关键业务约束
    /// 只反映当前 Casbin 绑定，不把角色撤销解释成查询资格终止。
    pub async fn direct_admin_ids_for_role(&self, role_id: &str) -> Result<Vec<String>> {
        let enforcer = self.fresh_enforcer().await?.read().await;
        let mut ids = enforcer
            .get_users_for_role(&role_key(role_id), None)
            .into_iter()
            .filter_map(|subject| {
                subject.strip_prefix("user:admin:").filter(|id| !id.is_empty()).map(str::to_string)
            })
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    /// 批量查询同类账号直接绑定的角色 ID。
    ///
    /// # 参数
    /// * `account_kind` - 账号类型。
    /// * `account_ids` - 待查询的账号 ID 集合。
    ///
    /// # 返回
    /// 返回账号 ID 到角色 ID 集合的映射；未绑定角色的账号对应空集合。
    ///
    /// # 错误
    /// 当 Casbin policy 加载失败时返回错误。
    pub async fn role_ids_by_accounts(
        &self,
        account_kind: AccountKind,
        account_ids: &[String],
    ) -> Result<HashMap<String, Vec<RoleId>>> {
        if account_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let enforcer = self.fresh_enforcer().await?.read().await;
        collect_role_ids(&enforcer, account_kind, account_ids)
    }

    /// 在账号列表的原快照内批量读取直接角色绑定。
    /// # 参数
    /// account_kind、account_ids 为本批账号，executor 为列表授权的原执行器。
    /// # 返回
    /// 每个账号均有一项，未绑定角色时返回空集合。
    /// # 错误
    /// 策略损坏、角色身份非法或数据库读取失败时拒绝。
    pub(crate) async fn account_roles(
        &self,
        account_kind: AccountKind,
        account_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<RoleId>>> {
        let mut result = HashMap::new();
        for batch in account_ids.chunks(500) {
            let subjects = batch.iter().map(|id| subject(account_kind, id)).collect::<Vec<_>>();
            let mut rows = self.policy_store.subject_policies(&subjects, executor).await?;
            for (id, key) in batch.iter().zip(subjects) {
                let roles = RoleIdSet::from_casbin_role_keys(rows.remove(&key).unwrap_or_default().roles)?;
                result.insert(id.clone(), roles.into());
            }
        }
        Ok(result)
    }

    /// 查询账号通过角色继承获得的权限。
    ///
    /// # 参数
    /// * `account_kind` - 账号类型。
    /// * `account_id` - 账号 ID。
    ///
    /// # 返回
    /// 返回去重后的隐式权限。
    ///
    /// # 错误
    /// Casbin policy 无法加载，或策略行无法解析为权限时返回错误。
    pub async fn permissions(&self, account_kind: AccountKind, account_id: &str) -> Result<PermissionSet> {
        let policies = self
            .fresh_enforcer()
            .await?
            .read()
            .await
            .get_implicit_permissions_for_user(&subject(account_kind, account_id), None);
        parse_policy_permissions(policies)
    }
}

/// 一次角色查询私有的策略图；每个主体只沿当前事务读取一次。
struct QueryPolicy {
    pending: Vec<String>,
    visited: HashSet<String>,
    enforcer: Enforcer,
}

impl QueryPolicy {
    /// 创建不连接数据库、也不保存策略的临时 Casbin 模型。
    async fn new(subjects: Vec<String>) -> Result<Self> {
        let model = DefaultModel::from_str(RBAC_MODEL).await?;
        let mut enforcer = Enforcer::new(model, MemoryAdapter::default()).await?;
        enforcer.enable_auto_save(false);
        Ok(Self { pending: subjects, visited: HashSet::new(), enforcer })
    }

    /// 跳过已读取主体，包含重复引用或继承循环时也不重复取证。
    fn next_subjects(&mut self) -> Vec<String> {
        let mut subjects = Vec::new();
        while subjects.len() < 500 {
            let Some(subject) = self.pending.pop() else { break };
            if self.visited.insert(subject.clone()) {
                subjects.push(subject);
            }
        }
        subjects
    }

    /// 只装载原执行器读取的直接权限与完整父角色，不借用共享缓存。
    async fn include(
        &mut self,
        subject: &str,
        inherited: Vec<String>,
        permissions: Vec<(String, String)>,
    ) -> Result<()> {
        for parent in inherited {
            self.enforcer.add_role_for_user(subject, &parent, None).await?;
            self.pending.push(parent);
        }
        for (resource, action) in permissions {
            self.enforcer.add_permission_for_user(subject, vec![resource, action]).await?;
        }
        Ok(())
    }
}

/// 直接权限仅用于响应内容，保持原角色顺序和空权限角色。
fn role_items(roles: Vec<Role>, policy: &Enforcer) -> Result<Vec<RoleItem>> {
    let ids = roles.iter().map(|role| role.base.id.clone()).collect::<Vec<_>>();
    let mut permissions = collect_role_permissions(policy, &ids)?;
    Ok(role_items_from_map(roles, &mut permissions))
}

/// 用同一事务策略图中的隐式权限检查授予上限，响应仍只输出角色直接权限。
fn assignable_role_items(roles: Vec<Role>, actor: &AuditActor, policy: &Enforcer) -> Result<Vec<RoleItem>> {
    let actor_permissions = permissions_for_actor(policy, actor)?;
    let mut manageable = Vec::with_capacity(roles.len());
    for role in roles {
        let required = implicit_permissions_for_role(policy, &role.base.id)?;
        if actor_permissions.covers(&required) {
            manageable.push(role);
        }
    }
    role_items(manageable, policy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Error, Permission, RoleData};

    /// 实际批量读取结果可装配完整列表或筛选子集，空权限角色不会缺失。
    #[tokio::test]
    async fn role_items_preserve_order_and_empty_permissions() {
        let model = DefaultModel::from_str(RBAC_MODEL).await.unwrap();
        let mut enforcer = Enforcer::new(model, MemoryAdapter::default()).await.unwrap();
        enforcer
            .add_permission_for_user(&role_key("role-a"), vec!["customer".into(), "list".into()])
            .await
            .unwrap();
        let roles = ["role-b", "role-a"]
            .into_iter()
            .map(|id| Role::new(RoleId::parse(id).unwrap(), RoleData::new(id)).unwrap())
            .collect::<Vec<_>>();
        let ids = roles.iter().map(|role| role.base.id.clone()).collect::<Vec<_>>();
        let mut permissions = collect_role_permissions(&enforcer, &ids).unwrap();
        let items = role_items_from_map(roles.clone(), &mut permissions);
        assert_eq!(items[0].id, "role-b");
        assert!(items[0].permissions.is_empty());
        assert_eq!(items[1].id, "role-a");
        assert_eq!(items[1].permissions, vec![Permission::parse("customer:list").unwrap()]);

        let mut permissions = collect_role_permissions(&enforcer, &ids).unwrap();
        let items = role_items_from_map(vec![roles[0].clone()], &mut permissions);
        assert_eq!(items.len(), 1);
        assert!(items[0].permissions.is_empty());
    }

    /// 角色响应和授予上限使用冻结策略，后续策略修改不混入当前列表。
    #[tokio::test]
    async fn role_queries_keep_direct_permissions_and_inherited_limits_in_one_snapshot() {
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let roles = vec![role("candidate"), role("empty")];
        let frozen = query_policy("customer", "customer").await;
        let current = query_policy("customer", "supplier").await;

        let items = role_items(roles.clone(), &frozen.enforcer).unwrap();
        assert_eq!(items.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(), ["candidate", "empty"]);
        assert_eq!(items[0].permissions, [Permission::parse("contract:list").unwrap()]);
        assert!(items[1].permissions.is_empty());
        let allowed = assignable_role_items(roles.clone(), &actor, &frozen.enforcer).unwrap();
        assert_eq!(allowed.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(), ["candidate", "empty"]);
        let changed = assignable_role_items(roles.clone(), &actor, &current.enforcer).unwrap();
        assert_eq!(changed.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(), ["empty"]);
        assert_eq!(
            assignable_role_items(roles, &actor, &frozen.enforcer).unwrap()[0].permissions,
            allowed[0].permissions
        );
    }

    /// 遍历完整父角色，但重复引用及循环不会重复读取同一主体。
    #[tokio::test]
    async fn query_policy_visits_each_subject_once_even_with_cycles() {
        let mut policy = QueryPolicy::new(vec![role_key("a"), role_key("a")]).await.unwrap();
        assert_eq!(policy.next_subjects(), ["role:a"]);
        policy.include("role:a", vec![role_key("b"), role_key("b")], vec![]).await.unwrap();
        assert_eq!(policy.next_subjects(), ["role:b"]);
        policy.include("role:b", vec![role_key("a")], vec![]).await.unwrap();
        assert!(policy.next_subjects().is_empty());
        assert_eq!(policy.visited, HashSet::from([role_key("a"), role_key("b")]));
    }

    /// 损坏策略不能被列表输出或可分配性判断降级为零权限。
    #[tokio::test]
    async fn role_queries_reject_invalid_snapshot_permissions() {
        let mut policy = QueryPolicy::new(vec![]).await.unwrap();
        policy.include("role:bad", vec![], vec![("bad resource".into(), "list".into())]).await.unwrap();
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        assert!(matches!(role_items(vec![role("bad")], &policy.enforcer), Err(Error::Logic(_))));
        assert!(matches!(
            assignable_role_items(vec![role("bad")], &actor, &policy.enforcer),
            Err(Error::Logic(_))
        ));
    }

    /// 使用生产策略装载步骤构造两条独立的角色继承链。
    async fn query_policy(actor_resource: &str, candidate_resource: &str) -> QueryPolicy {
        let mut policy = QueryPolicy::new(vec![]).await.unwrap();
        policy.include("user:admin:actor", vec![role_key("actor")], vec![]).await.unwrap();
        for (id, parent) in [("actor", "actor-parent"), ("candidate", "candidate-parent")] {
            policy
                .include(&role_key(id), vec![role_key(parent)], vec![("contract".into(), "list".into())])
                .await
                .unwrap();
        }
        for (id, resource) in [("actor-parent", actor_resource), ("candidate-parent", candidate_resource)] {
            policy.include(&role_key(id), vec![], vec![(resource.into(), "*".into())]).await.unwrap();
        }
        policy
    }

    /// 构造可普通分配的生产角色实体。
    fn role(id: &str) -> Role {
        Role::new(RoleId::parse(id).unwrap(), RoleData::new(id)).unwrap()
    }
}
