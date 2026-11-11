//! 固定选品登记与真实责任事实装配。
use std::sync::Arc;

use erp_identity::entity::authorization::{FactKind, ReadAccess};
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
) -> Result<SelectionAuthorization> {
    let booklet: Arc<dyn AccessMapping> = Arc::new(SelectionMapping::new("sales_selection_booklet")?);
    let proposal: Arc<dyn AccessMapping> = Arc::new(SelectionMapping::new("sales_selection_proposal")?);
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
    Ok(AccessRegistration {
        permission: Permission::parse(format!("{resource}:{action}"))?,
        required_permissions: vec![],
        access: AccessMode::QueryAndObject {
            data: DataAccess {
                subject: AccessSubject::Direct,
                required_facts: [FactKind::Owner, FactKind::Organization].into(),
                policy: AccessPolicy::Scope {
                    permission: Permission::parse(format!("{resource}:{action}"))?,
                    read: ReadAccess::default(),
                },
            },
            mapping,
            facts,
        },
    })
}
