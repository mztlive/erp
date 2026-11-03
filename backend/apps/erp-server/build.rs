#[path = "../../crates/domains/erp-identity/src/entity/access_control/audit_actions.rs"]
mod audit_actions;

mod build_catalog;
mod build_format;
mod build_permissions;
mod build_policy_catalog;
#[path = "permission_catalog/mod.rs"]
mod permissions;
use std::path::{Path, PathBuf};
use std::{env, fs, io};

use build_format::push_description;
use build_permissions::PermissionGroup;
use build_policy_catalog::write_policy_catalog;
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

    generate_audit_actions(&backend_root, &repo_root).expect("generate audit action catalog");
    rerun_if_changed(&backend_root.join("crates/domains/erp-identity/src/entity/policy_permission.rs"));

    let routes_mod_path = manifest_dir.join("src/http/routes/mod.rs");
    let routes_admin_path = manifest_dir.join("src/http/routes/admin.rs");
    rerun_if_changed(&manifest_dir.join("permission_catalog"));
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

    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").expect("Cargo provides OUT_DIR"))
            .join("permission_references.rs"),
        build_catalog::compiled_references().expect("权限引用必须可枚举"),
    )
    .expect("必须写入权限常量引用");

    // P0-4.1：权限生成物落点修正，输出到 erp-client（实际前端）。
    // erp-client 侧自包含 PermissionItem 类型（其 tsconfig 无 @/types/admin 映射）。
    let client_output_path = repo_root.join("erp-client/lib/permissions.generated.ts");
    write_generated_file(&client_output_path, &groups, client_permission_item_source())
        .expect("必须生成前端权限目录");
}

/// 记录 build.rs 的文件依赖，路径不存在时跳过。
///
/// cargo 把指向不存在路径的 `rerun-if-changed` 永久判定为脏，会导致 build script
/// 与 web-api 每次 `cargo test` / `cargo build` 无条件重编译。`DOMAIN_MODULES` 中
/// 存在没有独立路由文件的域（如 organization 的路由挂在 `routes/access_control.rs`），
/// 故此处按存在性过滤。新增路由文件必然伴随 `routes/mod.rs` 改动，该文件已被监听。
///
/// # 参数
/// * `path` - 路径
///
/// # 返回
/// 不返回数据，仅表示执行结果。
fn rerun_if_changed(path: &Path) {
    if !path.exists() {
        return;
    }
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

/// 生成并写入权限配置文件。
///
/// # 参数
/// * `output_path` - 输出路径
/// * `groups` - 权限分组
/// * `permission_item_source` - `PermissionItem` 类型来源（import 行或内联类型定义）
///
/// # 返回
/// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
///
/// # 错误
/// 当验证失败或底层操作失败时返回错误。
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

/// 返回 erp-client 侧 `PermissionItem` 的内联类型定义。
///
/// `erp-client` 的 tsconfig 没有 `@/types/admin` 映射，且代码风格为 4 空格缩进、
/// 无分号；生成文件按该风格输出，避免 lint/tsc 漂移。
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

/// 从事件拥有领域生成前端固定目录；写失败使构建失败，禁止保留过期生成物。
fn generate_audit_actions(backend_root: &Path, repo_root: &Path) -> io::Result<()> {
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
    let target = repo_root.join("erp-client/lib/audit-actions.generated.ts");
    if fs::read_to_string(&target).ok().as_deref() != Some(output.as_str()) {
        fs::write(target, output)?;
    }
    Ok(())
}
