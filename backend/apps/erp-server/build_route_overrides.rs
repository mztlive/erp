//! HTTP 展示差异。此模块只供构建生成器使用，不参与请求鉴权。
use std::collections::HashSet;

use erp_permissions::{self as permissions, Permission, PermissionGroupDef, groups};

use crate::build_permissions::RouteHandler;
use crate::route_descriptions::DESCRIPTIONS;

/// 当前接口相对公共目录的差异；未覆盖字段继续使用公共默认值。
#[derive(Debug, Default)]
pub(crate) struct RoutePresentation {
    pub(crate) description: Option<&'static str>,
    pub(crate) group: Option<PermissionGroupDef>,
    pub(crate) catalog_permission: Option<&'static Permission>,
}

// 生成权限目录时，按实际 HTTP 方法及不含 `/admin` 的路径查找显式登记的展示差异；不改变鉴权权限。
pub(crate) fn presentation(method: &str, path: &str) -> RoutePresentation {
    RoutePresentation {
        description: DESCRIPTIONS.iter().find(|row| (row.0, row.1) == (method, path)).map(|row| row.2),
        group: GROUPS.iter().find(|row| (row.0, row.1) == (method, path)).map(|row| row.2),
        catalog_permission: CATALOG_PERMISSIONS
            .iter()
            .find(|row| (row.0, row.1) == (method, path))
            .map(|row| row.2),
    }
}

// 生成前核对展示差异引用的接口真实存在，且同一字段只登记一次；登记失配时拒绝生成。
pub(crate) fn validate(routes: &[RouteHandler]) -> Result<(), String> {
    validate_keys(DESCRIPTIONS.iter().map(|row| (row.0, row.1)), routes)?;
    validate_keys(GROUPS.iter().map(|row| (row.0, row.1)), routes)?;
    validate_keys(CATALOG_PERMISSIONS.iter().map(|row| (row.0, row.1)), routes)
}

// 逐类校验展示字段的路由键，允许同一接口分别覆盖说明和分组，但同类覆盖不能重复。
fn validate_keys<'a>(
    keys: impl IntoIterator<Item = (&'a str, &'a str)>,
    routes: &[RouteHandler],
) -> Result<(), String> {
    let mut seen = HashSet::new();
    for (method, path) in keys {
        if !seen.insert((method, path)) {
            return Err(format!("接口展示字段重复登记：{method} {path}"));
        }
        if !routes
            .iter()
            .any(|route| route.method == method && route.path == path && route.permission.is_some())
        {
            return Err(format!("接口展示引用了不存在的受保护路由：{method} {path}"));
        }
    }
    Ok(())
}

/// 仅登记与公共目录不同的展示分组。
const GROUPS: &[(&str, &str, PermissionGroupDef)] = &[
    ("GET", "/customer-quality/history/directory", groups::COST_ENTRY),
    ("GET", "/customer-quality/current", groups::CUSTOMER_QUALITY),
    ("GET", "/customer-quality/history", groups::CUSTOMER_QUALITY),
    ("POST", "/customer-quality/current/exports", groups::CUSTOMER_QUALITY),
    ("POST", "/customer-quality/history/exports", groups::CUSTOMER_QUALITY),
    ("GET", "/warehouse-directory", groups::WAREHOUSE),
    ("GET", "/warehouse-directory/selected", groups::WAREHOUSE),
    ("GET", "/sales-orders/{id}/inquiry-fees", groups::COST_ENTRY_2),
    ("POST", "/purchase-payment-requests/{id}/reveal-account", groups::PURCHASE_PAYMENT_REQUEST),
    ("GET", "/actual-profit-loss/history-directory", groups::COST_ENTRY),
];

/// 保留既有目录展示差异；实际请求仍由路由中的权限常量鉴权。
const CATALOG_PERMISSIONS: &[(&str, &str, &Permission)] =
    &[("POST", "/supplier-api-connections/{id}/commands", &permissions::supplier_api::CONNECTION_COMMAND)];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_routes_have_no_overrides_and_special_catalog_does_not_change_the_guard() {
        let ordinary = presentation("GET", "/sales-inquiries");
        assert!(ordinary.description.is_none());
        assert!(ordinary.group.is_none());
        assert!(ordinary.catalog_permission.is_none());
        let command = presentation("POST", "/supplier-api-connections/{id}/commands");
        assert_eq!(command.catalog_permission.unwrap(), &permissions::supplier_api::CONNECTION_COMMAND);
        assert_eq!(permissions::supplier_api::CONNECTION_DETAIL.action(), "detail");
    }

    #[test]
    fn duplicate_and_orphan_overrides_are_rejected() {
        let route = RouteHandler {
            method: "GET".into(),
            path: "/present".into(),
            handler: "h::read".into(),
            permission: Some("sales_inquiry::QUERY_SALES_LIST".into()),
            guards: vec![],
        };
        let routes = [route];
        assert!(
            validate_keys([("GET", "/present"), ("GET", "/present")], &routes).unwrap_err().contains("重复")
        );
        assert!(validate_keys([("POST", "/missing")], &routes).unwrap_err().contains("不存在"));
        validate_keys([("GET", "/present")], &routes).unwrap();
    }
}
