use clap::Parser;

use crate::nacos::NacosConfig;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// 配置文件路径
    #[arg(short, long, default_value = "./config.toml")]
    pub config_path: String,

    /// 是否启用 Nacos
    #[arg(long)]
    enable_nacos: Option<bool>,

    /// Nacos 服务器地址
    #[arg(long)]
    nacos_addr: Option<String>,

    /// Nacos 命名空间
    #[arg(long)]
    nacos_namespace: Option<String>,

    /// Nacos 数据组
    #[arg(long)]
    nacos_group: Option<String>,

    /// Nacos 数据ID
    #[arg(long)]
    nacos_data_id: Option<String>,
}

impl Args {
    /// 判断是否为 `enable_nacos`。
    ///
    /// # 返回
    /// 返回布尔值表示条件是否满足或操作是否成功。
    pub fn is_enable_nacos(&self) -> bool {
        self.enable_nacos.unwrap_or(false)
    }

    /// 转换为`nacos_config`表示。
    ///
    /// # 返回
    /// 返回 `NacosConfig` 实例。
    pub fn to_nacos_config(&self) -> NacosConfig {
        NacosConfig {
            addr: self.nacos_addr.clone().unwrap_or_default(),
            namespace: self.nacos_namespace.clone().unwrap_or_default(),
            group: self.nacos_group.clone().unwrap_or_default(),
            data_id: self.nacos_data_id.clone().unwrap_or_default(),
        }
    }
}
