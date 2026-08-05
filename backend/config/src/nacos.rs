use nacos_sdk::api::{
    config::{ConfigService, ConfigServiceBuilder},
    props::ClientProps,
};

#[derive(Clone)]
pub struct NacosConfig {
    pub addr: String,
    pub namespace: String,
    pub group: String,
    pub data_id: String,
}

/// Nacos 配置客户端
#[derive(Clone)]
pub struct NacosConfigClient {
    config: NacosConfig,
    nacos_cs: ConfigService,
}

impl NacosConfigClient {
    /// 创建 NacosConfigClient 实例。
    ///
    /// # 参数
    /// * `addr` - 服务地址
    /// * `namespace` - 命名空间
    /// * `group` - 分组名称
    /// * `data_id` - 配置标识
    ///
    /// # 返回
    /// 返回实例构建结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当验证失败或底层操作失败时返回错误。
    pub async fn new(addr: &str, namespace: &str, group: &str, data_id: &str) -> crate::Result<Self> {
        let addr = addr.trim_end_matches('/').to_string();

        let config_service =
            ConfigServiceBuilder::new(ClientProps::new().server_addr(&addr).namespace(namespace))
                .build()
                .await?;

        Ok(Self {
            config: NacosConfig {
                addr,
                namespace: namespace.to_string(),
                group: group.to_string(),
                data_id: data_id.to_string(),
            },
            nacos_cs: config_service,
        })
    }

    /// 从`config`构建实例。
    ///
    /// # 参数
    /// * `config` - 配置数据
    ///
    /// # 返回
    /// 返回实例构建结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当验证失败或底层操作失败时返回错误。
    pub async fn from_config(config: NacosConfig) -> crate::Result<Self> {
        Self::new(&config.addr, &config.namespace, &config.group, &config.data_id).await
    }

    /// 从 Nacos 获取当前配置内容。
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当验证失败或底层操作失败时返回错误。
    pub async fn fetch(&self) -> crate::Result<String> {
        let config = self
            .nacos_cs
            .get_config(self.config.data_id.clone(), self.config.group.clone())
            .await?;

        Ok(config.content().clone())
    }
}
