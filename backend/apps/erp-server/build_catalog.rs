//! 显式权限目录与 HTTP 路由的构建期装配和校验。
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use erp_permissions::{MODULES, PermissionDef, PermissionModule};

use crate::build_permissions::{
    DOMAIN_MODULES, PermissionGroup, PermissionItem, RouteHandler, append_policy_permissions,
    management_routes,
};
use crate::build_route_overrides;

#[path = "build_routes.rs"]
mod route_parser;

/// 收集管理端注册，保留既有模块与路由顺序。
pub(crate) fn collect_routes(manifest: &Path) -> Result<Vec<RouteHandler>, String> {
    let root = manifest.join("src/http/routes");
    let mut routes = Vec::new();
    for module in std::iter::once("admin").chain(DOMAIN_MODULES.iter().copied()) {
        let path = root.join(format!("{module}.rs"));
        // organization 的入口合并在 access_control；保留原有模块枚举。
        if !path.exists() {
            continue;
        }
        println!("cargo:rerun-if-changed={}", path.display());
        let source = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let parsed =
            route_parser::parse_routes(&source).map_err(|error| format!("{}: {error}", path.display()))?;
        routes.extend(management_routes(module, parsed));
    }
    reject_unscanned_modules(&root)?;
    build_route_overrides::validate(&routes)?;
    Ok(routes)
}

/// 新管理端路由模块必须进入生成清单，避免带权限的接口被静默遗漏。
fn reject_unscanned_modules(root: &Path) -> Result<(), String> {
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
            continue;
        }
        let module = path.file_stem().and_then(|name| name.to_str()).ok_or("路由文件名必须为 UTF-8")?;
        if module == "admin" || DOMAIN_MODULES.contains(&module) {
            continue;
        }
        let source = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let routes = route_parser::parse_routes(&source)?;
        if routes.iter().any(|route| route.permission.is_some()) {
            return Err(format!("受保护路由模块 {module} 未登记到权限生成清单"));
        }
    }
    Ok(())
}

/// 校验并索引实际编译的静态定义；同权限的不同展示引用可以共存。
pub(crate) fn registry(modules: &[PermissionModule]) -> Result<HashMap<String, PermissionDef>, String> {
    let mut definitions = HashMap::new();
    let mut module_names = HashSet::new();
    let mut groups = HashMap::new();
    for module in modules {
        if !module_names.insert(module.name) {
            return Err(format!("重复的权限模块 {}", module.name));
        }
        for (name, definition) in module.entries {
            if definition.description.is_empty()
                || definition.group.name.is_empty()
                || definition.group.description.is_empty()
            {
                return Err(format!("权限 {name} 缺少展示信息"));
            }
            if let Some(previous) = groups.insert(definition.group.name, definition.group.description)
                && previous != definition.group.description
            {
                return Err(format!("权限分组 {} 的说明不一致", definition.group.name));
            }
            let symbol = format!("{}::{name}", module.name);
            if definitions.insert(symbol.clone(), *definition).is_some() {
                return Err(format!("重复的权限定义 {symbol}"));
            }
        }
    }
    Ok(definitions)
}

/// 从同一显式定义生成保持既有顺序和展示内容的权限分组。
pub(crate) fn groups(
    prefix: &str,
    routes: &[RouteHandler],
) -> Result<Vec<(String, PermissionGroup)>, String> {
    let definitions = registry(MODULES)?;
    project_groups(prefix, routes, &definitions)
}

fn project_groups(
    prefix: &str,
    routes: &[RouteHandler],
    definitions: &HashMap<String, PermissionDef>,
) -> Result<Vec<(String, PermissionGroup)>, String> {
    let mut groups = HashMap::<String, PermissionGroup>::new();
    let mut order = Vec::new();
    for route in routes {
        if exempt(route) {
            continue;
        }
        let definition = resolve(route, definitions)?;
        let name = definition.group.name;
        let entry = groups.entry(name.into()).or_insert_with(|| {
            order.push(name.into());
            PermissionGroup { desc: definition.group.description.into(), permissions: Vec::new() }
        });
        entry.permissions.push(PermissionItem {
            module: "admin".into(),
            method: route.method.clone(),
            path: format!("{}/{}", prefix.trim_end_matches('/'), route.path.trim_start_matches('/')),
            description: definition.description.into(),
            resource: definition.permission.resource().into(),
            action: definition.permission.action().into(),
        });
    }
    append_policy_permissions(&mut groups, &mut order);
    Ok(order
        .into_iter()
        .map(|name| {
            let group = groups.remove(&name).expect("分组顺序只登记已插入的分组");
            (name, group)
        })
        .collect())
}

fn exempt(route: &RouteHandler) -> bool {
    route.permission.is_none()
        && (route.path.starts_with("/public/selection/")
            || route.path.starts_with("/public/sales-selections/")
            || matches!(route.path.as_str(), "/settlement-parties" | "/settlement-parties/selected"))
}

fn resolve(
    route: &RouteHandler,
    definitions: &HashMap<String, PermissionDef>,
) -> Result<PermissionDef, String> {
    for symbol in &route.guards {
        if !definitions.contains_key(symbol) {
            return Err(format!("{} {} 引用了未登记的权限 {symbol}", route.method, route.path));
        }
    }
    let symbol = route
        .permission
        .as_ref()
        .ok_or_else(|| format!("{} {} 缺少显式权限绑定", route.method, route.path))?;
    let required = definitions
        .get(symbol)
        .ok_or_else(|| format!("{} {} 引用了未登记的权限 {symbol}", route.method, route.path))?;
    let presentation = build_route_overrides::presentation(&route.method, &route.path);
    let mut catalog = if let Some(permission) = presentation.catalog_permission {
        *definitions
            .values()
            .find(|definition| definition.permission == permission)
            .ok_or_else(|| format!("接口展示引用了未登记权限 {permission}"))?
    } else {
        *required
    };
    if let Some(group) = presentation.group {
        catalog.group = group;
    }
    if let Some(description) = presentation.description {
        catalog.description = description;
    }
    Ok(catalog)
}

