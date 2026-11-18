#[path = "../../crates/domains/erp-identity/src/entity/access_control/audit_actions.rs"]
mod audit_actions;

mod build_catalog;
mod build_format;
mod build_permissions;
mod build_policy_catalog;
mod build_route_overrides;
mod route_descriptions;
use std::path::{Path, PathBuf};
use std::{env, fs, io};

use build_format::push_description;
use build_permissions::PermissionGroup;
use build_policy_catalog::write_policy_catalog;

// 在 Cargo 构建或独立生成权限目录时读取公共权限与实际路由，输出后端校验目录、路由合同及前端权限和审计目录；解析或写出失败时不继续生成。
fn main() {
    let manifest_dir = match env::var("ERP_PERMISSION_MANIFEST_DIR")
        .or_else(|_| env::var("CARGO_MANIFEST_DIR"))
        .map(PathBuf::from)
    {
        Ok(path) => path,
        Err(err) => {
            println!("cargo:warning=missing manifest dir: {}", err);
            return;
        },
    };
    let backend_root = match manifest_dir.parent().and_then(|path| path.parent()) {
        Some(path) => path.to_path_buf(),
        None => {
            println!("cargo:warning=missing backend root");
            return;
        },
    };
    // `erp-client` 位于仓库根（backend 的兄弟目录）。
    let repo_root = match backend_root.parent() {
        Some(path) => path.to_path_buf(),
        None => {
            println!("cargo:warning=missing repo root");
            return;
        },
    };

    let client_output_dir = env::var_os("ERP_PERMISSION_OUTPUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root.join("erp-client/lib"));
    fs::create_dir_all(&client_output_dir).expect("create catalog output directory");
    generate_audit_actions(&backend_root, &client_output_dir).expect("generate audit action catalog");
    rerun_if_changed(&backend_root.join("crates/foundation/erp-permissions/src"));

    let routes_mod_path = manifest_dir.join("src/http/routes/mod.rs");
    let routes_admin_path = manifest_dir.join("src/http/routes/admin.rs");
    rerun_if_changed(&manifest_dir.join("route_descriptions.rs"));
    rerun_if_changed(&manifest_dir.join("build_route_overrides.rs"));
    rerun_if_changed(&manifest_dir.join("build_catalog.rs"));
    rerun_if_changed(&manifest_dir.join("build_routes.rs"));
    rerun_if_changed(&manifest_dir.join("build_permissions.rs"));
    rerun_if_changed(&manifest_dir.join("build_format.rs"));
    rerun_if_changed(&manifest_dir.join("build_policy_catalog.rs"));
    rerun_if_changed(&manifest_dir.join("src/http/routes"));
    rerun_if_changed(&routes_mod_path);
    rerun_if_changed(&routes_admin_path);
    let source = fs::read_to_string(&routes_mod_path).expect("必须读取路由前缀");
    let prefix = parse_admin_prefix(&source).expect("必须声明管理端路由前缀");
    let route_handlers = build_catalog::collect_routes(&manifest_dir).expect("权限路由必须完整可解析");
    let groups = build_catalog::groups(&prefix, &route_handlers).expect("路由必须引用有效的显式权限定义");
    write_policy_catalog(&groups).expect("权限目录必须随运行版本生成");
    let contract = build_catalog::route_contract(&route_handlers).expect("路由权限合同必须可生成");
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").expect("Cargo provides OUT_DIR")).join("permission_routes.json"),
        contract,
    )
    .expect("必须写入路由权限合同");

    #[cfg(not(test))]
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").expect("Cargo provides OUT_DIR"))
            .join("permission_references.rs"),
        build_catalog::compiled_references().expect("权限引用必须可枚举"),
    )
    .expect("必须写入权限常量引用");

    // P0-4.1：权限生成物落点修正，输出到 erp-client（实际前端）。
    // erp-client 侧自包含 PermissionItem 类型（其 tsconfig 无 @/types/admin 映射）。
    let client_output_path = client_output_dir.join("permissions.generated.ts");
    write_generated_file(&client_output_path, &groups, client_permission_item_source())
        .expect("必须生成前端权限目录");
}

