//! 从路由注册目录生成授权文件校验白名单。
use std::path::PathBuf;
use std::{env, fs, io};

use crate::build_permissions::PermissionGroup;

/// 同一注册来源同时生成后端文件校验目录，避免维护第二份权限白名单。
/// # 参数
/// groups 为路由及领域政策构建的统一权限目录。
/// # 返回
/// 后端校验目录写入 OUT_DIR 成功。
/// # 错误
/// 目录文件不能写入时返回 I/O 错误。
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
