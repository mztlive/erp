use crate::{NacosConfigClient, SafeConfig};
use tracing::error;

pub struct NacosConfigWatcher {
    config: SafeConfig,
    nacos_client: NacosConfigClient,
}

impl NacosConfigWatcher {
    /// 创建 NacosConfigWatcher 实例。
    ///
    /// # 参数
    /// * `config` - 配置数据
    /// * `nacos_client` - Nacos 客户端
    ///
    /// # 返回
    /// 返回创建的实例。
    pub fn new(config: SafeConfig, nacos_client: NacosConfigClient) -> Self {
        Self { config, nacos_client }
    }

    /// 启动配置监听。
    ///
    /// 监听任务会持续到运行时关闭。
    pub fn start(self) {
        let config = self.config;
        let nacos_client = self.nacos_client;

        tokio::spawn(async move {
            loop {
                if let Err(e) = config.reload_from_nacos(&nacos_client).await {
                    error!("Failed to reload config from nacos: {}", e);
                }

                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            }
        });
    }
}
