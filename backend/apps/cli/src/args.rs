use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

use clap::error::ErrorKind;
use clap::{Args, Parser, Subcommand};
use config::ConfigArgs;

/// ERP 运维命令行。
#[derive(Debug, Parser)]
#[command(name = "cli", about = "ERP 管理与受控运维命令", disable_help_subcommand = true)]
pub struct Cli {
    /// 文件或 Nacos 配置来源。
    #[command(flatten)]
    pub config: ConfigArgs,
    /// 子命令。
    #[command(subcommand)]
    pub command: Command,
}

/// 支持的运维子命令。
#[derive(Debug, Subcommand)]
pub enum Command {
    /// 从本地文件导入京东已有授权，默认只预览；--apply 显式写入。
    #[command(name = "jd-vop-credential-import")]
    JdVopCredentialImport(JdVopCredentialImportArgs),
    /// 查询配置绑定的京东凭据脱敏状态，不读取或刷新远端令牌。
    #[command(name = "jd-vop-credential-status")]
    JdVopCredentialStatus,
    /// 登记合同模板新增集合及唯一索引，不修改历史合同。
    MigrateContractTemplates,
    /// 创建或修复系统超级管理员。
    InitAdmin(InitAdminArgs),
    /// 重置已有管理员密码，不改角色。
    ResetPassword(ResetPasswordArgs),
}

impl Cli {
    /// 解析带程序名的完整 CLI 参数。帮助和版本保留 Clap 的标准输出；用法错误只保留类别
    /// 和固定提示，避免把误传的令牌或密码回显到终端。
    pub fn try_parse_safe_from<I, T>(arguments: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        Self::try_parse_from(arguments).map_err(|error| match error.kind() {
            ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => error,
            kind => clap::Error::raw(
                kind,
                "命令参数无效；请使用 --help。京东凭据只允许通过 --file 读取，不接受令牌参数。",
            ),
        })
    }
}

/// 京东凭据导入参数；令牌只能来自受限文件或标准输入。
#[derive(Args)]
pub struct JdVopCredentialImportArgs {
    /// 本地授权 JSON 文件；- 表示标准输入，最大 65536 字节。
    #[arg(long, value_name = "PATH")]
    pub file: PathBuf,
    /// 替换已有记录所需的脱敏状态版本；首次导入不提供。
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
    pub expected_version: Option<u64>,
    /// 明确写入；未提供时只验证并预览。
    #[arg(long)]
    pub apply: bool,
    /// 导入重新授权取得的新令牌，必须提供版本和新授权依据。
    #[arg(long, requires_all = ["expected_version", "evidence_reference"])]
    pub reauthorized: bool,
    /// 新授权的可追溯凭证引用；不得填入令牌或授权响应正文。
    #[arg(long, requires = "reauthorized")]
    pub evidence_reference: Option<String>,
}

impl fmt::Debug for JdVopCredentialImportArgs {
    // 调试导入命令时保留版本和操作模式，隐藏本地输入路径及重新授权依据。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JdVopCredentialImportArgs")
            .field("file", &"[LOCAL INPUT]")
            .field("expected_version", &self.expected_version)
            .field("apply", &self.apply)
            .field("reauthorized", &self.reauthorized)
            .field("evidence_reference", &self.evidence_reference.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
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
    // 诊断初始化参数时保留目标账号和显示名称，密码只显示是否提供。
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
    // 诊断密码重置参数时保留目标账号，不让已传入的密码进入调试输出。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResetPasswordArgs")
            .field("account", &self.account)
            .field("password", &redacted_password(&self.password))
            .finish()
    }
}

