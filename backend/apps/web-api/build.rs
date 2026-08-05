use std::{
    collections::{HashMap, HashSet},
    env, fs, io,
    path::{Path, PathBuf},
};

use syn::{
    punctuated::Punctuated, spanned::Spanned, Attribute, Expr, File, Item, ItemFn, Lit, MetaNameValue, Token,
};

#[derive(Debug, Clone)]
struct PermissionMeta {
    group: String,
    group_desc: String,
    desc: String,
    resource: Option<String>,
    action: Option<String>,
}

#[derive(Debug, Clone)]
struct RouteHandler {
    method: String,
    path: String,
    handler: String,
}

#[derive(Debug, Default)]
struct PermissionGroup {
    desc: String,
    permissions: Vec<PermissionItem>,
}

#[derive(Debug, Clone)]
struct PermissionItem {
    module: String,
    method: String,
    path: String,
    description: String,
    resource: String,
    action: String,
}

/// 构建脚本入口。
///
/// # 返回
/// 不返回数据，仅表示执行结果。
fn main() {
    let manifest_dir = match env::var("CARGO_MANIFEST_DIR").map(PathBuf::from) {
        Ok(path) => path,
        Err(err) => {
            println!("cargo:warning=missing manifest dir: {}", err);
            return;
        }
    };
    let repo_root = match manifest_dir.parent().and_then(|path| path.parent()) {
        Some(path) => path.to_path_buf(),
        None => {
            println!("cargo:warning=missing repo root");
            return;
        }
    };

    let routes_mod_path = manifest_dir.join("src/core/routes/mod.rs");
    let routes_admin_path = manifest_dir.join("src/core/routes/admin.rs");
    let handlers_dir = manifest_dir.join("src/core/handler/admin");

    rerun_if_changed(&routes_mod_path);
    rerun_if_changed(&routes_admin_path);

    let handler_files = match collect_rs_files(&handlers_dir) {
        Ok(files) => files,
        Err(err) => {
            println!("cargo:warning=failed to collect handler files: {}", err);
            return;
        }
    };

    for file in &handler_files {
        rerun_if_changed(file);
    }

    let prefix = match fs::read_to_string(&routes_mod_path) {
        Ok(content) => parse_admin_prefix(&content).unwrap_or_else(|| "/admin".to_string()),
        Err(err) => {
            println!(
                "cargo:warning=failed to read routes mod file {}: {}",
                routes_mod_path.display(),
                err
            );
            return;
        }
    };

    let handler_meta = match parse_handler_permissions(&handlers_dir, &handler_files) {
        Ok(meta) => meta,
        Err(err) => {
            println!("cargo:warning=failed to parse handler permissions: {}", err);
            return;
        }
    };

    let route_handlers = match fs::read_to_string(&routes_admin_path) {
        Ok(content) => parse_routes(&content),
        Err(err) => {
            println!(
                "cargo:warning=failed to read admin routes file {}: {}",
                routes_admin_path.display(),
                err
            );
            return;
        }
    };

    let (groups, used_handlers) = build_permission_groups(&prefix, &handler_meta, &route_handlers);
    for handler in handler_meta.keys() {
        if !used_handlers.contains(handler) {
            println!("cargo:warning=handler '{}' not found in routes", handler);
        }
    }

    let output_path = repo_root.join("fronts/admin/src/constants/permissions.generated.ts");
    if let Err(err) = write_generated_file(&output_path, &groups) {
        println!("cargo:warning=failed to write generated permissions: {}", err);
    }
}

/// 记录 build.rs 的文件依赖。
///
/// # 参数
/// * `path` - 路径
///
/// # 返回
/// 不返回数据，仅表示执行结果。
fn rerun_if_changed(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.display());
}

/// 解析 `/admin` 路由前缀。
///
/// # 参数
/// * `content` - 文件内容
///
/// # 返回
/// 返回可选结果，`Some` 表示存在，`None` 表示不存在。
fn parse_admin_prefix(content: &str) -> Option<String> {
    let marker = "nest(";
    let mut index = 0;
    while let Some(pos) = content[index..].find(marker) {
        let start = index + pos + marker.len();
        let mut cursor = start;
        let bytes = content.as_bytes();
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] != b'"' {
            index = start;
            continue;
        }
        cursor += 1;
        let path_start = cursor;
        while cursor < bytes.len() && bytes[cursor] != b'"' {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }
        let path = &content[path_start..cursor];
        cursor += 1;

        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }

        if cursor >= bytes.len() || bytes[cursor] != b',' {
            index = cursor;
            continue;
        }
        cursor += 1;

        let expr_start = cursor;
        let mut depth = 1;
        let mut end = cursor;
        while end < bytes.len() {
            match bytes[end] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            end += 1;
        }

        if end >= bytes.len() {
            break;
        }

        let expr = &content[expr_start..end];
        if expr.contains("admin::routes") {
            return Some(path.to_string());
        }

        index = end + 1;
    }
    None
}

