//! 在原事务中读取账号及启用角色的完整权限，供跨域责任指派使用。
use std::collections::HashSet;

use mongodb::Database;
use persistence_core::Executor;

use crate::{
    AccessControlExt, AccountCore, Error, MongoCasbinAdapter, Permission, PermissionSet, Result, subject,
};

/// 核验有效后台账号及至少一个启用角色同时覆盖全部所需权限。
/// # 参数
/// 数据库、人员、所需权限与调用方执行器。
/// # 返回
/// 返回已核验的账号；不授予普通业务数据范围。
/// # 错误
/// 账号失效、角色或权限缺失、继承超限及读取失败时拒绝。
pub async fn qualified_account(
    db: &Database,
    user: &str,
    required: &[&str],
    ex: &mut dyn Executor,
) -> Result<AccountCore> {
    let account = db
        .accounts()
        .find_by_id(user, ex)
        .await?
        .filter(AccountCore::is_active_backoffice)
        .ok_or_else(|| Error::ValidationError("审批负责人账号不存在或已停用，请调整负责人".into()))?;
    let store = MongoCasbinAdapter::new(db.clone());
    let required = PermissionSet::new(
        required
            .iter()
            .map(|value| Permission::parse(value).map_err(Error::from))
            .collect::<Result<Vec<_>>>()?,
    );
    for key in store.subject_roles(&subject(account.kind, user), ex).await? {
        let Some(id) = key.strip_prefix("role:") else { continue };
        if db.roles().find_by_id(id, ex).await?.is_none_or(|role| role.disabled) {
            continue;
        }
        if inherited_permissions(&store, key, ex).await?.covers(&required) {
            return Ok(account);
        }
    }
    Err(Error::ValidationError("审批负责人缺少审批读取或决定权限，请联系管理员配置角色".into()))
}

/// 有界展开 Casbin 角色继承，复用 PermissionSet 的资源及动作覆盖规则。
async fn inherited_permissions(
    store: &MongoCasbinAdapter,
    key: String,
    ex: &mut dyn Executor,
) -> Result<PermissionSet> {
    let mut pending = vec![key];
    let mut seen = HashSet::new();
    let mut permissions = Vec::new();
    while let Some(key) = pending.pop() {
        if !seen.insert(key.clone()) {
            continue;
        }
        if seen.len() > 1000 {
            return Err(Error::ValidationError("角色继承超过1000项，请检查权限配置".into()));
        }
        for (resource, action) in store.role_permissions(&key, ex).await? {
            permissions.push(Permission::parse(format!("{resource}:{action}"))?);
        }
        pending.extend(store.subject_roles(&key, ex).await?);
    }
    Ok(PermissionSet::new(permissions))
}
