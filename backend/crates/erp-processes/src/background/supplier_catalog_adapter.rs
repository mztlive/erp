//! 按既有明确采集授权持续调度目录任务，并恢复进程中断的待执行任务。
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::AccessControlExt;
use erp_supply::dto::supplier_api::SupplierConnectionCommand;
use erp_supply::entity::supplier_api::{SupplierApiConnection, SupplierConnectionAction};
use erp_supply::repository::supplier_catalog;
use erp_support::repository::prelude::*;
use erp_support::{BackgroundJob, BulkJobExt, JobStatus};
use mongodb::Database;
use persistence_core::NoTransaction;
use tokio::sync::{Mutex, Semaphore};

use super::BackgroundTaskAdapter;
use crate::supplier_connection_execution::SupplierConnectionExecutionProcess;
use crate::supply_governance::supplier_api::SupplierApiGovernanceProcess;
use crate::{Error, Result};

/// 目录持续采集适配器；每进程最多两条采集，领取结果仍受数据库版本保护。
pub struct SupplierCatalogTaskAdapter {
    db: Database,
    execution: Arc<SupplierConnectionExecutionProcess>,
    governance: SupplierApiGovernanceProcess,
    interval_seconds: u64,
    cursor: Mutex<String>,
    permits: Arc<Semaphore>,
}

impl SupplierCatalogTaskAdapter {
    /// 复用应用已经装配的技术网关、授权与媒体准备器。
    /// # 参数
    /// 数据库、执行流程、治理流程及明确部署的采集周期秒数。
    /// # 返回
    /// 可登记到统一后台执行器的适配器。
    /// # 错误
    /// 不返回错误；周期由配置边界验证。
    pub fn new(
        db: Database,
        execution: SupplierConnectionExecutionProcess,
        governance: SupplierApiGovernanceProcess,
        interval_seconds: u64,
    ) -> Self {
        Self {
            db,
            execution: Arc::new(execution),
            governance,
            interval_seconds,
            cursor: Mutex::new(String::new()),
            permits: Arc::new(Semaphore::new(2)),
        }
    }

    async fn scheduled_job(
        &self,
        connection: &SupplierApiConnection,
    ) -> Result<Option<(String, AuditActor)>> {
        let active =
            self.db.background_jobs().active_catalog_jobs(&connection.base.id, &mut NoTransaction).await?;
        if let Some(pending) =
            active.iter().find(|job| job.status == JobStatus::Pending && !job.catalog_stalled(Instant::now()))
        {
            let actor = self.actor(pending).await?;
            return Ok(Some((pending.base.id.clone(), actor)));
        }
        if active.iter().any(|job| !job.catalog_stalled(Instant::now())) {
            return Ok(None);
        }
        let history =
            self.db.background_jobs().recent_catalog_jobs(&connection.base.id, &mut NoTransaction).await?;
        let Some(latest) = history.first() else { return Ok(None) };
        if active.is_empty()
            && !is_due(latest.base.created_at, Instant::now().unix_secs(), self.interval_seconds)
        {
            return Ok(None);
        }
        let actor = self.actor(latest).await?;
        let command = SupplierConnectionCommand {
            action: SupplierConnectionAction::StartCatalogSync,
            expected_version: connection.base.version,
            payload_reference: None,
            reason_code: None,
            check_type: None,
            idempotency_key: format!("catalog-auto-{}-{}", connection.base.id, latest.base.id),
        };
        let result = self.governance.execute_connection_command(&connection.base.id, command, &actor).await?;
        Ok(result.job_id.map(|id| (id, actor)))
    }

    async fn actor(&self, job: &BackgroundJob) -> Result<AuditActor> {
        let account = self
            .db
            .accounts()
            .find_by_id(&job.requested_by, &mut NoTransaction)
            .await?
            .filter(|account| account.is_active_backoffice())
            .ok_or_else(|| Error::Forbidden("商品自动同步授权人已失效，请重新授权同步".into()))?;
        Ok(AuditActor::new(account.base.id.clone(), account.secret.account().into(), account.kind)
            .with_actor_name_snapshot(Some(account.name))?)
    }
}

#[async_trait::async_trait]
impl BackgroundTaskAdapter for SupplierCatalogTaskAdapter {
    fn name(&self) -> &'static str {
        "supplier_catalog_sync"
    }

    async fn run_due(&self) -> Result<usize> {
        let mut cursor = self.cursor.lock().await;
        let connections = supplier_catalog::connection_page(&self.db, &cursor, &mut NoTransaction).await?;
        if connections.is_empty() {
            cursor.clear();
            return Ok(0);
        }
        let mut claimed = 0;
        for connection in connections {
            let Ok(permit) = Arc::clone(&self.permits).try_acquire_owned() else { break };
            *cursor = connection.base.id.clone();
            match self.scheduled_job(&connection).await {
                Ok(Some((id, actor))) => {
                    let process = Arc::clone(&self.execution);
                    tokio::spawn(async move {
                        let _permit = permit;
                        if let Err(error) = process.process_connection_job(&id, &actor).await {
                            tracing::warn!(job_id = %id, account = %actor.id(), error = %error, "supplier catalog execution interrupted");
                        }
                    });
                    claimed += 1;
                },
                Ok(None) => {},
                Err(error) => {
                    tracing::warn!(connection_id = %connection.base.id, error = %error, "supplier catalog schedule rejected")
                },
            }
        }
        Ok(claimed)
    }
}

fn is_due(created_at: u64, now: i64, interval: u64) -> bool {
    u64::try_from(now).is_ok_and(|now| now.saturating_sub(created_at) >= interval)
}

#[cfg(test)]
mod tests {
    use super::is_due;
    #[test]
    fn scheduled_reads_respect_interval_without_underflow_or_future_jobs() {
        assert!(!is_due(100, 109, 10));
        assert!(is_due(100, 110, 10));
        assert!(!is_due(100, 99, 10));
        assert!(!is_due(0, -1, 10));
    }
}
