use config::ConfigArgs;
use erp_identity::AdminService;
use erp_supplier_orders::supplier_credentials::JdVopCredentialProcess;

use crate::adapters::shared_rbac_service;
use crate::error::{Error, Result};

/// 只装配配置绑定凭据用例；不启动供应商调用、刷新或业务 worker。
pub struct JdVopCredentialRuntime {
    pub process: JdVopCredentialProcess,
}

impl JdVopCredentialRuntime {
    /// 京东凭据导入或查询前，读取一次部署配置，校验凭据设置并连接目标数据库，
    /// 按凭据 ID、账号范围和 app key 装配用例。即使业务对接已停用，也不放宽凭据校验，
    /// 且不启动远端调用或刷新任务。
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
    /// 为初始化和重置密码命令装配目标库的管理员服务。先确认 MongoDB 支持事务（副本集或
    /// 分片集群），再确保索引就绪；初始化可能同时写入账号、角色和 Casbin 规则。
    pub async fn connect(source: &ConfigArgs) -> Result<Self> {
        let config = source.load().await?;
        let (_, db) = persistence_core::connect(&config.database.uri, &config.database.db_name).await?;
        persistence_core::ensure_transaction_support(&db).await?;
        crate::indexes::ensure_indexes(&db).await?;
        let rbac = shared_rbac_service(db.clone());
        Ok(Self { service: AdminService::new(db, rbac) })
    }
}
