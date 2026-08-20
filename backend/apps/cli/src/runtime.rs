use config::Config;
use services::iam::{shared_rbac_service, AdminService};

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
    /// * `config_path` - TOML 配置文件路径
    ///
    /// # 返回值
    /// 返回可调用 `AdminService` 的运行时。
    ///
    /// # 错误
    /// 配置无效、数据库不可用或不支持事务时返回错误。
    pub async fn connect(config_path: &str) -> Result<Self> {
        let config = Config::from_file(config_path).await?;
        let (_, db) = database::connect(&config.database.uri, &config.database.db_name).await?;
        database::ensure_transaction_support(&db).await?;
        database::ensure_indexes(&db).await?;
        let rbac = shared_rbac_service(db.clone());
        Ok(Self {
            service: AdminService::new(db, rbac),
        })
    }
}
