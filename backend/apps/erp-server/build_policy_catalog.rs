//! 从路由注册目录生成授权文件校验白名单。
use std::path::PathBuf;
use std::{env, fs, io};

use crate::build_permissions::PermissionGroup;

// 用路由和领域政策共用的权限分组生成 `OUT_DIR` 后端白名单；写入失败时返回 I/O 错误，不从另一份清单兜底。
pub(crate) fn write_policy_catalog(groups: &[(String, PermissionGroup)]) -> io::Result<()> {
    let mut codes = groups
        .iter()
        .flat_map(|(_, group)| &group.permissions)
        .map(|permission| format!("{}:{}", permission.resource, permission.action))
        .collect::<Vec<_>>();
    codes.sort();
    codes.dedup();
    let mut content = String::from("const POLICY_PERMISSION_CODES: &[&str] = &[\n");
    for code in codes {
        content.push_str(&format!("{code:?},\n"));
    }
    content.push_str("];\n");
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").expect("Cargo provides OUT_DIR")).join("policy_permissions.rs"),
        content,
    )
}
