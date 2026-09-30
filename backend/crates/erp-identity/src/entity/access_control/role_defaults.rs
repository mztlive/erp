//! 普通角色新获业务动作的默认范围计划。
use crate::Result;
use crate::entity::rbac::{Permission, PermissionSet};

/// 比较真实动作覆盖，权限通配写法变化不重新初始化已持有动作。
/// # 参数
/// 业务、已接线动作以及前后权限集合。
/// # 返回
/// 新增获得资格的明确动作。
/// # 错误
/// 非法资源动作代码返回校验错误。
pub(crate) fn new_default_actions(
    resource: &str,
    actions: &[&str],
    previous: &PermissionSet,
    next: &PermissionSet,
) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for action in actions {
        let permission = Permission::parse(format!("{resource}:{action}"))?;
        if next.covers_one(&permission) && !previous.covers_one(&permission) {
            result.push((*action).into());
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_only_cover_new_actions_and_do_not_restore_existing_missing_rules() {
        let previous = PermissionSet::new(vec![Permission::parse("sales_order:list").unwrap()]);
        let next = PermissionSet::new(vec![Permission::parse("sales_order:*").unwrap()]);
        assert_eq!(
            new_default_actions("sales_order", &["list", "detail", "update"], &previous, &next).unwrap(),
            vec!["detail", "update"]
        );
        assert!(new_default_actions("sales_order", &["list"], &next, &previous).unwrap().is_empty());
    }
}
