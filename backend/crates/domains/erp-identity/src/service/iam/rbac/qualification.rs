//! 同一账号、策略和启用角色快照下的批量功能资格。
use application_core::AuditActor;
use persistence_core::Executor;

use super::{RbacService, RolePermissionSnapshot};
use crate::repository::prelude::*;
use crate::service::directory::AccountDirectoryService;
use crate::{AccessControlExt, Error, Permission, PermissionSet, Result, Role, RoleId};

/// 已核验账号及启用角色的功能资格；不授予对象访问权。
#[derive(Debug, Clone)]
pub struct QualifiedPermissions {
    snapshot: RolePermissionSnapshot,
    version: String,
}

impl QualifiedPermissions {
    /// 取得单项权限的有效授权角色。
    /// # 参数
    /// permission 为本批取证的具体权限。
    /// # 返回
    /// 未请求、未授权或角色停用时返回空集合。
    /// # 错误
    /// 无。
    pub fn roles(&self, permission: &Permission) -> Vec<RoleId> {
        self.snapshot.granting_role_ids(permission)
    }

    /// 检查全部权限是否由同一有效角色授予。
    /// # 参数
    /// permissions 为必须同角色覆盖的权限。
    /// # 返回
    /// 空要求或没有共同角色时返回 false。
    /// # 错误
    /// 无。
    pub fn same_role(&self, permissions: &[Permission]) -> bool {
        !self.snapshot.granting_role_ids_for_all(permissions).is_empty()
    }

    /// 返回逐项独立成立的权限集合，允许不同权限来自不同角色。
    /// # 参数
    /// 无。
    /// # 返回
    /// 只包含本批请求且由启用角色授予的具体权限。
    /// # 错误
    /// 无。
    pub fn permissions(&self) -> PermissionSet {
        PermissionSet::new(self.snapshot.grants.values().flatten().cloned())
    }

    /// 返回本批取证的策略修订号。
    /// # 参数
    /// 无。
    /// # 返回
    /// 供原事务继续校验或策略写入使用的修订号。
    /// # 错误
    /// 无。
    pub fn policy_revision(&self) -> u64 {
        self.snapshot.policy_revision()
    }

    /// 返回绑定账号、策略及有效角色版本的身份版本。
    /// # 参数
    /// 无。
    /// # 返回
    /// 用于授权凭证的完整身份依赖。
    /// # 错误
    /// 无。
    pub fn version(&self) -> &str {
        &self.version
    }
}

impl RbacService {
    /// 取得同一身份数据库的账号目录。
    /// # 参数
    /// 无。
    /// # 返回
    /// 不查询、不缓存的身份目录服务。
    /// # 错误
    /// 无。
    pub fn directory(&self) -> AccountDirectoryService {
        AccountDirectoryService::new(self.db.clone())
    }

    /// 一次读取账号、权限快照及启用角色，分别证明本批具体权限。
    /// # 参数
    /// actor 为真实账号，permissions 为具体功能权限，executor 沿用业务事务。
    /// # 返回
    /// 账号失效时返回 None；有效账号返回逐项资格，允许零项授权。
    /// # 错误
    /// 通配查询、基础设施或策略版本错误保持失败；不降级为空权限。
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
        let snapshot = self.role_permission_snapshot(actor.kind(), actor.id(), permissions).await?;
        let roles = self.db.roles().enabled_roles(snapshot.role_ids(), executor).await?;
        self.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
        Ok(Some(qualified(snapshot, &roles, actor, account.base.version)))
    }
}

/// 只保留启用角色的授权，身份版本包含所有参与取证角色。
fn qualified(
    mut snapshot: RolePermissionSnapshot,
    roles: &[Role],
    actor: &AuditActor,
    account_version: u64,
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
    QualifiedPermissions { snapshot, version }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use entity_core::BaseModel;
    use erp_core::AccountKind;

    use super::*;
    use crate::RoleData;

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
        let both = qualified(snapshot.clone(), &roles, &actor, 1);
        assert!(both.permissions().covers_one(&read));
        assert!(both.permissions().covers_one(&write));
        assert!(!both.same_role(&[read.clone(), write.clone()]));
        assert!(both.same_role(std::slice::from_ref(&read)));
        assert!(!both.same_role(&[]));
        let reduced = qualified(snapshot, &roles[..1], &actor, 1);
        assert!(reduced.permissions().covers_one(&read));
        assert!(!reduced.permissions().covers_one(&write));
        assert!(reduced.roles(&write).is_empty());
        assert_ne!(both.version(), reduced.version());
        assert_eq!(reduced.policy_revision(), 7);
    }

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
        let original = qualified(snapshot.clone(), &roles, &actor, 1);
        assert!(original.same_role(&[read, write]));
        assert_ne!(original.version(), qualified(snapshot.clone(), &roles, &actor, 2).version());
        roles[0].base.version += 1;
        assert_ne!(original.version(), qualified(snapshot.clone(), &roles, &actor, 1).version());
        let empty = qualified(snapshot, &[], &actor, 1);
        assert!(empty.permissions().into_vec().is_empty());
    }

    fn role(id: &str) -> Role {
        let mut role = Role::new(RoleId::parse(id).unwrap(), RoleData::new(id)).unwrap();
        role.base = BaseModel::fake();
        role.base.id = id.into();
        role
    }
}