// 只将存在的源码路径登记为 Cargo 构建依赖，避免缺少独立文件的领域让构建一直失效。新增路由模块仍会通过已监听的路由目录与模块入口触发重建。
fn rerun_if_changed(path: &Path) {
    if !path.exists() {
        return;
    }
    println!("cargo:rerun-if-changed={}", path.display());
}

// 生成路由权限合同时，从根路由的 `nest` 注册查找 `admin::routes` 挂载前缀；找不到时不猜测路径。
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
                },
                _ => {},
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

// 把已归组权限和前端 `PermissionItem` 类型写成 TypeScript，供前端消费同一权限目录；传播文件写入错误。
fn write_generated_file(
    output_path: &Path,
    groups: &[(String, PermissionGroup)],
    permission_item_source: &str,
) -> io::Result<()> {
    let mut content = String::new();
    content.push_str("// @generated by apps/erp-server/build.rs. Do not edit.\n");
    content.push_str(permission_item_source);
    content.push_str("\nexport interface PermissionGroup {\n");
    content.push_str("    name: string\n");
    content.push_str("    description: string\n");
    content.push_str("    permissions: PermissionItem[]\n");
    content.push_str("}\n\n");
    content.push_str("export const PERMISSION_GROUPS: PermissionGroup[] = [\n");

    for (group_name, group) in groups {
        content.push_str("    {\n");
        content.push_str(&format!("        name: \"{}\",\n", escape_ts(group_name)));
        push_description(&mut content, 8, &group.desc);
        content.push_str("        permissions: [\n");
        for perm in &group.permissions {
            content.push_str("            {\n");
            content.push_str(&format!("                module: \"{}\",\n", escape_ts(&perm.module)));
            content.push_str(&format!("                method: \"{}\",\n", escape_ts(&perm.method)));
            content.push_str(&format!("                path: \"{}\",\n", escape_ts(&perm.path)));
            push_description(&mut content, 16, &perm.description);
            content.push_str("                permission: {\n");
            content.push_str(&format!("                    resource: \"{}\",\n", escape_ts(&perm.resource)));
            content.push_str(&format!("                    action: \"{}\",\n", escape_ts(&perm.action)));
            content.push_str("                },\n");
            content.push_str("            },\n");
        }
        content.push_str("        ],\n");
        content.push_str("    },\n");
    }

    content.push_str("]\n");

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    if let Ok(existing) = fs::read_to_string(output_path)
        && existing == content
    {
        return Ok(());
    }

    fs::write(output_path, content)
}

// 在写出权限目录的 TypeScript 字符串字面量前转义内容，避免描述中的特殊字符改变生成源码。
fn escape_ts(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

// 为前端权限目录提供自包含的 `PermissionItem` 类型，不依赖 erp-client 未配置的 `@/types/admin` 路径别名。
fn client_permission_item_source() -> &'static str {
    concat!(
        "export interface PermissionItem {\n",
        "    module: string\n",
        "    method: string\n",
        "    path: string\n",
        "    description: string\n",
        "    permission: {\n",
        "        resource: string\n",
        "        action: string\n",
        "    }\n",
        "}\n",
    )
}

// 从审计动作定义生成前端固定目录；写入失败必须中止构建，不允许沿用旧目录。
fn generate_audit_actions(backend_root: &Path, client_output_dir: &Path) -> io::Result<()> {
    rerun_if_changed(
        &backend_root.join("crates/domains/erp-identity/src/entity/access_control/audit_actions.rs"),
    );
    let mut output = String::from(
        "// Generated by apps/erp-server/build.rs from identity audit_actions.rs. Do not edit.\nexport const AUDIT_ACTION_OPTIONS = [\n",
    );
    for (value, label) in audit_actions::AUDIT_ACTIONS {
        output.push_str(&format!("    {{\n        value: {value:?},\n        label: {label:?},\n    }},\n"));
    }
    output.push_str("] as const\n");
    let target = client_output_dir.join("audit-actions.generated.ts");
    if fs::read_to_string(&target).ok().as_deref() != Some(output.as_str()) {
        fs::write(target, output)?;
    }
    Ok(())
}
