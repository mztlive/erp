//! 显式权限目录与 HTTP 路由的构建期装配和校验。
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use crate::build_permissions::{
    DOMAIN_MODULES, PermissionGroup, PermissionItem, RouteHandler, append_policy_permissions,
    management_routes,
};
use crate::permissions::{MODULES, PermissionBinding, PermissionDef, PermissionModule};

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
            validate_code(definition.code)?;
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

fn validate_code(code: &str) -> Result<(), String> {
    let (resource, action) = code.split_once(':').ok_or_else(|| format!("非法权限代码 {code}"))?;
    for (part, slash) in [(resource, true), (action, false)] {
        let valid = !part.is_empty()
            && part.len() <= 128
            && (part == "*"
                || part.split('/').all(|segment| {
                    !segment.is_empty()
                        && segment.chars().all(|ch| {
                            ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '-')
                        })
                }))
            && (slash || !part.contains('/'));
        if !valid {
            return Err(format!("非法权限代码 {code}"));
        }
    }
    Ok(())
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
        let definition = resolve(route, definitions)?.catalog;
        let name = definition.group.name;
        let entry = groups.entry(name.into()).or_insert_with(|| {
            order.push(name.into());
            PermissionGroup { desc: definition.group.description.into(), permissions: Vec::new() }
        });
        let (resource, action) = definition.code.split_once(':').ok_or("已校验权限代码缺少分隔符")?;
        entry.permissions.push(PermissionItem {
            module: "admin".into(),
            method: route.method.clone(),
            path: format!("{}/{}", prefix.trim_end_matches('/'), route.path.trim_start_matches('/')),
            description: definition.description.into(),
            resource: resource.into(),
            action: action.into(),
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
) -> Result<PermissionBinding, String> {
    for symbol in &route.guards {
        if !definitions.contains_key(symbol) {
            return Err(format!("{} {} 引用了未登记的权限 {symbol}", route.method, route.path));
        }
    }
    let symbol = route
        .permission
        .as_ref()
        .ok_or_else(|| format!("{} {} 缺少显式权限绑定", route.method, route.path))?;
    let catalog = definitions
        .get(symbol)
        .ok_or_else(|| format!("{} {} 引用了未登记的权限 {symbol}", route.method, route.path))?;
    let required_symbol = route.guards.last().unwrap_or(symbol);
    let required = definitions.get(required_symbol).ok_or("入口权限必须已登记")?;
    Ok(required.documented_as(*catalog))
}

/// 输出可对比的路由权限事实；不包含源码符号的迁移噪声。
pub(crate) fn route_contract(routes: &[RouteHandler]) -> Result<String, String> {
    let definitions = registry(MODULES)?;
    let mut rows = Vec::new();
    for route in routes {
        if exempt(route) {
            continue;
        }
        let definition = resolve(route, &definitions)?.catalog;
        rows.push(serde_json::json!({
            "method": route.method, "path": route.path, "handler": route.handler,
            "code": definition.code, "group": definition.group.name,
            "guards": route.guards.iter().map(|symbol| definitions.get(symbol).expect("全部权限层已校验").code).collect::<Vec<_>>(),
            "group_desc": definition.group.description, "description": definition.description,
        }));
    }
    serde_json::to_string_pretty(&rows).map_err(|error| error.to_string())
}

/// 为库测试生成对真实 Rust 常量的引用，防止显式索引名称与值配错。
#[cfg(not(test))]
pub(crate) fn compiled_references() -> Result<String, String> {
    let definitions = registry(MODULES)?;
    let mut names = definitions.keys().collect::<Vec<_>>();
    names.sort();
    let mut output = String::from("const COMPILED_DEFINITIONS: &[(&str, PermissionDef)] = &[\n");
    for name in names {
        output.push_str(&format!("({name:?}, permissions::{name}),\n"));
    }
    output.push_str("];\n");
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::{PermissionGroupDef, purchase_order};

    #[test]
    fn registered_catalog_preserves_all_frozen_route_bindings() {
        let routes = collect_routes(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        let actual: serde_json::Value = serde_json::from_str(&route_contract(&routes).unwrap()).unwrap();
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("testdata/permission-routes.json")).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn duplicate_symbols_and_inconsistent_groups_fail() {
        const DUPLICATE: PermissionModule = PermissionModule {
            name: "purchase",
            entries: &[("SUBMIT", purchase_order::SUBMIT), ("SUBMIT", purchase_order::SUBMIT)],
        };
        assert!(registry(&[DUPLICATE]).unwrap_err().contains("重复的权限定义"));
        const EMPTY: PermissionModule = PermissionModule { name: "purchase", entries: &[] };
        assert!(registry(&[EMPTY, EMPTY]).unwrap_err().contains("重复的权限模块"));
        const WRONG_GROUP: PermissionDef = purchase_order::SUBMIT.described(
            PermissionGroupDef { name: purchase_order::SUBMIT.group.name, description: "不同说明" },
            "接口",
        );
        const CONFLICT: PermissionModule = PermissionModule {
            name: "purchase",
            entries: &[("SUBMIT", purchase_order::SUBMIT), ("OTHER", WRONG_GROUP)],
        };
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
    fn invalid_codes_fail_before_generating_catalogs() {
        for code in ["", "a", "a:", ":read", "a:read:more", "a:bad/action", "a::read", "a/ /b:read"] {
            assert!(validate_code(code).is_err(), "{code}");
        }
        for code in ["purchase_order:submit", "a/b:read", "a:*", "*:*", "a-b:action_1"] {
            validate_code(code).unwrap();
        }
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
        assert_eq!(groups[0].1.desc, purchase_order::SUBMIT.group.description);
        assert_eq!(groups[0].1.permissions[0].module, "admin");
        assert_eq!(groups[0].1.permissions[0].path, "/admin/a");
        assert_eq!(groups[0].1.permissions[1].description, purchase_order::SUBMIT.description);
        assert!(groups.iter().flat_map(|(_, group)| &group.permissions).any(|item| item.method == "POLICY"
            && item.resource == "finance_ledger"
            && item.action == "read"));
    }
}