/// 读取 handler 目录并解析权限注解。
///
/// # 参数
/// * `handler_root` - 处理器根目录
/// * `handler_files` - 处理器文件集合
///
/// # 返回
/// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
///
/// # 错误
/// 当验证失败或底层操作失败时返回错误。
fn parse_handler_permissions(
    handler_root: &Path,
    handler_files: &[PathBuf],
) -> io::Result<HashMap<String, PermissionMeta>> {
    let mut out = HashMap::new();

    for file_path in handler_files {
        if file_path.file_name().and_then(|name| name.to_str()) == Some("mod.rs") {
            continue;
        }

        let module_path = match module_path_for_file(handler_root, file_path) {
            Some(path) => path,
            None => continue,
        };

        let content = fs::read_to_string(file_path)?;
        let file = match syn::parse_file(&content) {
            Ok(file) => file,
            Err(err) => {
                println!(
                    "cargo:warning=failed to parse handler file {}: {}",
                    file_path.display(),
                    err
                );
                continue;
            }
        };

        collect_permission_meta(&file, &module_path, &mut out);
    }

    Ok(out)
}

/// 收集文件中的权限注解信息。
///
/// # 参数
/// * `file` - 文件
/// * `module_path` - 模块路径
/// * `out` - 输出收集器
///
/// # 返回
/// 不返回数据，仅表示执行结果。
fn collect_permission_meta(file: &File, module_path: &str, out: &mut HashMap<String, PermissionMeta>) {
    for item in &file.items {
        let Item::Fn(item_fn) = item else {
            continue;
        };
        if let Some(meta) = extract_permission_meta(item_fn) {
            let handler_name = format!("{}::{}", module_path, item_fn.sig.ident);
            if out.insert(handler_name.clone(), meta).is_some() {
                println!(
                    "cargo:warning=duplicate permission metadata for handler '{}'",
                    handler_name
                );
            }
        }
    }
}

/// 从函数属性中提取权限注解。
///
/// # 参数
/// * `item_fn` - 函数节点
///
/// # 返回
/// 返回可选结果，`Some` 表示存在，`None` 表示不存在。
fn extract_permission_meta(item_fn: &ItemFn) -> Option<PermissionMeta> {
    let attrs = &item_fn.attrs;
    for attr in attrs {
        if !is_permission_attr(attr) {
            continue;
        }

        match parse_permission_args(attr) {
            Ok(meta) => return Some(meta),
            Err(err) => {
                println!(
                    "cargo:warning=failed to parse permission attribute for {}: {}",
                    item_fn.sig.ident, err
                );
                return None;
            }
        }
    }

    None
}

/// 判断属性是否为 permission 标注。
///
/// # 参数
/// * `attr` - 属性节点
///
/// # 返回
/// 返回布尔值表示条件是否满足或操作是否成功。
fn is_permission_attr(attr: &Attribute) -> bool {
    attr.path()
        .segments
        .last()
        .map(|seg| seg.ident == "permission")
        .unwrap_or(false)
}

/// 解析 permission 宏参数。
///
/// # 参数
/// * `attr` - 属性节点
///
/// # 返回
/// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
///
/// # 错误
/// 当验证失败或底层操作失败时返回错误。
fn parse_permission_args(attr: &Attribute) -> syn::Result<PermissionMeta> {
    let parser = Punctuated::<MetaNameValue, Token![,]>::parse_terminated;
    let args = attr.parse_args_with(parser)?;

    let mut group = None;
    let mut group_desc = None;
    let mut desc = None;
    let mut resource = None;
    let mut action = None;

    for arg in args {
        let name = arg.path.get_ident().map(|ident| ident.to_string());
        let value = match arg.value {
            Expr::Lit(expr) => match expr.lit {
                Lit::Str(lit) => lit.value(),
                _ => continue,
            },
            _ => continue,
        };

        match name.as_deref() {
            Some("group") => group = Some(value),
            Some("group_desc") => group_desc = Some(value),
            Some("desc") => desc = Some(value),
            Some("resource") => resource = Some(value),
            Some("action") => action = Some(value),
            _ => {}
        }
    }

    let group = group.ok_or_else(|| syn::Error::new(attr.span(), "missing group"))?;
    let group_desc = group_desc.ok_or_else(|| syn::Error::new(attr.span(), "missing group_desc"))?;
    let desc = desc.ok_or_else(|| syn::Error::new(attr.span(), "missing desc"))?;

    Ok(PermissionMeta {
        group,
        group_desc,
        desc,
        resource,
        action,
    })
}

