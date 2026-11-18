//! 供应商连接后台任务执行：启动事务、事务外网关调用和结果事务。

use std::sync::Arc;

use application_core::AuditActor;
use erp_catalog::CatalogService;
use erp_identity::AuthorizationService;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::entity::supplier_api::{SupplierApiConnection, SupplierConnectorType};
use erp_supply::ports::catalog_sync::CatalogSyncPolicy;
use erp_supply::ports::supplier_api_gateway::{ClassifiedError, SupplierApiGateway};
use erp_support::{BulkJobExt, SUPPLIER_CATALOG_SYNC_JOB_TYPE, SUPPLIER_HEALTH_CHECK_JOB_TYPE};
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::source_media::SourceMediaPreparer;
use crate::{Error, Result};

mod message_writer;
pub use message_writer::CatalogMessageWriter;
mod control;
pub use control::CatalogTaskControl;
mod catalog;
mod catalog_membership;
mod catalog_scan;
pub(crate) use catalog_scan::save_observation as save_catalog_observation;
mod catalog_supply;
pub(crate) use catalog_supply::apply_source_supply;
mod execution;
mod failure;
mod health;
mod refresh;
pub(crate) use refresh::{catalog_refresh_actor, guard_catalog_refresh};

/// 消费组合根已注入的供应商网关，执行已登记的连接任务。
pub struct SupplierConnectionExecutionProcess {
    authorization: Arc<AuthorizationService>,
    catalog: CatalogService,
    db: Database,
    gateway: Arc<dyn SupplierApiGateway>,
    source_media: Option<Arc<SourceMediaPreparer>>,
    control: CatalogTaskControl,
}

impl SupplierConnectionExecutionProcess {
    /// 复用应用数据库与已注入网关，不另建外部连接器或读取凭据。
    ///
    /// # 参数
    /// * `db` - 应用数据库。
    /// * `gateway` - 组合根注入的供应商网关。
    /// * `catalog` - 已装配审计与文件资产 Port 的商品服务。
    /// * `authorization` - 应用共享的统一授权服务。
    ///
    /// # 返回
    /// 返回连接任务执行流程。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(
        db: Database,
        gateway: Arc<dyn SupplierApiGateway>,
        catalog: CatalogService,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        Self {
            db,
            gateway,
            catalog,
            authorization,
            source_media: None,
            control: CatalogTaskControl::default(),
        }
    }

    /// 绑定应用退出时使用的目录任务控制器。
    /// # 参数
    /// `control` 为同进程调度器与关闭流程共享的控制器。
    /// # 返回
    /// 使用共享停止状态的执行流程。
    /// # 错误
    /// 不返回错误。
    pub fn with_control(mut self, control: CatalogTaskControl) -> Self {
        self.control = control;
        self
    }

    /// 读取已登记的目录候选协议。
    pub(crate) fn catalog_connector_types(&self) -> Vec<SupplierConnectorType> {
        self.gateway.catalog_connector_types()
    }

    /// 从适配器取得当前连接策略，缺少目录能力时拒绝执行。
    pub(crate) fn catalog_policy(
        &self,
        connection: &SupplierApiConnection,
    ) -> std::result::Result<CatalogSyncPolicy, ClassifiedError> {
        self.gateway.catalog_policy(connection)?.ok_or_else(|| ClassifiedError {
            class: SupplierFailureClass::CapabilityGap,
            code: "CATALOG_SYNC_ADAPTER_UNAVAILABLE".into(),
            summary: "当前供应商尚未开通商品同步，请联系管理员核对连接功能".into(),
        })
    }

    /// 读取调度策略；无目录能力的候选连接可被跳过。
    pub(crate) fn scheduled_policy(
        &self,
        connection: &SupplierApiConnection,
    ) -> std::result::Result<Option<CatalogSyncPolicy>, ClassifiedError> {
        self.gateway.catalog_policy(connection)
    }

    /// 注入事务外来源图片准备器，不改变网关或业务启用状态。
    /// # 参数
    /// `source_media` 使用部署登记的受信图片主机、对象存储及内容密钥。
    /// # 返回
    /// 带媒体准备能力的连接执行流程。
    /// # 错误
    /// 不返回错误。
    pub fn with_source_media(mut self, source_media: Option<Arc<SourceMediaPreparer>>) -> Self {
        self.source_media = source_media;
        self
    }

    /// 按任务类型执行已登记的连接后台任务；已经终态则直接返回。
    ///
    /// 供后台调度器调用，不得在创建任务的 HTTP 请求内等待。
    ///
    /// # 参数
    /// * `job_id` - 后台任务主键。
    /// * `actor` - 结果事务使用的审计操作人。
    ///
    /// # 返回
    /// 任务已终态，或健康检查、目录同步执行完成时返回。
    ///
    /// # 错误
    /// 任务不存在、任务类型不属于连接治理，或对应执行的启动与结果事务失败时返回错误。
    pub async fn process_connection_job(&self, job_id: &str, actor: &AuditActor) -> Result<()> {
        let job = self
            .db
            .background_jobs()
            .find_by_id(job_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("连接后台任务不存在".to_string()))?;
        if job.is_terminal() {
            return Ok(());
        }

        match job.domain_job_type.as_deref() {
            Some(SUPPLIER_HEALTH_CHECK_JOB_TYPE) => self.process_health_job(job, actor).await,
            Some(SUPPLIER_CATALOG_SYNC_JOB_TYPE) => self.process_catalog_job(job, actor).await,
            _ => Err(Error::BusinessLogicError(
                "此任务不是供应商连接检查或商品同步任务，请从对应连接重新选择任务".to_string(),
            )),
        }
    }
}