// 供两个管理员命令的调试输出使用：已提供的密码统一显示为遮罩，未提供的仍显示为缺省。
fn redacted_password(password: &Option<String>) -> Option<&'static str> {
    password.as_ref().map(|_| "[REDACTED]")
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, Command};

    #[test]
    fn credential_import_defaults_to_preview_and_accepts_explicit_versioned_apply() {
        for (options, apply, version) in [
            (vec![], false, None),
            (vec!["--expected-version", "3"], false, Some(3)),
            (vec!["--apply", "--expected-version", "3"], true, Some(3)),
        ] {
            let mut argv = vec!["cli", "jd-vop-credential-import", "--file", "-"];
            argv.extend(options);
            let cli = Cli::try_parse_safe_from(argv).unwrap();
            let Command::JdVopCredentialImport(args) = cli.command else {
                panic!("应解析为京东凭据导入");
            };
            assert_eq!(args.file.to_str(), Some("-"));
            assert_eq!(args.apply, apply);
            assert_eq!(args.expected_version, version);
            assert!(!args.reauthorized);
        }
        assert!(matches!(
            Cli::try_parse_safe_from(["cli", "jd-vop-credential-status"]).unwrap().command,
            Command::JdVopCredentialStatus
        ));
    }

    #[test]
    fn reauthorization_requires_a_version_and_new_authorization_evidence() {
        let base = ["cli", "jd-vop-credential-import", "--file", "credentials.json"];
        for options in [
            vec!["--reauthorized"],
            vec!["--reauthorized", "--expected-version", "1"],
            vec!["--reauthorized", "--evidence-reference", "authorization-ticket"],
            vec!["--evidence-reference", "authorization-ticket"],
            vec!["--expected-version", "0"],
        ] {
            assert!(Cli::try_parse_safe_from(base.into_iter().chain(options)).is_err());
        }
        let cli = Cli::try_parse_safe_from(base.into_iter().chain([
            "--reauthorized",
            "--expected-version",
            "1",
            "--evidence-reference",
            "authorization-ticket",
        ]))
        .unwrap();
        let Command::JdVopCredentialImport(args) = cli.command else {
            panic!("应解析为京东凭据导入");
        };
        assert!(args.reauthorized);
        assert!(!args.apply);
        assert_eq!(args.evidence_reference.as_deref(), Some("authorization-ticket"));
    }

    #[test]
    fn credential_arguments_and_parse_failures_never_echo_secrets() {
        for flag in
            ["--access-token=never-print-token", "--refresh-token=never-print-token", "never-print-token"]
        {
            let error = Cli::try_parse_safe_from([
                "cli",
                "jd-vop-credential-import",
                "--file",
                "credentials.json",
                flag,
            ])
            .unwrap_err();
            assert!(!error.to_string().contains("never-print-token"));
            assert!(!format!("{error:?}").contains("never-print-token"));
        }
        let cli = Cli::try_parse_safe_from([
            "cli",
            "jd-vop-credential-import",
            "--file",
            "private-file-name",
            "--reauthorized",
            "--expected-version",
            "1",
            "--evidence-reference",
            "private-evidence-reference",
        ])
        .unwrap();
        let debug = format!("{cli:?}");
        assert!(!debug.contains("private-file-name"));
        assert!(!debug.contains("private-evidence-reference"));
    }

    #[test]
    fn safe_parser_preserves_help_and_requires_local_input_selection() {
        let error = Cli::try_parse_safe_from(["cli", "jd-vop-credential-import", "--help"]).unwrap_err();
        assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
        assert!(error.to_string().contains("--file"));
        assert!(Cli::try_parse_safe_from(["cli", "jd-vop-credential-import"]).is_err());
        assert!(Cli::try_parse_safe_from(["cli", "jd-vop-credential-status", "--apply"]).is_err());
    }

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

        assert_eq!(cli.config.config_path, "./config.toml");
        match cli.command {
            Command::InitAdmin(args) => {
                assert_eq!(args.account, "admin");
                assert_eq!(args.name, "System Admin");
                assert_eq!(args.password.as_deref(), Some("secret1"));
            },
            _ => panic!("应解析为 init-admin"),
        }
    }

    #[test]
    fn parses_reset_password_without_flag_password() {
        let cli = Cli::try_parse_from(["cli", "reset-password", "--account", "admin"]).unwrap();

        match cli.command {
            Command::ResetPassword(args) => {
                assert_eq!(args.account, "admin");
                assert!(args.password.is_none());
            },
            _ => panic!("应解析为 reset-password"),
        }
    }

    #[test]
    fn retired_scope_migration_is_not_an_available_command() {
        let error = Cli::try_parse_from(["cli", "migrate-person-scopes", "--user-id", "alice"]).unwrap_err();
        assert_eq!(error.kind(), clap::error::ErrorKind::InvalidSubcommand);
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
