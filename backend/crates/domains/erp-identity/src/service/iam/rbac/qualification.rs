//! 同一账号、策略和启用角色快照下的批量功能资格。
use application_core::AuditActor;
use persistence_core::Executor;

use super::{RbacService, RolePermissionSnapshot};
use crate::repository::prelude::*;
use crate::service::directory::AccountDirectoryService;
use crate::{
    AccessControlExt, AccountCore, AccountSummary, Error, Permission, PermissionSet, Result, Role, RoleId,
};

/// 批量功能资格结果，绑定已核验账号及启用角色；对象访问仍需完整授权。
#[derive(Debug, Clone)]
pub struct QualifiedPermissions {
    // 同一策略版本下的有效角色及逐角色权限命中。
    snapshot: RolePermissionSnapshot,
    // 取证时的无凭证账号摘要，供调用方使用同次读取的显示信息。
    account: AccountSummary,
    // 由账号、策略及有效角色版本组合的身份版本，供授权凭证复验。
    version: String,
    /// 本次取证读取的启用角色实体，供固定角色资格判断和数据范围解析复用。
    pub(crate) roles: Vec<Role>,
    /// 本次读取的账号记录版本，参与数据范围版本计算。
    pub(crate) account_version: u64,
}

impl QualifiedPermissions {
    /// 解析数据范围时，选取本次取证中各自覆盖全部要求的启用角色。
    pub(crate) fn roles_for_all<'a>(
        &'a self,
        required: &'a [Permission],
    ) -> impl Iterator<Item = &'a Role> + Clone {
        self.roles.iter().filter(move |role| self.snapshot.role_covers(&role.base.id, required.iter()))
    }

    /// 读取有效内部账号的无凭证摘要，供调用方使用与权限取证一致的显示名。
    pub fn account(&self) -> &AccountSummary {
        &self.account
    }

    /// 取得本批指定权限的启用授予角色，供单项范围解析使用。
    /// 未请求或无启用角色授予时返回空集合。
    pub fn roles(&self, permission: &Permission) -> Vec<RoleId> {
        self.snapshot.granting_role_ids(permission)
    }

    /// 检查是否有一个启用角色同时授予全部指定权限；空要求或没有共同角色时返回 `false`。
    pub fn same_role(&self, permissions: &[Permission]) -> bool {
        self.roles_for_all(permissions).next().is_some()
    }

    /// 检查同一启用角色覆盖必需的读取权限和完整执行子集。
    pub(crate) fn same_role_with(&self, read: &Permission, execution: &[Permission]) -> bool {
        self.roles
            .iter()
            .any(|role| self.snapshot.role_covers(&role.base.id, std::iter::once(read).chain(execution)))
    }

    /// 汇总本批由启用角色授予的具体权限，供逐项资格查询；各项可由不同角色授予。
    pub fn permissions(&self) -> PermissionSet {
        PermissionSet::new(self.snapshot.grants.values().flatten().cloned())
    }

    /// 返回本批取证的策略修订号，供原事务校验或授权后的策略写入使用。
    pub fn policy_revision(&self) -> u64 {
        self.snapshot.policy_revision()
    }

    /// 返回绑定账号、策略及有效角色版本的身份版本，供授权凭证记录和复验。
    pub fn version(&self) -> &str {
        &self.version
    }
}

impl RbacService {
    /// 为后续账号查询构造复用同一身份数据库的目录服务；此时不读库或建缓存。
    pub fn directory(&self) -> AccountDirectoryService {
        AccountDirectoryService::new(self.db.clone())
    }