/// 获取 handler 文件对应的模块路径。
///
/// # 参数
/// * `handler_root` - 处理器根目录
/// * `file_path` - 文件路径
///
/// # 返回
/// 返回可选结果，`Some` 表示存在，`None` 表示不存在。
fn module_path_for_file(handler_root: &Path, file_path: &Path) -> Option<String> {
    let relative = file_path.strip_prefix(handler_root).ok()?;
    let mut segments = vec!["admin".to_string()];

    for component in relative.components() {
        let name = component.as_os_str().to_string_lossy();
        if name.ends_with(".rs") {
            let stem = name.trim_end_matches(".rs");
            if stem == "mod" {
                continue;
            }
            segments.push(stem.to_string());
        } else {
            segments.push(name.to_string());
        }
    }

    Some(segments.join("::"))
}

/// 递归收集目录下所有 Rust 文件。
///
/// # 参数
/// * `root` - 根目录
///
/// # 返回
/// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
///
/// # 错误
/// 当验证失败或底层操作失败时返回错误。
fn collect_rs_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_rs_files_inner(root, &mut files)?;
    Ok(files)
}

/// 递归收集 Rust 文件的内部实现。
///
/// # 参数
/// * `root` - 根目录
/// * `files` - 文件集合
///
/// # 返回
/// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
///
/// # 错误
/// 当验证失败或底层操作失败时返回错误。
fn collect_rs_files_inner(root: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files_inner(&path, files)?;
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path);
        }
    }
    Ok(())
}

/// 解析 admin routes 文件中的路由信息。
///
/// # 参数
/// * `content` - 文件内容
///
/// # 返回
/// 返回结果集合。
fn parse_routes(content: &str) -> Vec<RouteHandler> {
    let mut result = Vec::new();
    let mut index = 0;
    let marker = ".route(";

    while let Some(pos) = content[index..].find(marker) {
        let start = index + pos + marker.len();
        let bytes = content.as_bytes();
        let mut cursor = start;

        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }

        if cursor >= bytes.len() || bytes[cursor] != b'"' {
            index = start;
            continue;
        }
        cursor += 1;
        let path_start = cursor;
        while cursor < bytes.len() && bytes[cursor] != b'"' {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }
        let path = content[path_start..cursor].to_string();
        cursor += 1;

        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] != b',' {
            index = cursor;
            continue;
        }
        cursor += 1;
        let expr_start = cursor;
        let mut depth = 1;
        let mut end = cursor;
        while end < bytes.len() {
            match bytes[end] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            end += 1;
        }
        if end >= bytes.len() {
            break;
        }

        let expr = content[expr_start..end].trim();
        for handler in parse_handler_chain(expr) {
            result.push(RouteHandler {
                method: handler.method,
                path: path.clone(),
                handler: handler.handler,
            });
        }

        index = end + 1;
    }

    result
}

/// 解析 `.route()` 中的方法链。
///
/// # 参数
/// * `expr` - 表达式节点
///
/// # 返回
/// 返回结果集合。
fn parse_handler_chain(expr: &str) -> Vec<RouteHandler> {
    let methods = ["get", "post", "put", "delete", "patch", "head"];
    let bytes = expr.as_bytes();
    let mut index = 0;
    let mut out = Vec::new();

    while index < bytes.len() {
        if !bytes[index].is_ascii_alphabetic() {
            index += 1;
            continue;
        }

        let mut matched = None;
        for method in methods {
            if expr[index..].starts_with(method) {
                matched = Some(method);
                break;
            }
        }

        let Some(method) = matched else {
            index += 1;
            continue;
        };

        let mut cursor = index + method.len();
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }

        if cursor >= bytes.len() || bytes[cursor] != b'(' {
            index += method.len();
            continue;
        }

        cursor += 1;
        let handler_start = cursor;
        let mut depth = 1;
        while cursor < bytes.len() {
            match bytes[cursor] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }

        let handler = expr[handler_start..cursor].trim().to_string();
        out.push(RouteHandler {
            method: method.to_uppercase(),
            path: String::new(),
            handler,
        });

        index = cursor + 1;
    }

    out
}

