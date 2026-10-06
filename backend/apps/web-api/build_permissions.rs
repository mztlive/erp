//! 权限构建目录的值类型及非路由政策权限。
use std::collections::HashMap;
#[path = "../../crates/erp-identity/src/entity/policy_permission.rs"]
mod policy_permission;

#[derive(Debug, Clone)]
pub(crate) struct PermissionMeta {
    pub(crate) group: String,
    pub(crate) group_desc: String,
    pub(crate) desc: String,
    pub(crate) resource: Option<String>,
    pub(crate) action: Option<String>,
}

/// 权限扫描覆盖的业务域模块：既有业务域、审批定义管理，以及演示主数据。
pub(crate) const DOMAIN_MODULES: &[&str] = &[
    "organization",
    "source_registry",
    "document_registry",
    "work_item",
    "approval_instance",
    "approval_process",
    "bulk_job",
    "file_asset",
    "financial_files",
    "access_control",
    "party",
    "customer",
    "customer_quality",
    "supplier",
    "catalog",
    "warehouse",
    "contract",
    "sales_order",
    "sales_review",
    "sales_selection",
    "purchase_order",
    "fulfillment",
    "inventory",
    "receivable",
    "payable",
    "cost",
    "returns",
    "legacy_import",
    "supplier_offering",
    "supplier_portal",
    "supplier_api",
    "supplier_fulfillment",
    "supplier_settlement",
    "integration_ops",
    // 单文件不够，扫描 `handler/demo_master_data/mod.rs`。
    "demo_master_data",
];

#[derive(Debug, Clone)]
pub(crate) struct RouteHandler {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) handler: String,
}

/// 只登记管理端路由，补全门户专项路由的嵌套前缀。
/// # 参数
/// 路由模块名称和解析出的实际方法、路径及处理器引用。
/// # 返回
/// 内部权限目录使用的路由，不包含供应商外部登录和业务入口。
/// # 错误
/// 纯路由投影，不执行外部操作。
pub(crate) fn management_routes(module: &str, parsed: Vec<RouteHandler>) -> Vec<RouteHandler> {
    if module != "supplier_portal" {
        return parsed;
    }
    parsed
        .into_iter()
        .filter_map(|mut route| {
            if !route.handler.starts_with("supplier_portal::") {
                return None;
            }
            route.path = format!("/supplier-portal{}", route.path);
            Some(route)
        })
        .collect()
}

#[derive(Debug, Default)]
pub(crate) struct PermissionGroup {
    pub(crate) desc: String,
    pub(crate) permissions: Vec<PermissionItem>,
}

#[derive(Debug, Clone)]
pub(crate) struct PermissionItem {
    pub(crate) module: String,
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) description: String,
    pub(crate) resource: String,
    pub(crate) action: String,
}

/// 将领域政策资格加入同一权限目录；该资格没有独立 HTTP 路由。
pub(crate) fn append_policy_permissions(
    groups: &mut HashMap<String, PermissionGroup>,
    order: &mut Vec<String>,
) {
    for &(group, description, key, label) in policy_permission::POLICY_PERMISSIONS {
        let Some((resource, action)) = key.split_once(':') else {
            panic!("invalid domain policy permission: {key}");
        };
        let entry = groups.entry(group.into()).or_insert_with(|| {
            order.push(group.into());
            PermissionGroup { desc: description.into(), permissions: Vec::new() }
        });
        entry.permissions.push(PermissionItem {
            module: "admin".into(),
            method: "POLICY".into(),
            path: String::new(),
            description: label.into(),
            resource: resource.into(),
            action: action.into(),
        });
    }
}
