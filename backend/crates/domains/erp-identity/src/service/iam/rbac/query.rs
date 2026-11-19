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

// 两个角色列表共用此装配步骤：按输入顺序取出各角色的直接权限，也保留空权限角色。
// 权限映射须覆盖所有待装配角色；角色 ID 重复或缺少映射项时触发 panic。
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
    /// 需要在原事务确认账号的有效直接绑定时，读取 Casbin 绑定和启用状态并返回排序去重后的角色 ID。
    /// 角色 ID 非法或数据库读取失败时返回错误。
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

    /// 沿调用方执行器将候选筛为快照中仍启用的角色 ID；空候选直接返回空列表。
    /// 角色 ID 非法或数据库读取失败时返回错误。
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

    /// 管理端查看角色时，在同一事务内检查列表权限并读取全部角色及其直接权限。
    /// 无角色时返回空列表；授权、数据库或策略读取失败时返回错误。
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

    /// 为普通角色分配提供候选：在同一事务中检查入口权限，并筛出启用、可分配且实际权限受操作人覆盖的角色。
    /// 授予上限包含继承权限，响应只展示直接权限；授权、数据库或策略读取失败时返回错误。
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

    // 装配列表前沿原执行器加载角色及其继承链；需检查授予上限时一并加载操作人的策略。
    // 结果只写入本次查询的临时 Enforcer。
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

    /// 从当前 Casbin 策略读取账号的直接绑定，返回去掉 `role:` 前缀的角色 ID；未绑定时为空列表。
    /// 此入口只读绑定，策略加载或角色 ID 解析失败时返回错误。
    pub async fn role_ids(&self, account_kind: AccountKind, account_id: &str) -> Result<Vec<RoleId>> {
        let enforcer = self.fresh_enforcer().await?.read().await;
        role_ids_for_account(&enforcer, account_kind, account_id)
    }

    /// 按固定角色 ID（不含 `role:` 前缀）读取直接绑定的后台账号，供人员目录资格同步使用。
    /// 以当前 Casbin 绑定为准，`user_roles` 的撤销留痕不改变本查询结果；忽略非后台主体并排序去重。
    /// 策略加载失败时返回错误。
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

    /// 从当前 Casbin 策略批量读取同类账号的直接角色绑定；未绑定账号对应空列表。
    /// 空批次直接返回空映射；策略加载或角色 ID 解析失败时返回错误。
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

    /// 装配账号列表时沿授权使用的执行器批量读取直接角色绑定；未绑定账号对应空列表。
    /// 策略损坏、角色 ID 非法或数据库读取失败时返回错误。
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

    /// 装配账号资料等展示信息时，从当前 Casbin 策略汇总账号的隐式权限（含角色继承）并去重。
    /// 策略加载或权限解析失败时返回错误。
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

// 一次角色列表查询的临时策略图；每个主体只沿原事务读取一次。
struct QueryPolicy {
    // 待展开的 Casbin 主体，包含查询起点和发现的父角色。
    pending: Vec<String>,
    // 已安排读取的主体，用于去重及终止继承环。
    visited: HashSet<String>,
    // 本次查询的内存 Enforcer，用于读取直接权限和计算继承权限。
    enforcer: Enforcer,
}

impl QueryPolicy {
    // 从起始主体建立内存策略图，供列表在原快照内计算；关闭自动持久化。
    async fn new(subjects: Vec<String>) -> Result<Self> {
        let model = DefaultModel::from_str(RBAC_MODEL).await?;
        let mut enforcer = Enforcer::new(model, MemoryAdapter::default()).await?;
        enforcer.enable_auto_save(false);
        Ok(Self { pending: subjects, visited: HashSet::new(), enforcer })
    }

    // 为下一次批量读取取出至多 500 个未访问主体；重复引用和继承环在此去重。
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

    // 读到主体策略后，将直接权限和继承边写入临时策略图，并把父角色排入后续读取。
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

// 列表返回前按输入顺序装配直接权限，保留没有直接权限的角色。
fn role_items(roles: Vec<Role>, policy: &Enforcer) -> Result<Vec<RoleItem>> {
    let ids = roles.iter().map(|role| role.base.id.clone()).collect::<Vec<_>>();
    let mut permissions = collect_role_permissions(policy, &ids)?;
    Ok(role_items_from_map(roles, &mut permissions))
}

// 装配分配候选时，用同一事务策略图核对操作人能否覆盖候选的继承权限，响应只展示直接权限。
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

    // 验证批量权限映射可按输入顺序装配完整列表或子集，并保留空权限角色。
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

    // 验证直接权限展示和继承权限上限均使用同一冻结策略，隔离后续策略变化。
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

    // 验证完整遍历父角色时，重复引用和继承环中的每个主体只进入一次读取批次。
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

    // 验证直接权限列表和可分配性判断均会拒绝损坏策略，避免将错误当成零权限。
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

    // 通过生产装载入口构造操作人和候选角色的独立继承链，供授予上限测试使用。
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

    // 为角色查询单测创建可普通分配的非系统角色。
    fn role(id: &str) -> Role {
        Role::new(RoleId::parse(id).unwrap(), RoleData::new(id)).unwrap()
    }
}
