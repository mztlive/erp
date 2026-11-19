use persistence_core::NoTransaction;

use super::policy::{permissions_for_role, root_role_is_current};
use super::{ROOT_ROLE_ID, ROOT_ROLE_INIT_ATTEMPTS, ROOT_ROLE_NAME, SharedRbacService};
use crate::AccessControlExt;
use crate::entity::rbac::Permission;
use crate::entity::role::{Role, RoleData};
use crate::error::{Error, Result};

/// 启动或初始化超级管理员时，按固定 ID `role-root` 创建或修复角色及 `*:*` 策略，返回生效角色。
/// 遇到冲突错误时有限重试；读取或修复失败时返回错误。
/// 不写入或恢复人员数据范围；全公司数据范围由公共解析器提供，不依赖人员范围初始化或历史记录。
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

// 每次初始化尝试读取固定角色（含软删除）和直接权限；已满足规范则直接返回，否则创建或修复。
// 写入仅涉及角色与策略；角色读取、Enforcer 刷新、策略解析或写入失败时返回错误。
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
