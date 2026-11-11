//! 固定销售及来源消费入口；请求不能扩展登记目录。
use std::collections::BTreeSet;
use std::sync::Arc;

use erp_identity::entity::authorization::{FactKind, ReadAccess, RelationKey, RelationMatch};
use erp_identity::ports::{AccessFactsPort, AccessMapping};
use erp_identity::{
    AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AuthorizationRegistry, AuthorizationService,
    DataAccess, Permission, SharedRbacService,
};
use erp_workflow::service::approval::policy::{ALL_DOCUMENT_TYPES, DocumentApprovalPolicy, policy_of};
use mongodb::Database;

use crate::sales_center::repository::authorization::{SalesFacts, SalesMapping};
use crate::{Error, Result};

/// 仅固定代码调用的入口和同角色权限组合可以登记。
fn known(resource: &str, action: &str, source: bool) -> bool {
    if source {
        return match resource {
            "work_item" => matches!(action, "list" | "detail" | "manage"),
            "approval_instance" => matches!(
                action,
                "read" | "decide" | "resume" | "cancel" | "cancel_blocked" | "upgrade_binding"
            ),
            "customer_receipt" | "receivable_account" | "sales_invoice_request" | "invoice" => {
                matches!(
                    action,
                    "list"
                        | "detail"
                        | "create"
                        | "update"
                        | "submit"
                        | "register"
                        | "download"
                        | "post"
                        | "cancel"
                )
            },
            "purchase_invoice_allocation" | "cost_allocation" => action == "list",
            "cost_entry" => matches!(action, "list" | "detail"),
            "purchase_order" => action == "create",
            _ => false,
        };
    }
    match resource {
        "sales_order" => matches!(
            action,
            "list" | "detail" | "create" | "update" | "delete" | "submit" | "cancel_approval"
        ),
        "cost_entry" => matches!(action, "list" | "detail"),
        "purchase_order" => action == "create",
        "customer_receipt" | "receivable_account" | "sales_invoice_request" | "invoice" => {
            matches!(action, "list" | "detail")
        },
        "purchase_invoice_allocation" | "cost_allocation" => action == "list",
        _ => false,
    }
}
pub(super) fn service(
    db: &Database,
    rbac: &SharedRbacService,
    resource: &str,
    action: &str,
    permissions: &[Permission],
    source: bool,
) -> Result<(AuthorizationService, Arc<dyn AccessMapping>, AccessSubject)> {
    service_facts(db, rbac, resource, action, permissions, source, None)
}
pub(super) fn service_facts(
    db: &Database,
    rbac: &SharedRbacService,
    resource: &str,
    action: &str,
    permissions: &[Permission],
    source: bool,
    override_facts: Option<Arc<dyn AccessFactsPort>>,
) -> Result<(AuthorizationService, Arc<dyn AccessMapping>, AccessSubject)> {
    let permission = Permission::parse(format!("{resource}:{action}"))?;
    let mapping: Arc<dyn AccessMapping> = Arc::new(SalesMapping::new(db.clone(), resource, "sales_orders")?);
    let facts = override_facts.unwrap_or_else(|| {
        Arc::new(SalesFacts(db.clone(), source || (resource == "purchase_order" && action == "create")))
    });
    let (registration, subject) = registration(permission, permissions, source, mapping.clone(), facts)?;
    let registry = AuthorizationRegistry::new(vec![registration])?;
    Ok((AuthorizationService::new(db.clone(), rbac.clone(), registry), mapping, subject))
}

/// 附加功能资格只登记为同角色必需权限，数据政策始终沿真实销售责任。
fn registration(
    permission: Permission,
    permissions: &[Permission],
    source: bool,
    mapping: Arc<dyn AccessMapping>,
    facts: Arc<dyn AccessFactsPort>,
) -> Result<(AccessRegistration, AccessSubject)> {
    validate_entry(permission.resource(), permission.action(), permissions, source)?;
    let subject = if source {
        AccessSubject::Source(RelationKey::parse("sales_source")?)
    } else {
        AccessSubject::Direct
    };
    let policy = policy(&permission, source)?;
    let registration = AccessRegistration {
        permission,
        required_permissions: permissions.to_vec(),
        access: AccessMode::QueryAndObject {
            data: DataAccess {
                subject: subject.clone(),
                required_facts: [FactKind::Owner, FactKind::Organization].into(),
                policy,
            },
            mapping,
            facts,
        },
    };
    Ok((registration, subject))
}