/// 输出可对比的路由权限事实；不包含源码符号的迁移噪声。
pub(crate) fn route_contract(routes: &[RouteHandler]) -> Result<String, String> {
    let definitions = registry(MODULES)?;
    let mut rows = Vec::new();
    for route in routes {
        if exempt(route) {
            continue;
        }
        let definition = resolve(route, &definitions)?;
        rows.push(serde_json::json!({
            "method": route.method, "path": route.path, "handler": route.handler,
            "code": definition.permission, "group": definition.group.name,
            "guards": route.guards.iter().map(|symbol| definitions.get(symbol).expect("全部权限层已校验").permission).collect::<Vec<_>>(),
            "group_desc": definition.group.description, "description": definition.description,
        }));
    }
    serde_json::to_string_pretty(&rows).map_err(|error| error.to_string())
}

/// 为库测试生成对真实 Rust 常量的引用，防止显式索引名称与值配错。
pub(crate) fn compiled_references() -> Result<String, String> {
    let definitions = registry(MODULES)?;
    let mut names = definitions.keys().collect::<Vec<_>>();
    names.sort();
    let mut output = String::from("const COMPILED_PERMISSIONS: &[(&str, &Permission)] = &[\n");
    for name in names {
        output.push_str(&format!("({name:?}, &permissions::{name}),\n"));
    }
    output.push_str("];\n");
    Ok(output)
}

#[cfg(test)]
mod tests {
    use erp_permissions::{self as permissions, Permission, PermissionGroupDef, groups, purchase_order};

    use super::*;

    const SUBMIT: PermissionDef = PermissionDef {
        permission: &purchase_order::SUBMIT,
        group: groups::PURCHASE_ORDER,
        description: "提交",
    };

    include!(concat!(env!("OUT_DIR"), "/permission_references.rs"));

    #[test]
    fn public_index_resolves_to_the_compiled_permission_constants() {
        let actual = registry(MODULES).unwrap();
        for (symbol, permission) in COMPILED_PERMISSIONS {
            assert_eq!(actual[*symbol].permission, *permission, "{symbol}");
        }
    }

    #[test]
    fn registered_catalog_preserves_all_frozen_route_bindings() {
        let manifest = std::env::var("ERP_PERMISSION_MANIFEST_DIR")
            .unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").into());
        let routes = collect_routes(Path::new(&manifest)).unwrap();
        let actual: serde_json::Value = serde_json::from_str(&route_contract(&routes).unwrap()).unwrap();
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("testdata/permission-routes.json")).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn duplicate_symbols_and_inconsistent_groups_fail() {
        const DUPLICATE: PermissionModule =
            PermissionModule { name: "purchase", entries: &[("SUBMIT", SUBMIT), ("SUBMIT", SUBMIT)] };
        assert!(registry(&[DUPLICATE]).unwrap_err().contains("重复的权限定义"));
        const EMPTY: PermissionModule = PermissionModule { name: "purchase", entries: &[] };
        assert!(registry(&[EMPTY, EMPTY]).unwrap_err().contains("重复的权限模块"));
        const WRONG_GROUP: PermissionDef = PermissionDef {
            group: PermissionGroupDef { name: SUBMIT.group.name, description: "不同说明" },
            ..SUBMIT
        };
        const CONFLICT: PermissionModule =
            PermissionModule { name: "purchase", entries: &[("SUBMIT", SUBMIT), ("OTHER", WRONG_GROUP)] };
        assert!(registry(&[CONFLICT]).unwrap_err().contains("说明不一致"));
    }

    #[test]
    fn missing_bindings_and_unknown_definitions_fail() {
        let mut route = RouteHandler {
            method: "POST".into(),
            path: "/purchase-orders/{id}/submit".into(),
            handler: "handler::submit".into(),
            permission: None,
            guards: vec![],
        };
        let definitions = registry(MODULES).unwrap();
        assert!(
            project_groups("/admin", &[route.clone()], &definitions)
                .unwrap_err()
                .contains("缺少显式权限绑定")
        );
        route.permission = Some("purchase_order::MISSING".into());
        assert!(project_groups("/admin", &[route], &definitions).unwrap_err().contains("未登记"));
    }

    #[test]
    fn reuse_keeps_endpoint_descriptions_and_non_http_policy_permissions() {
        let routes = route_parser::parse_routes(
            r#"fn routes() { Router::new()
            .route("/a", with_permission(post(h::submit), rbac, permissions::purchase_order::SUBMIT))
            .route("/b", with_permission(post(h::submit), rbac, permissions::purchase_order::SUBMIT)); }"#,
        )
        .unwrap();
        let groups = groups("/admin", &routes).unwrap();
        assert_eq!(groups[0].1.permissions.len(), 2);
        assert_eq!(groups[0].1.desc, groups::PURCHASE_ORDER.description);
        assert_eq!(groups[0].1.permissions[0].module, "admin");
        assert_eq!(groups[0].1.permissions[0].path, "/admin/a");
        assert_eq!(
            groups[0].1.permissions[1].description,
            registry(MODULES).unwrap()["purchase_order::SUBMIT"].description
        );
        assert!(groups.iter().flat_map(|(_, group)| &group.permissions).any(|item| item.method == "POLICY"
            && item.resource == "finance_ledger"
            && item.action == "read"));
    }
}
