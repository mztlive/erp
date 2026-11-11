//! 固定选品登记与真实责任事实装配。
use std::sync::Arc;

use erp_identity::entity::authorization::{FactKind, ReadAccess, RelationKey, RelationMatch};
use erp_identity::{
    AccessFactsPort, AccessMapping, AccessMode, AccessPolicy, AccessRegistration, AccessSubject,
    AuthorizationRegistry, AuthorizationService, DataAccess, Permission, Result, SharedRbacService,
};
use mongodb::Database;

use crate::repository::sales_selection::authorization::SelectionMapping;

pub(super) type SelectionAuthorization =
    (AuthorizationService, Arc<dyn AccessMapping>, Arc<dyn AccessMapping>);

const BOOKLET_ACTIONS: &[&str] = &[
    "list",
    "get",
    "create",
    "maintain",
    "prepare",
    "publish",
    "copy_link",
    "rotate_link",
    "close",
    "revoke",
    "void",
];

/// 绑定固定目录和同一组查询模型；创建前可注入拟写入的真实责任。
pub(super) fn service(
    db: Database,
    rbac: SharedRbacService,
    facts: Arc<dyn AccessFactsPort>,
    customer: Arc<dyn AccessMapping>,
) -> Result<SelectionAuthorization> {
    let booklet: Arc<dyn AccessMapping> =
        Arc::new(SelectionMapping::with_customer("sales_selection_booklet", customer.clone())?);
    let proposal: Arc<dyn AccessMapping> =
        Arc::new(SelectionMapping::with_customer("sales_selection_proposal", customer)?);
    let mut entries = Vec::new();
    for (resource, actions, mapping) in [
        ("sales_selection_booklet", BOOKLET_ACTIONS, &booklet),
        ("sales_selection_proposal", &["list", "get"][..], &proposal),
    ] {
        for action in actions {
            entries.push(registration(resource, action, mapping.clone(), facts.clone())?);
        }
    }
    Ok((AuthorizationService::new(db, rbac, AuthorizationRegistry::new(entries)?), booklet, proposal))
}

/// 方案责任取当前所属册，不增加来源菜单资格。
fn registration(
    resource: &str,
    action: &str,
    mapping: Arc<dyn AccessMapping>,
    facts: Arc<dyn AccessFactsPort>,
) -> Result<AccessRegistration> {
    let data = DataAccess {
        subject: AccessSubject::Direct,
        required_facts: [FactKind::Owner, FactKind::Organization].into(),
        policy: policy(resource, action)?,
    };
    let access = if action == "list" {
        // 客户多负责人事实以完整来源查询执行，不声明单值对象事实能力。
        AccessMode::Query { data, mapping }
    } else {
        AccessMode::QueryAndObject { data, mapping, facts }
    };
    Ok(AccessRegistration {
        permission: Permission::parse(format!("{resource}:{action}"))?,
        required_permissions: vec![],
        access,
    })
}

/// 列表客户范围与当前选品责任在同次授权中取交集。
fn policy(resource: &str, action: &str) -> Result<AccessPolicy> {
    let permission = Permission::parse(format!("{resource}:{action}"))?;
    let scope = AccessPolicy::Scope { permission, read: ReadAccess::default() };
    if action == "list" {
        return Ok(AccessPolicy::All(vec![scope, customer_list_policy()?]));
    }
    if resource == "sales_selection_booklet" && action != "get" && action != "create" {
        let get = Permission::parse("sales_selection_booklet:get")?;
        return Ok(AccessPolicy::All(vec![
            scope,
            AccessPolicy::Independent {
                permission: get.clone(),
                policy: Box::new(AccessPolicy::Scope { permission: get, read: ReadAccess::default() }),
            },
        ]));
    }
    Ok(scope)
}

/// 有目录资格仍须满足客户范围；普通列表只显示当前负责的有权客户。
fn customer_list_policy() -> Result<AccessPolicy> {
    Ok(AccessPolicy::Related {
        relation: RelationKey::parse("customer")?,
        resource: "customer".into(),
        mode: RelationMatch::AllNonEmpty,
        required_facts: [FactKind::Owner, FactKind::Organization].into(),
        policy: Box::new(AccessPolicy::All(vec![
            AccessPolicy::Scope {
                permission: Permission::parse("customer:list")?,
                read: ReadAccess::default(),
            },
            AccessPolicy::Any(vec![
                AccessPolicy::Permission(Permission::parse("customer_scope:detail")?),
                AccessPolicy::Owned,
            ]),
        ])),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_actions_preserve_a_separately_qualified_get_scope() {
        for action in
            BOOKLET_ACTIONS.iter().copied().filter(|action| !matches!(*action, "list" | "get" | "create"))
        {
            let AccessPolicy::All(policies) = policy("sales_selection_booklet", action).unwrap() else {
                panic!("册写动作须保留动作范围及独立读取前提")
            };
            assert!(matches!(&policies[0], AccessPolicy::Scope { permission, .. }
                if permission.action() == action));
            let AccessPolicy::Independent { permission, policy } = &policies[1] else {
                panic!("读取前提按独立入口取有资格角色")
            };
            assert_eq!(permission.to_string(), "sales_selection_booklet:get");
            assert!(matches!(policy.as_ref(), AccessPolicy::Scope { permission, read }
                if permission.action() == "get" && *read == ReadAccess::default()));
        }
    }

    #[test]
    fn get_and_create_do_not_acquire_an_additional_get_requirement() {
        for (resource, action) in [
            ("sales_selection_booklet", "get"),
            ("sales_selection_booklet", "create"),
            ("sales_selection_proposal", "get"),
        ] {
            assert!(matches!(policy(resource, action).unwrap(), AccessPolicy::Scope { permission, .. }
                if permission.resource() == resource && permission.action() == action));
        }
    }
}
