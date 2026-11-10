use persistence_core::NoTransaction;

use super::policy::{permissions_for_role, root_role_is_current};
use super::{ROOT_ROLE_ID, ROOT_ROLE_INIT_ATTEMPTS, ROOT_ROLE_NAME, SharedRbacService};
use crate::AccessControlExt;
use crate::entity::rbac::Permission;
use crate::entity::role::{Role, RoleData};
use crate::error::{Error, Result};

/// 确保 root 角色存在且拥有全量操作权限。
///
/// # 参数
/// * `rbac` - 共享 RBAC 服务
///
/// # 返回
/// 返回当前生效的超级管理员角色。
///
/// # 错误
/// 角色或 Casbin policy 写入失败时返回错误。
///
/// # 业务约束
/// 角色元数据与 `*:*` policy 按固定 root ID 修复；不写入或恢复业务数据范围。
/// 内建超级管理员的全公司数据范围由公共解析器提供，不依赖人员范围初始化或旧记录。
pub async fn ensure_root_role(rbac: &SharedRbacService) -> Result<Role> {
    let root_permission = Permission::parse("*:*")?;
    let mut attempts = 0;
    let role = loop {
        attempts += 1;
        match ensure_root_role_once(rbac, &root_permission).await {
            Err(Error::ConflictError(_)) if attempts < ROOT_ROLE_INIT_ATTEMPTS => continue,
            result => break result?,
        }
    };
    Ok(role)
}

/// 执行一次可重入的 root 角色校验或修复。
///
/// # 参数
/// * `rbac` - 共享 RBAC 服务
/// * `root_permission` - 固定的 `*:*` 权限
///
/// # 返回值
/// 返回已存在且元数据/policy 正确，或本次创建/修复后的角色。
///
/// # 错误
/// 角色读取、Enforcer 刷新、policy 解析或创建/修复写入失败时返回错误。
///
/// # 业务约束
/// 本方法不处理 DataScope，不得通过角色修复补齐业务范围。
async fn ensure_root_role_once(rbac: &SharedRbacService, root_permission: &Permission) -> Result<Role> {
    if let Some(role) = rbac.db.roles().find_by_id_including_deleted(ROOT_ROLE_ID, &mut NoTransaction).await?
    {
        let enforcer = rbac.fresh_enforcer().await?.read().await;
        let permissions = permissions_for_role(&enforcer, ROOT_ROLE_ID)?;
        drop(enforcer);
        if root_role_is_current(&role, permissions.as_slice(), root_permission) {
            return Ok(role);
        }
        return rbac.repair_root_role(role, root_permission.clone()).await;
    }

    let data = RoleData::new(ROOT_ROLE_NAME.to_string()).with_system(true);
    rbac.create_role_with_id(ROOT_ROLE_ID.to_string(), data, vec![root_permission.clone()], None, None).await
}
