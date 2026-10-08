//! 供应商连接后台任务执行：启动事务、事务外网关调用和结果事务。

use std::sync::Arc;

use application_core::AuditActor;
use erp_supply::ports::supplier_api_gateway::SupplierApiGateway;
use erp_support::{BulkJobExt, SUPPLIER_CATALOG_SYNC_JOB_TYPE, SUPPLIER_HEALTH_CHECK_JOB_TYPE};
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::{Error, Result};

mod catalog;
mod execution;
mod failure;
mod health;

/// 消费组合根已注入的供应商网关，执行已登记的连接任务。
pub struct SupplierConnectionExecutionProcess {
    db: Database,
    gateway: Arc<dyn SupplierApiGateway>,
}

impl SupplierConnectionExecutionProcess {
    /// 复用应用数据库与已注入网关，不另建外部连接器或读取凭据。
    ///
    /// # 参数
    /// * `db` - 应用数据库。
    /// * `gateway` - 组合根注入的供应商网关。
    ///
    /// # 返回
    /// 返回连接任务执行流程。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, gateway: Arc<dyn SupplierApiGateway>) -> Self {
        Self { db, gateway }
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
            _ => Err(Error::BusinessLogicError("任务不属于 W20 连接治理".to_string())),
        }
    }
}