    /// 为当前账号批量核验具体功能权限，并沿原执行器核对策略版本及启用角色。
    /// 账号失效返回 `None`，有效账号可以零项授权；通配请求、版本不一致或读取失败均返回错误。
    pub async fn qualified_permissions(
        &self,
        actor: &AuditActor,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<Option<QualifiedPermissions>> {
        for permission in permissions {
            if permission.resource().contains('*') || permission.action().contains('*') {
                return Err(Error::ValidationError("功能资格查询必须指定具体权限".into()));
            }
        }

        let account = self
            .db
            .accounts()
            .find_by_id(actor.id(), executor)
            .await?
            .filter(|account| account.is_authorization_actor(actor.kind()));
        let Some(account) = account else { return Ok(None) };

        Ok(Some(self.qualified_permissions_for_account(actor, &account, permissions, executor).await?))
    }

    /// 沿原执行器为已确认有效的账号核对策略版本与启用角色，返回对应批次的功能资格快照。
    pub(crate) async fn qualified_permissions_for_account(
        &self,
        actor: &AuditActor,
        account: &AccountCore,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<QualifiedPermissions> {
        let snapshot = self.role_permission_snapshot(actor.kind(), actor.id(), permissions).await?;
        let roles = self.db.roles().enabled_roles(snapshot.role_ids(), executor).await?;
        self.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
        Ok(qualified(snapshot, roles, actor, account.base.version, AccountSummary::from(account)))
    }
}

#[cfg(any(test, feature = "test-support"))]
impl QualifiedPermissions {
    /// 为默认后台测试账号构造同一快照下的角色授权，供同角色规则测试使用。
    #[cfg(test)]
    pub(crate) fn test_grants(grants: &[(&str, &[Permission])]) -> Self {
        use erp_core::AccountKind;

        let actor = AuditActor::new("user".into(), "login".into(), AccountKind::Admin);
        Self::test_grants_for(&actor, grants)
    }

    /// 为指定测试账号和启用角色构造内存资格，复用生产的资格过滤与版本计算。
    /// 测试角色编号或名称无效时触发断言。
    pub(crate) fn test_grants_for(actor: &AuditActor, grants: &[(&str, &[Permission])]) -> Self {
        use std::collections::{HashMap, HashSet};

        let mut role_ids = Vec::new();
        let mut map = HashMap::new();
        let mut roles = Vec::new();
        for (index, (id, permissions)) in grants.iter().enumerate() {
            let role_id = RoleId::parse(id).expect("测试角色编号");
            role_ids.push(role_id.clone());
            map.insert(role_id, permissions.iter().cloned().collect::<HashSet<_>>());
            let mut role = Role::new(RoleId::parse(id).expect("测试角色编号"), crate::RoleData::new(*id))
                .expect("测试角色");
            role.base = entity_core::BaseModel::fake();
            role.base.id = (*id).into();
            role.base.version = u64::try_from(index).expect("角色序号") + 1;
            roles.push(role);
        }
        let snapshot = RolePermissionSnapshot { role_ids, grants: map, policy_revision: 1 };
        qualified(snapshot, roles, actor, 1, AccountSummary::new(actor.id(), actor.kind(), true))
    }
}

// 取证完成后按启用角色过滤授权，并用账号、策略及本批启用角色版本构造身份版本。
fn qualified(
    mut snapshot: RolePermissionSnapshot,
    roles: Vec<Role>,
    actor: &AuditActor,
    account_version: u64,
    account: AccountSummary,
) -> QualifiedPermissions {
    snapshot.role_ids.retain(|id| roles.iter().any(|role| role.base.id == id.as_str()));
    snapshot.grants.retain(|id, _| snapshot.role_ids.contains(id));

    let mut versions = roles.iter().map(|role| (&role.base.id, role.base.version)).collect::<Vec<_>>();
    versions.sort();
    let version = format!(
        "{}:{:?}:{account_version}:{}:{versions:?}",
        actor.id(),
        actor.kind(),
        snapshot.policy_revision()
    );
    QualifiedPermissions { snapshot, account, version, roles, account_version }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use entity_core::BaseModel;
    use erp_core::AccountKind;

    use super::*;
    use crate::RoleData;

    // 验证逐项权限不能拼成同角色资格，停用角色的授予会从结果中剔除。
    #[test]
    fn independent_permissions_do_not_imply_a_common_role_and_disabled_roles_are_removed() {
        let read = Permission::parse("example:read").unwrap();
        let write = Permission::parse("example:write").unwrap();
        let a = RoleId::parse("a").unwrap();
        let b = RoleId::parse("b").unwrap();
        let snapshot = RolePermissionSnapshot {
            role_ids: vec![a.clone(), b.clone()],
            grants: HashMap::from([
                (a.clone(), HashSet::from([read.clone()])),
                (b.clone(), HashSet::from([write.clone()])),
            ]),
            policy_revision: 7,
        };
        let actor = AuditActor::new("user".into(), "login".into(), AccountKind::Admin);
        let roles = [role("a"), role("b")];
        let both = qualified(snapshot.clone(), roles.to_vec(), &actor, 1, account());
        assert!(both.permissions().covers_one(&read));
        assert!(both.permissions().covers_one(&write));
        assert!(!both.same_role(&[read.clone(), write.clone()]));
        assert!(both.same_role(std::slice::from_ref(&read)));
        assert!(!both.same_role(&[]));

        let reduced = qualified(snapshot, vec![role("a")], &actor, 1, account());
        assert!(reduced.permissions().covers_one(&read));
        assert!(!reduced.permissions().covers_one(&write));
        assert!(reduced.roles(&write).is_empty());
        assert_ne!(both.version(), reduced.version());
        assert_eq!(reduced.policy_revision(), 7);
    }

    // 验证同角色授予和账号摘要得以保留，账号或角色版本变化会更新身份版本。
    #[test]
    fn common_role_and_identity_versions_are_preserved() {
        let read = Permission::parse("example:read").unwrap();
        let write = Permission::parse("example:write").unwrap();
        let id = RoleId::parse("a").unwrap();
        let snapshot = RolePermissionSnapshot {
            role_ids: vec![id.clone()],
            grants: HashMap::from([(id, HashSet::from([read.clone(), write.clone()]))]),
            policy_revision: 7,
        };
        let actor = AuditActor::new("user".into(), "login".into(), AccountKind::Admin);
        let mut roles = [role("a")];
        let original = qualified(snapshot.clone(), roles.to_vec(), &actor, 1, account());
        assert!(original.same_role(&[read, write]));
        assert_eq!(original.account(), &account());
        assert_eq!(original.account().display_name, "审批人");

        assert_ne!(
            original.version(),
            qualified(snapshot.clone(), roles.to_vec(), &actor, 2, account()).version()
        );
        roles[0].base.version += 1;
        assert_ne!(
            original.version(),
            qualified(snapshot.clone(), roles.to_vec(), &actor, 1, account()).version()
        );

        let empty = qualified(snapshot, vec![], &actor, 1, account());
        assert!(empty.permissions().into_vec().is_empty());
    }

    // 验证固定角色和权限的资格判断只采纳启用角色证据。
    #[test]
    fn fixed_role_and_permission_policies_only_accept_enabled_role_evidence() {
        let preview = Permission::parse("file_asset:preview").unwrap();
        let id = RoleId::parse("role-sysadmin").unwrap();
        let snapshot = RolePermissionSnapshot {
            role_ids: vec![id.clone()],
            grants: HashMap::from([(id.clone(), HashSet::from([preview.clone()]))]),
            policy_revision: 7,
        };
        let actor = AuditActor::new("user".into(), "login".into(), AccountKind::Admin);
        let enabled = qualified(snapshot.clone(), vec![role(id.as_str())], &actor, 1, account());
        let disabled = qualified(snapshot, vec![], &actor, 1, account());
        assert_eq!(enabled.roles(&preview), vec![id]);
        assert!(disabled.roles(&preview).is_empty());
        assert!(enabled.same_role(std::slice::from_ref(&preview)));
        assert!(!disabled.same_role(std::slice::from_ref(&preview)));
        assert_ne!(enabled.version(), disabled.version());
    }

    // 为资格测试提供带固定显示名和登录名的有效后台账号摘要。
    fn account() -> AccountSummary {
        AccountSummary::new("user", AccountKind::Admin, true)
            .with_display_name("审批人")
            .with_login_account("login")
    }

    // 用指定 ID 构造启用角色，供资格测试匹配策略中的角色绑定。
    fn role(id: &str) -> Role {
        let mut role = Role::new(RoleId::parse(id).unwrap(), RoleData::new(id)).unwrap();
        role.base = BaseModel::fake();
        role.base.id = id.into();
        role
    }
}
