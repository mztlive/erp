//! 供应商连接后台任务执行：启动事务、事务外网关调用和结果事务。

use application_core::AuditActor;
use database::SupplierApiExt;
use erp_support::{SUPPLIER_CATALOG_SYNC_JOB_TYPE, SUPPLIER_HEALTH_CHECK_JOB_TYPE};
use mongodb::Database;
use persistence_core::NoTransaction;
use services::supplier_api::SupplierApiGateway;
use services::{Error, Result};
use std::sync::Arc;

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
    /// 复用应用数据库与网关；不另建外部连接器或读取凭据。
    pub fn new(db: Database, gateway: Arc<dyn SupplierApiGateway>) -> Self {
        Self { db, gateway }
    }

    /// 执行已登记的连接后台任务。
    ///
    /// 该入口供 Web 进程后台调度器调用，绝不能在创建任务的 HTTP 请求内等待。
    /// 默认未注入真实 adapter 时任务形成明确失败终态并进入 W29。
    ///
    /// # Errors
    /// 任务不存在、状态冲突或任务结果落库失败时返回错误。
    pub async fn process_connection_job(&self, job_id: &str, actor: &AuditActor) -> Result<()> {
        let job = self
            .db
            .supplier_api()
            .governance_job(job_id, &mut NoTransaction)
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
