use tracing::error;

use crate::{NacosConfigClient, SafeConfig};

pub struct NacosConfigWatcher {
    config: SafeConfig,
    nacos_client: NacosConfigClient,
}

impl NacosConfigWatcher {
    /// 首次 Nacos 配置校验成功后，把当前快照和读取它的远端客户端交给刷新任务；构造时不启动轮询。
    pub fn new(config: SafeConfig, nacos_client: NacosConfigClient) -> Self {
        Self { config, nacos_client }
    }

    /// 启动 Nacos 后台轮询，每次刷新尝试结束后等待 10 秒，直到 Tokio 运行时关闭。
    /// 读取或校验失败只记录错误，已有快照继续有效。
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
