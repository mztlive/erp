use clap::{ArgAction, Parser};

use crate::nacos::{NacosConfig, NacosConfigClient};
use crate::{Config, Error, Result};

/// API 与运维 CLI 共用的配置来源参数；凭据只从环境变量读取。
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct ConfigArgs {
    /// 本地 TOML 配置路径；不得与 Nacos 模式混用。
    #[arg(short, long, default_value = "./config.toml", global = true, conflicts_with = "enable_nacos")]
    pub config_path: String,

    /// 从 Nacos 读取配置；兼容旧的 --enable-nacos true 写法。
    #[arg(long, global = true, action = ArgAction::Set, num_args = 0..=1, default_missing_value = "true", default_value = "false")]
    pub(crate) enable_nacos: bool,

    /// Nacos 客户端地址，格式为 host:port，不使用控制台 URL。
    #[arg(long, global = true, requires = "enable_nacos")]
    nacos_addr: Option<String>,

    /// Nacos Namespace UUID，不使用环境显示名。
    #[arg(long, global = true, requires = "enable_nacos")]
    nacos_namespace: Option<String>,

    /// Nacos 分组。
    #[arg(long, global = true, requires = "enable_nacos")]
    nacos_group: Option<String>,

    /// Nacos Data ID。
    #[arg(long, global = true, requires = "enable_nacos")]
    nacos_data_id: Option<String>,
}

impl ConfigArgs {
    /// 读取一次完整配置，供运维命令使用，不启动刷新任务。
    ///
    /// # 参数
    /// 使用当前实例中的配置源参数。
    /// # 返回
    /// 经过解析和应用校验的配置快照。
    /// # 错误
    /// 参数、凭据、远端读取或内容校验失败时返回错误，不回退到文件。
    pub async fn load(&self) -> Result<Config> {
        if self.enable_nacos {
            let client = NacosConfigClient::from_config(self.nacos_config()?).await?;
            return Config::from_toml_str(&client.fetch().await?);
        }
        if [&self.nacos_addr, &self.nacos_namespace, &self.nacos_group, &self.nacos_data_id]
            .iter()
            .any(|value| value.is_some())
        {
            return Err(Error::Invalid("Nacos 参数不能与 --enable-nacos false 混用".into()));
        }
        Config::from_file(&self.config_path).await
    }

    pub(crate) fn nacos_config(&self) -> Result<NacosConfig> {
        NacosConfig::new(
            self.nacos_addr.as_deref().unwrap_or_default(),
            self.nacos_namespace.as_deref().unwrap_or_default(),
            self.nacos_group.as_deref().unwrap_or_default(),
            self.nacos_data_id.as_deref().unwrap_or_default(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_mode_remains_default() {
        let args = ConfigArgs::try_parse_from(["api"]).unwrap();
        assert!(!args.enable_nacos);
        assert_eq!(args.config_path, "./config.toml");
    }

    #[test]
    fn nacos_flag_supports_bare_and_legacy_true() {
        for argv in [vec!["api", "--enable-nacos"], vec!["api", "--enable-nacos", "true"]] {
            let args = ConfigArgs::try_parse_from(argv).unwrap();
            assert!(args.enable_nacos);
            assert!(args.nacos_config().is_err());
        }
    }

    #[test]
    fn conflicting_sources_and_orphan_nacos_options_are_rejected() {
        assert!(ConfigArgs::try_parse_from(["api", "--enable-nacos", "--config-path", "x"]).is_err());
        assert!(ConfigArgs::try_parse_from(["api", "--nacos-namespace", "test"]).is_err());
    }

    #[test]
    fn complete_nacos_identity_is_valid() {
        let args = ConfigArgs::try_parse_from([
            "api",
            "--enable-nacos",
            "--nacos-addr",
            "127.0.0.1:8848",
            "--nacos-namespace",
            "ccf7ec38-1d60-407e-bf2c-7c4654c481d0",
            "--nacos-group",
            "DEFAULT_GROUP",
            "--nacos-data-id",
            "erp",
        ])
        .unwrap();
        assert!(args.nacos_config().is_ok());
    }

    #[tokio::test]
    async fn disabled_nacos_with_remote_options_does_not_load_file() {
        let args =
            ConfigArgs::try_parse_from(["api", "--enable-nacos", "false", "--nacos-addr", "localhost:8848"])
                .unwrap();
        assert!(matches!(args.load().await, Err(Error::Invalid(_))));
    }

    #[test]
    fn global_source_options_work_before_and_after_subcommand() {
        #[derive(Parser)]
        struct Cli {
            #[command(flatten)]
            config: ConfigArgs,
            #[command(subcommand)]
            command: Command,
        }
        #[derive(clap::Subcommand)]
        enum Command {
            Run,
        }
        for argv in [vec!["cli", "--enable-nacos", "true", "run"], vec!["cli", "run", "--enable-nacos"]] {
            assert!(Cli::try_parse_from(argv).unwrap().config.enable_nacos);
        }
    }
}
