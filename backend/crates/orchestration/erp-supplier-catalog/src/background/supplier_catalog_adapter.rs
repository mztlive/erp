//! 按既有明确采集授权持续调度目录任务，并恢复进程中断的待执行任务。
use std::sync::Arc;

use application_core::AuditActor;
use background_runtime::{BackgroundTaskAdapter, TaskResult};
use erp_core::common::time::Instant;
use erp_identity::AccessControlExt;
use erp_supply::dto::supplier_api::SupplierConnectionCommand;
use erp_supply::entity::supplier_api::{SupplierApiConnection, SupplierConnectionAction};
use erp_supply::repository::supplier_catalog::{self, SupplierCatalogExt};
use erp_support::repository::prelude::*;
use erp_support::{BackgroundJob, BulkJobExt, JobStatus};
use mongodb::Database;
use persistence_core::NoTransaction;
use tokio::sync::Mutex;

use super::polling_result;
use crate::supplier_connection_execution::{CatalogTaskControl, SupplierConnectionExecutionProcess};
use crate::supply_governance::supplier_api::SupplierApiGovernanceProcess;
use crate::{Error, Result};

/// 目录持续采集适配器；每进程最多两条采集，领取结果仍受数据库版本保护。
pub struct SupplierCatalogTaskAdapter {
    db: Database,
    execution: Arc<SupplierConnectionExecutionProcess>,
    governance: SupplierApiGovernanceProcess,
    control: CatalogTaskControl,
    cursor: Mutex<String>,
    schedule_cursor: Mutex<String>,
}

impl SupplierCatalogTaskAdapter {
    /// 复用应用已经装配的技术网关、授权与媒体准备器。
    ///
    /// 数据库、执行流程、治理流程及应用共享的停止控制器。
    ///
    /// 可登记到统一后台执行器的适配器。
    ///
    /// 周期由配置边界验证。
    pub fn new(
        db: Database,
        execution: SupplierConnectionExecutionProcess,
        governance: SupplierApiGovernanceProcess,
        control: CatalogTaskControl,
    ) -> Self {
        Self {
            db,
            execution: Arc::new(execution.with_control(control.clone())),
            governance,
            control,
            cursor: Mutex::new(String::new()),
            schedule_cursor: Mutex::new(String::new()),
        }
    }

    /// 扫描一页连接并持久化到期任务，不启动目录采集。
    ///
    /// 本页已生成待执行任务的连接数。
    ///
    /// 任一连接无法确认调度结果时返回错误；已保存任务不回滚或重放。
    pub async fn schedule_due(&self) -> Result<usize> {
        let mut cursor = self.schedule_cursor.lock().await;
        let kinds = self.execution.catalog_connector_types();
        let connections =
            supplier_catalog::connection_page(&self.db, &cursor, &kinds, &mut NoTransaction).await?;
        if connections.is_empty() {
            cursor.clear();
            return Ok(0);
        }

        let mut scheduled = 0;
        let mut failed = false;
        for connection in connections {
            *cursor = connection.base.id.clone();
            match self.schedule_connection(&connection).await {
                Ok(Some(_)) => scheduled += 1,
                Ok(None) => {},
                Err(_) => {
                    failed = true;
                    tracing::warn!(connection_id=%connection.base.id, "目录计划生成失败，请核对连接原记录");
                },
            }
        }

        if failed {
            return Err(Error::BusinessLogicError("本页部分目录计划未确认，请核对原连接任务".into()));
        }
        Ok(scheduled)
    }

    async fn ready_job(&self, connection: &SupplierApiConnection) -> Result<Option<(String, AuditActor)>> {
        let Some(_) = self
            .execution
            .scheduled_policy(connection)
            .map_err(|error| Error::BusinessLogicError(error.summary))?
        else {
            return Ok(None);
        };

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
        let handoff = if latest.status == JobStatus::Failed {
            self.db
                .supplier_catalog_runs()
                .find_by_id(&latest.base.id, &mut NoTransaction)
                .await?
                .is_some_and(|run| run.resume_requested)
        } else {
            false
        };
        if active.is_empty() && !handoff {
            return Ok(None);
        }

        self.create_job(connection, latest).await
    }

    // XXL 只生成新的到期任务，旧任务恢复交给持续消费循环。
    async fn schedule_connection(
        &self,
        connection: &SupplierApiConnection,
    ) -> Result<Option<(String, AuditActor)>> {
        let Some(policy) = self
            .execution
            .scheduled_policy(connection)
            .map_err(|error| Error::BusinessLogicError(error.summary))?
        else {
            return Ok(None);
        };

        if !self
            .db
            .background_jobs()
            .active_catalog_jobs(&connection.base.id, &mut NoTransaction)
            .await?
            .is_empty()
        {
            return Ok(None);
        }

        let history =
            self.db.background_jobs().recent_catalog_jobs(&connection.base.id, &mut NoTransaction).await?;
        let Some(latest) = history.first() else { return Ok(None) };
        if !latest.catalog_schedule_due(Instant::now().unix_secs(), policy.sync_interval_seconds) {
            return Ok(None);
        }

        self.create_job(connection, latest).await
    }

    // 周期生成与恢复均沿原任务身份执行治理命令。
    async fn create_job(
        &self,
        connection: &SupplierApiConnection,
        latest: &BackgroundJob,
    ) -> Result<Option<(String, AuditActor)>> {
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

    async fn drain(&self) {
        if !self.control.drain(std::time::Duration::from_secs(60)).await {
            tracing::warn!("目录任务排空超时，保留原检查点恢复");
        }
    }

    async fn run_due(&self) -> TaskResult<usize> {
        if self.control.stopping() {
            return Ok(0);
        }

        let mut cursor = self.cursor.lock().await;
        let kinds = self.execution.catalog_connector_types();
        let connections = polling_result(
            supplier_catalog::connection_page(&self.db, &cursor, &kinds, &mut NoTransaction).await,
        )?;
        if connections.is_empty() {
            cursor.clear();
            return Ok(0);
        }

        let mut claimed = 0;
        for connection in connections {
            let Some(permit) = self.control.claim() else { break };
            *cursor = connection.base.id.clone();
            match self.ready_job(&connection).await {
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
