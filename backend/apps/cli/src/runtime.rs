use config::ConfigArgs;
use erp_identity::AdminService;
use erp_processes::adapters::identity::shared_rbac_service;

use crate::error::Result;

/// 已连接到目标库的管理员服务运行时。
pub struct AdminRuntime {
    /// 管理员服务。
    pub service: AdminService,
}

impl AdminRuntime {
    /// 读取配置、连接 MongoDB 并构造管理员服务。
    ///
    /// 会校验副本集/分片事务能力。初始化超级管理员还会写入角色与 Casbin
    /// 规则，因此同时确保索引存在。
    ///
    /// # 参数
    /// * `source` - 文件或 Nacos 配置来源
    ///
    /// # 返回值
    /// 返回可调用 `AdminService` 的运行时。
    ///
    /// # 错误
    /// 配置无效、数据库不可用或不支持事务时返回错误。
    pub async fn connect(source: &ConfigArgs) -> Result<Self> {
        let config = source.load().await?;
        let (_, db) = persistence_core::connect(&config.database.uri, &config.database.db_name).await?;
        persistence_core::ensure_transaction_support(&db).await?;
        crate::indexes::ensure_indexes(&db).await?;
        let rbac = shared_rbac_service(db.clone());
        Ok(Self { service: AdminService::new(db, rbac) })
    }
}