/// 构建权限分组并返回已使用的 handler 集合。
///
/// # 参数
/// * `prefix` - 前缀
/// * `handler_meta` - 处理器元数据
/// * `route_handlers` - 路由处理器映射
///
/// # 返回
/// 返回 `(Vec<(String, PermissionGroup)>, HashSet<String>)` 结果。
fn build_permission_groups(
    prefix: &str,
    handler_meta: &HashMap<String, PermissionMeta>,
    route_handlers: &[RouteHandler],
) -> (Vec<(String, PermissionGroup)>, HashSet<String>) {
    let mut groups: HashMap<String, PermissionGroup> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut used_handlers = HashSet::new();

    for route in route_handlers {
        let Some(meta) = handler_meta.get(&route.handler) else {
            println!(
                "cargo:warning=missing #[permission] for handler '{}'",
                route.handler
            );
            continue;
        };

        used_handlers.insert(route.handler.clone());

        let entry = groups.entry(meta.group.clone()).or_insert_with(|| {
            order.push(meta.group.clone());
            PermissionGroup {
                desc: meta.group_desc.clone(),
                permissions: Vec::new(),
            }
        });

        if entry.desc != meta.group_desc {
            println!(
                "cargo:warning=group_desc mismatch for group '{}': '{}' vs '{}'",
                meta.group, entry.desc, meta.group_desc
            );
        }

        let Some(resource) = meta.resource.clone() else {
            println!(
                "cargo:warning=missing resource for permission mapping '{}'",
                route.handler
            );
            continue;
        };
        let Some(action) = meta.action.clone() else {
            println!(
                "cargo:warning=missing action for permission mapping '{}'",
                route.handler
            );
            continue;
        };
        entry.permissions.push(PermissionItem {
            module: "admin".to_string(),
            method: route.method.clone(),
            path: join_paths(prefix, &route.path),
            description: meta.desc.clone(),
            resource,
            action,
        });
    }

    let ordered = order
        .into_iter()
        .filter_map(|name| groups.remove(&name).map(|group| (name, group)))
        .collect();

    (ordered, used_handlers)
}

/// 拼接路由前缀与路径。
///
/// # 参数
/// * `prefix` - 前缀
/// * `path` - 路径
///
/// # 返回
/// 返回字符串结果。
fn join_paths(prefix: &str, path: &str) -> String {
    let left = prefix.trim_end_matches('/');
    let right = path.trim_start_matches('/');
    format!("{}/{}", left, right)
}

/// 生成并写入权限配置文件。
///
/// # 参数
/// * `output_path` - 输出路径
/// * `groups` - 权限分组
///
/// # 返回
/// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
///
/// # 错误
/// 当验证失败或底层操作失败时返回错误。
fn write_generated_file(output_path: &Path, groups: &[(String, PermissionGroup)]) -> io::Result<()> {
    let mut content = String::new();
    content.push_str("// @generated by apps/web-api/build.rs. Do not edit.\n");
    content.push_str("import type { PermissionItem } from \"@/types/admin\";\n\n");
    content.push_str("export interface PermissionGroup {\n");
    content.push_str("    name: string;\n");
    content.push_str("    description: string;\n");
    content.push_str("    permissions: PermissionItem[];\n");
    content.push_str("}\n\n");
    content.push_str("export const PERMISSION_GROUPS: PermissionGroup[] = [\n");

    for (group_name, group) in groups {
        content.push_str("    {\n");
        content.push_str(&format!("        name: \"{}\",\n", escape_ts(group_name)));
        content.push_str(&format!("        description: \"{}\",\n", escape_ts(&group.desc)));
        content.push_str("        permissions: [\n");
        for perm in &group.permissions {
            content.push_str("            {\n");
            content.push_str(&format!(
                "                module: \"{}\",\n",
                escape_ts(&perm.module)
            ));
            content.push_str(&format!(
                "                method: \"{}\",\n",
                escape_ts(&perm.method)
            ));
            content.push_str(&format!("                path: \"{}\",\n", escape_ts(&perm.path)));
            content.push_str(&format!(
                "                description: \"{}\",\n",
                escape_ts(&perm.description)
            ));
            content.push_str("                permission: {\n");
            content.push_str(&format!(
                "                    resource: \"{}\",\n",
                escape_ts(&perm.resource)
            ));
            content.push_str(&format!(
                "                    action: \"{}\",\n",
                escape_ts(&perm.action)
            ));
            content.push_str("                },\n");
            content.push_str("            },\n");
        }
        content.push_str("        ],\n");
        content.push_str("    },\n");
    }

    content.push_str(
        "];
",
    );

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    if let Ok(existing) = fs::read_to_string(output_path) {
        if existing == content {
            return Ok(());
        }
    }

    fs::write(output_path, content)
}

/// 转义 TS 字符串中的特殊字符。
///
/// # 参数
/// * `value` - 值
///
/// # 返回
/// 返回字符串结果。
fn escape_ts(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