/// 未在服务端列出的入口和同角色组合不产生登记。
fn validate_entry(resource: &str, action: &str, permissions: &[Permission], source: bool) -> Result<()> {
    if !known(resource, action, source) {
        return Err(Error::ValidationError("尚未登记该销售事实消费入口".into()));
    }
    if source && resource == "approval_instance" && action != "read" && permissions.is_empty() {
        return Err(Error::ValidationError("审批来源入口必须登记完整必需权限".into()));
    }
    if source && !permissions.is_empty() {
        if approval_permissions(resource, action, permissions)? {
            return Ok(());
        }
        return Err(Error::ValidationError("尚未登记该审批来源完整权限组合".into()));
    }
    if !permissions.is_empty()
        && !(resource == "sales_order"
            && ((action == "list" && permissions == [Permission::parse("cost_entry:list")?])
                || (action == "detail" && permissions == [Permission::parse("cost_entry:create")?])
                || (action == "create" && permissions == [Permission::parse("sales_order:submit")?])
                || (action == "submit" && permissions == [Permission::parse("sales_order:create")?])))
    {
        return Err(Error::ValidationError("尚未登记该同角色销售权限组合".into()));
    }
    Ok(())
}
/// 审批所需功能权限只取服务端穷尽政策目录，客户端不能指定新组合。
pub(crate) fn approval_permissions(resource: &str, action: &str, permissions: &[Permission]) -> Result<bool> {
    if resource != "approval_instance" {
        return Ok(false);
    }
    let entry = Permission::parse(format!("{resource}:{action}"))?;
    let read = Permission::parse("approval_instance:read")?;
    let provided = permission_codes(permissions.iter().chain(std::iter::once(&entry)));
    if action == "decide" && provided == permission_codes([&entry, &read].into_iter()) {
        return Ok(true);
    }
    for document_type in ALL_DOCUMENT_TYPES {
        let DocumentApprovalPolicy::ProcessRequired(policy) = policy_of(document_type)? else { continue };
        let runtime = Permission::parse(policy.runtime_admin_permission)?;
        if provided == permission_codes([&entry, &read, &runtime].into_iter()) {
            return Ok(true);
        }
        if action == "upgrade_binding" {
            let definition = Permission::parse(policy.definition_admin_permission)?;
            if provided == permission_codes([&entry, &read, &definition].into_iter()) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
/// 同角色声明按权限集合匹配，忽略重复项及调用顺序。
fn permission_codes<'a>(permissions: impl Iterator<Item = &'a Permission>) -> BTreeSet<String> {
    permissions.map(|permission| format!("{}:{}", permission.resource(), permission.action())).collect()
}
/// 把固定场景声明交给 identity；这里不读取角色或解释条款。
fn policy(permission: &Permission, source: bool) -> Result<AccessPolicy> {
    let resource = permission.resource();
    let action = permission.action();
    let policy = if source || (resource == "purchase_order" && action == "create") {
        AccessPolicy::Related {
            relation: RelationKey::parse("sales_source")?,
            resource: "sales_order".into(),
            mode: RelationMatch::AllNonEmpty,
            required_facts: [FactKind::Owner, FactKind::Organization].into(),
            policy: Box::new(AccessPolicy::Scope {
                permission: Permission::parse("sales_order:detail")?,
                read: ReadAccess { history: false, records: true },
            }),
        }
    } else {
        let read = ReadAccess {
            history: resource == "sales_order" && matches!(action, "list" | "detail"),
            records: resource == "sales_order" && matches!(action, "list" | "detail"),
        };
        AccessPolicy::Scope { permission: permission.clone(), read }
    };
    Ok(policy)
}

#[cfg(test)]
mod tests {
    use std::slice;

    use erp_identity::Error as IdentityError;

    use super::*;
    use crate::sales_center::repository::authorization::fixtures;

    fn registered(
        resource: &str,
        action: &str,
        permissions: &[Permission],
        source: bool,
    ) -> (AccessRegistration, AccessSubject) {
        registration(
            Permission::parse(format!("{resource}:{action}")).unwrap(),
            permissions,
            source,
            fixtures::mapping(resource),
            fixtures::facts(),
        )
        .unwrap()
    }

    #[test]
    fn cost_features_register_same_role_qualification_with_only_the_sales_scope() {
        for (action, extra) in [("list", "cost_entry:list"), ("detail", "cost_entry:create")] {
            let extra = Permission::parse(extra).unwrap();
            let (entry, subject) = registered("sales_order", action, slice::from_ref(&extra), false);
            assert_eq!(subject, AccessSubject::Direct);
            assert_eq!(entry.required_permissions, [extra]);
            let AccessMode::QueryAndObject { data, .. } = &entry.access else {
                panic!("销售行必须同时登记查询与完整对象能力");
            };
            let AccessPolicy::Scope { permission, read } = &data.policy else {
                panic!("成本功能资格不能产生独立数据范围");
            };
            assert_eq!(permission, &Permission::parse(format!("sales_order:{action}")).unwrap());
            assert!(read.history);
            assert!(read.records);
            AuthorizationRegistry::new(vec![entry]).unwrap();
        }
    }

    #[test]
    fn registry_rejects_cost_creation_as_an_independent_scope() {
        let create = Permission::parse("cost_entry:create").unwrap();
        let (mut entry, _) = registered("sales_order", "detail", slice::from_ref(&create), false);
        let AccessMode::QueryAndObject { data, .. } = &mut entry.access else {
            panic!("销售行必须同时登记查询与完整对象能力");
        };
        data.policy = AccessPolicy::Related {
            relation: RelationKey::parse("invalid_cost_scope").unwrap(),
            resource: "cost_entry".into(),
            mode: RelationMatch::AllNonEmpty,
            required_facts: [FactKind::Owner, FactKind::Organization].into(),
            policy: Box::new(AccessPolicy::Scope { permission: create, read: ReadAccess::default() }),
        };
        assert!(matches!(
            AuthorizationRegistry::new(vec![entry]),
            Err(IdentityError::ValidationError(message)) if message == "cost_entry:create 尚未接入 DataScope v2"
        ));
    }

    #[test]
    fn cost_source_queries_keep_actual_sales_authorization_without_extra_sales_feature_permissions() {
        for action in ["list", "detail"] {
            let (entry, subject) = registered("cost_entry", action, &[], true);
            let relation = RelationKey::parse("sales_source").unwrap();
            assert_eq!(subject, AccessSubject::Source(relation.clone()));
            assert!(entry.required_permissions.is_empty());
            let AccessMode::QueryAndObject { data, .. } = &entry.access else {
                panic!("来源查询必须保留完整对象能力");
            };
            let AccessPolicy::Related { relation: actual, resource, mode, policy, .. } = &data.policy else {
                panic!("成本查询必须继承真实销售来源");
            };
            assert_eq!(actual, &relation);
            assert_eq!(resource, "sales_order");
            assert_eq!(*mode, RelationMatch::AllNonEmpty);
            assert!(matches!(policy.as_ref(), AccessPolicy::Scope { permission, read }
                if permission == &Permission::parse("sales_order:detail").unwrap() && !read.history && read.records));
            AuthorizationRegistry::new(vec![entry]).unwrap();
        }
    }

    #[test]
    fn cost_qualification_combinations_remain_fixed() {
        let create = Permission::parse("cost_entry:create").unwrap();
        for action in ["list", "update", "submit"] {
            assert!(matches!(
                validate_entry("sales_order", action, slice::from_ref(&create), false),
                Err(Error::ValidationError(message)) if message == "尚未登记该同角色销售权限组合"
            ));
        }
        assert!(matches!(
            validate_entry(
                "sales_order",
                "detail",
                &[create, Permission::parse("cost_entry:update").unwrap()],
                false,
            ),
            Err(Error::ValidationError(message)) if message == "尚未登记该同角色销售权限组合"
        ));
    }
    #[test]
    fn source_consumers_are_explicit_and_unknown_actions_fail_closed() {
        for (resource, action) in [
            ("work_item", "detail"),
            ("approval_instance", "decide"),
            ("invoice", "detail"),
            ("cost_allocation", "list"),
        ] {
            assert!(known(resource, action, true));
        }
        assert!(!known("sales_order", "transfer", false));
        assert!(!known("invoice", "delete", true));
        assert!(!known("unknown", "list", true));
    }
    #[test]
    fn approval_sources_accept_only_complete_server_policy_permission_sets() {
        let read = Permission::parse("approval_instance:read").unwrap();
        let resume = Permission::parse("approval_instance:resume").unwrap();
        let runtime = Permission::parse("sales_order:approval_runtime_admin").unwrap();
        let definition = Permission::parse("sales_order:approval_definition_admin").unwrap();
        assert!(
            validate_entry("approval_instance", "resume", &[runtime.clone(), read.clone(), resume], true)
                .is_ok()
        );
        assert!(
            validate_entry("approval_instance", "upgrade_binding", &[read.clone(), definition], true).is_ok()
        );
        assert!(validate_entry("approval_instance", "resume", slice::from_ref(&runtime), true).is_err());
        assert!(validate_entry("invoice", "detail", &[runtime], true).is_err());
        assert!(
            validate_entry(
                "approval_instance",
                "resume",
                &[read, Permission::parse("sales_order:update").unwrap()],
                true
            )
            .is_err()
        );
    }
    #[test]
    fn management_and_decision_sources_cannot_use_empty_permission_registration() {
        for action in ["resume", "cancel", "cancel_blocked", "upgrade_binding", "decide"] {
            assert!(validate_entry("approval_instance", action, &[], true).is_err());
        }
        assert!(validate_entry("approval_instance", "read", &[], true).is_ok());
        assert!(
            validate_entry(
                "approval_instance",
                "decide",
                &[Permission::parse("approval_instance:read").unwrap()],
                true
            )
            .is_ok()
        );
    }
}
