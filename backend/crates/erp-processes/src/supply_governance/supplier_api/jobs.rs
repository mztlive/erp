use application_core::AuditActor;
use erp_core::ids::BackgroundJobId;
use erp_supply::dto::supplier_api::SupplierConnectionCommandResult;
use erp_supply::entity::supplier_api::{
    SupplierApiConnection, SupplierCommandOutcome, SupplierConnectionAction, SupplierHealthCheckType,
};
use erp_supply::repository::supplier_catalog::SupplierCatalogExt;
use erp_supply::service::supplier_api::SupplierApiService;
use erp_supply::service::supplier_api::command::CommandIdentity;
use erp_support::{BackgroundJob, BulkJobExt, SupplierGovernanceJobKind, SupplierGovernanceJobSpec};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::SupplierApiGovernanceProcess;
use super::catalog_recovery::{clear_stalled, next_run};
use super::receipt::{CommandReceiptWrite, persist_command_receipt};
use crate::Result;
impl SupplierApiGovernanceProcess {
    /// 在一个事务内登记健康检查后台任务、运行记录和命令回执。
    ///
    /// # 参数
    /// * `id` - 供应连接 ID。
    /// * `check_type` - 健康检查类型。
    /// * `expected_version` - 调用方冻结的连接版本。
    /// * `identity` - 已计算的命令身份。
    /// * `actor` - 当前操作人。
    ///
    /// # 返回
    /// 结果为处理中的连接命令回执。
    ///
    /// # 错误
    /// 目标版本不匹配、任务或健康检查运行构造失败，或任务、运行与回执写入失败时返回对应错误。
    pub(super) async fn create_health_job(
        &self,
        id: &str,
        check_type: SupplierHealthCheckType,
        expected_version: u64,
        identity: CommandIdentity,
        actor: &AuditActor,
    ) -> Result<SupplierConnectionCommandResult> {
        let deployment = self.deployment(id).await?;
        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        let connection_id_value = id.to_string();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let domain = SupplierApiService::new(db.clone());
                    let (mut connection, capabilities) = domain
                        .prepare_job_target(
                            &connection_id_value,
                            expected_version,
                            SupplierConnectionAction::RunHealthCheck,
                            executor,
                        )
                        .await?;
                    if deployment.apply(&mut connection, actor.id())? {
                        domain.persist_connection(&mut connection, executor).await?;
                    }
                    let job = background_job(
                        &connection_id_value,
                        SupplierGovernanceJobKind::HealthCheck,
                        &actor,
                        &identity,
                    )?;
                    let run = SupplierApiService::prepare_health_run(
                        &connection,
                        &capabilities,
                        job.base.id.clone(),
                        check_type,
                        actor.id(),
                        &identity,
                    )?;
                    db.background_jobs().create(&job, executor).await?;
                    domain.create_health_run(&run, executor).await?;
                    processing_receipt(
                        &db,
                        &connection,
                        SupplierConnectionAction::RunHealthCheck,
                        identity,
                        &job,
                        &actor,
                        executor,
                    )
                    .await
                })
            })
            .await
    }
    /// 在一个事务内登记目录同步后台任务和命令回执。
    ///
    /// # 参数
    /// * `id` - 供应连接 ID。
    /// * `expected_version` - 调用方冻结的连接版本。
    /// * `identity` - 已计算的命令身份。
    /// * `actor` - 当前操作人。
    ///
    /// # 返回
    /// 结果为处理中的连接命令回执。
    ///
    /// # 错误
    /// 目标版本不匹配、目录同步任务构造失败，或任务与回执写入失败时返回对应错误。
    pub(super) async fn create_catalog_job(
        &self,
        id: &str,
        expected_version: u64,
        identity: CommandIdentity,
        actor: &AuditActor,
    ) -> Result<SupplierConnectionCommandResult> {
        let deployment = self.deployment(id).await?;
        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        let connection_id_value = id.to_string();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let domain = SupplierApiService::new(db.clone());
                    let (mut connection, _capabilities) = domain
                        .prepare_job_target(
                            &connection_id_value,
                            expected_version,
                            SupplierConnectionAction::StartCatalogSync,
                            executor,
                        )
                        .await?;
                    clear_stalled(&db, &connection.base.id, executor).await?;
                    deployment.apply(&mut connection, actor.id())?;
                    connection.stable.touch(actor.id());
                    domain.persist_connection(&mut connection, executor).await?;
                    let job = background_job(
                        &connection_id_value,
                        SupplierGovernanceJobKind::CatalogSync,
                        &actor,
                        &identity,
                    )?;
                    let run = next_run(&db, &job.base.id, &connection, executor).await?;
                    db.supplier_catalog_runs().create(&run, executor).await?;
                    db.background_jobs().create(&job, executor).await?;
                    processing_receipt(
                        &db,
                        &connection,
                        SupplierConnectionAction::StartCatalogSync,
                        identity,
                        &job,
                        &actor,
                        executor,
                    )
                    .await
                })
            })
            .await
    }
}

/// 复用连接任务身份生成，任务和回执由调用方统一保存。
fn background_job(
    id: &str,
    kind: SupplierGovernanceJobKind,
    actor: &AuditActor,
    identity: &CommandIdentity,
) -> Result<BackgroundJob> {
    Ok(BackgroundJob::for_supplier_governance(SupplierGovernanceJobSpec {
        job_id: BackgroundJobId::new(next_id()),
        connection_id: id.into(),
        kind,
        requested_by: actor.id().to_string(),
        idempotency_hash: identity.idempotency_hash.clone(),
    })?)
}

/// 保存任务受理回执，沿用任务事务并保留命令幂等身份。
async fn processing_receipt(
    db: &Database,
    connection: &SupplierApiConnection,
    action: SupplierConnectionAction,
    identity: CommandIdentity,
    job: &BackgroundJob,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<SupplierConnectionCommandResult> {
    persist_command_receipt(
        db,
        CommandReceiptWrite {
            connection,
            action,
            identity: &identity,
            outcome: SupplierCommandOutcome::Processing,
            job_id: Some(job.base.id.clone()),
            actor,
        },
        executor,
    )
    .await
}
