use std::fmt;

use clap::{Args, Parser, Subcommand};

/// ERP 运维命令行。
#[derive(Debug, Parser)]
#[command(
    name = "cli",
    about = "初始化超级管理员与重置管理员密码",
    disable_help_subcommand = true
)]
pub struct Cli {
    /// TOML 配置文件路径。
    #[arg(short, long, default_value = "./config.toml", global = true)]
    pub config_path: String,
    /// 子命令。
    #[command(subcommand)]
    pub command: Command,
}

/// 支持的运维子命令。
#[derive(Debug, Subcommand)]
pub enum Command {
    /// 创建或修复系统超级管理员。
    InitAdmin(InitAdminArgs),
    /// 重置已有管理员密码，不改角色。
    ResetPassword(ResetPasswordArgs),
}

/// 初始化超级管理员参数。
#[derive(Args)]
pub struct InitAdminArgs {
    /// 登录账号。
    #[arg(long)]
    pub account: String,
    /// 显示名称。
    #[arg(long)]
    pub name: String,
    /// 明文密码。未提供时读取 `ERP_ADMIN_PASSWORD` 或交互输入。
    #[arg(long)]
    pub password: Option<String>,
}

/// 重置管理员密码参数。
#[derive(Args)]
pub struct ResetPasswordArgs {
    /// 登录账号。
    #[arg(long)]
    pub account: String,
    /// 明文密码。未提供时读取 `ERP_ADMIN_PASSWORD` 或交互输入。
    #[arg(long)]
    pub password: Option<String>,
}

impl fmt::Debug for InitAdminArgs {
    /// 输出不含明文密码的调试信息。
    ///
    /// # 参数
    /// * `formatter` - 调试格式化器
    ///
    /// # 返回值
    /// 格式化成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 写入格式化器失败时返回底层格式化错误。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InitAdminArgs")
            .field("account", &self.account)
            .field("name", &self.name)
            .field("password", &redacted_password(&self.password))
            .finish()
    }
}

impl fmt::Debug for ResetPasswordArgs {
    /// 输出不含明文密码的调试信息。
    ///
    /// # 参数
    /// * `formatter` - 调试格式化器
    ///
    /// # 返回值
    /// 格式化成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 写入格式化器失败时返回底层格式化错误。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResetPasswordArgs")
            .field("account", &self.account)
            .field("password", &redacted_password(&self.password))
            .finish()
    }
}

/// 将可选密码替换为脱敏标记。
///
/// # 参数
/// * `password` - 命令行中的可选明文密码
///
/// # 返回值
/// 未提供时返回 `None`，已提供时返回脱敏标记。
///
/// # 错误
/// 无。
fn redacted_password(password: &Option<String>) -> Option<&'static str> {
    password.as_ref().map(|_| "[REDACTED]")
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, Command};

    #[test]
    fn parses_init_admin_with_global_config_path() {
        let cli = Cli::try_parse_from([
            "cli",
            "--config-path",
            "./config.toml",
            "init-admin",
            "--account",
            "admin",
            "--name",
            "System Admin",
            "--password",
            "secret1",
        ])
        .unwrap();

        assert_eq!(cli.config_path, "./config.toml");
        match cli.command {
            Command::InitAdmin(args) => {
                assert_eq!(args.account, "admin");
                assert_eq!(args.name, "System Admin");
                assert_eq!(args.password.as_deref(), Some("secret1"));
            }
            Command::ResetPassword(_) => panic!("应解析为 init-admin"),
        }
    }

    #[test]
    fn parses_reset_password_without_flag_password() {
        let cli = Cli::try_parse_from(["cli", "reset-password", "--account", "admin"]).unwrap();

        match cli.command {
            Command::ResetPassword(args) => {
                assert_eq!(args.account, "admin");
                assert!(args.password.is_none());
            }
            Command::InitAdmin(_) => panic!("应解析为 reset-password"),
        }
    }

    #[test]
    fn debug_output_redacts_password() {
        let cli = Cli::try_parse_from([
            "cli",
            "reset-password",
            "--account",
            "admin",
            "--password",
            "visible-secret",
        ])
        .unwrap();

        let debug = format!("{cli:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("visible-secret"));
    }
}
