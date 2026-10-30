use config::ConfigArgs;
use erp_identity::AdminService;
use erp_processes::adapters::identity::shared_rbac_service;
use erp_processes::connectors::jd_vop::credential::JdVopCredentialProcess;

use crate::error::{Error, Result};

/// 只装配配置绑定凭据用例；不启动供应商调用、刷新或业务 worker。
pub struct JdVopCredentialRuntime {
    pub process: JdVopCredentialProcess,
}

impl JdVopCredentialRuntime {
    /// 校验停用模式下也必需的凭据配置，并连接目标数据库。
    /// # 参数
    /// `source` 为本地或 Nacos 的统一配置来源。
    /// # 返回
    /// 精确绑定凭据 ID、账号范围及 app key 的用例。
    /// # 错误
    /// 配置缺失、绑定设置无效、数据库连接或用例装配失败时返回错误。
    pub async fn connect(source: &ConfigArgs) -> Result<Self> {
        let config = source.load().await?;
        let settings =
            config.jd_vop.as_ref().ok_or_else(|| Error::Usage("配置缺少 jd_vop 凭据设置".into()))?;
        settings.validate_credential_settings()?;
        let (_, db) = persistence_core::connect(&config.database.uri, &config.database.db_name).await?;
        Ok(Self { process: JdVopCredentialProcess::new(db, settings)? })
    }
}

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
