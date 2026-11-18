//! 公共权限值、内置常量与目录。账号授权和业务事实判断由 identity 执行。
mod catalog_types;
mod permission;

pub use catalog_types::{PermissionDef, PermissionGroupDef, PermissionModule};
pub use permission::{ParsePermissionError, Permission};

pub mod approval_admin;
pub mod groups;
pub mod internal;
pub mod resources;

macro_rules! catalog_modules {
    ($($module:ident),+ $(,)?) => {
        $(pub mod $module;)+

        /// 内置权限的公共目录；HTTP 应用另行登记接口引用及说明。
        pub const MODULES: &[PermissionModule] = &[
            $(PermissionModule { name: stringify!($module), entries: $module::ENTRIES },)+
        ];
    };
}

catalog_modules!(
    access_control,
    admin,
    approval_instance,
    approval_process,
    bulk_job,
    catalog,
    contract,
    cost,
    customer,
    customer_quality,
    demo_master_data,
    document_registry,
    file_asset,
    financial_files,
    fulfillment,
    integration_ops,
    inventory,
    legacy_import,
    organization,
    party,
    payable,
    policy_permission,
    purchase_order,
    purchase_payment_request,
    receivable,
    returns,
    sales_inquiry,
    sales_order,
    sales_review,
    sales_selection,
    source_registry,
    supplier,
    supplier_api,
    supplier_deposit,
    supplier_fulfillment,
    supplier_offering,
    supplier_portal,
    supplier_settlement,
    warehouse,
    work_item,
);

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn public_catalog_has_unique_permissions_and_consistent_groups() {
        let mut permissions = HashMap::new();
        let mut groups = HashMap::new();
        for module in MODULES {
            for (name, definition) in module.entries {
                let permission = definition.permission;
                assert_eq!(Permission::parse(permission.to_string()).unwrap(), *permission);
                assert!(permissions.insert(permission, (module.name, name)).is_none(), "{permission}");
                assert!(!definition.description.is_empty(), "{permission}");
                if let Some(previous) = groups.insert(definition.group.name, definition.group.description) {
                    assert_eq!(previous, definition.group.description);
                }
            }
        }
    }
}
