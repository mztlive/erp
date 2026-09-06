use database::{AccessControlExt, NoTransaction};
use entities::{Permission, Role, RoleData};

use super::{
    policy::{permissions_for_role, root_role_is_current},
    SharedRbacService, ROOT_ROLE_ID, ROOT_ROLE_INIT_ATTEMPTS, ROOT_ROLE_NAME,
};
use crate::errors::{Error, Result};

/// 确保 root 角色存在且拥有全量权限。
///
/// # 错误
/// 当角色或 Casbin policy 写入失败时返回错误。
pub async fn ensure_root_role(rbac: &SharedRbacService) -> Result<Role> {
    let root_permission = Permission::parse("*:*")?;
    let mut attempts = 0;
    loop {
        attempts += 1;
        let result = ensure_root_role_once(rbac, &root_permission).await;
        match result {
            Err(Error::ConflictError(_)) if attempts < ROOT_ROLE_INIT_ATTEMPTS => continue,
            result => return result,
        }
    }
}

/// 执行一次可重入的 root 角色校验或修复。
async fn ensure_root_role_once(rbac: &SharedRbacService, root_permission: &Permission) -> Result<Role> {
    if let Some(role) = rbac
        .db
        .roles()
        .find_by_id_including_deleted(ROOT_ROLE_ID, &mut NoTransaction)
        .await?
    {
        let enforcer = rbac.fresh_enforcer().await?.read().await;
        let permissions = permissions_for_role(&enforcer, ROOT_ROLE_ID)?;
        drop(enforcer);
        if root_role_is_current(&role, &permissions, root_permission) {
            return Ok(role);
        }
        return rbac.repair_root_role(role, root_permission.clone()).await;
    }

    let data = RoleData {
        name: ROOT_ROLE_NAME.to_string(),
        description: None,
        system: true,
    };
    rbac.create_role_with_id(
        ROOT_ROLE_ID.to_string(),
        data,
        vec![root_permission.clone()],
        None,
        None,
    )
    .await
}
