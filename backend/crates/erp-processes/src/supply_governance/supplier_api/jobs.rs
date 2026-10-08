use application_core::AuditActor;
use erp_core::ids::BackgroundJobId;
use erp_supply::dto::supplier_api::SupplierConnectionCommandResult;
use erp_supply::entity::supplier_api::{
    SupplierCommandOutcome, SupplierConnectionAction, SupplierHealthCheckType,
};
use erp_supply::service::supplier_api::SupplierApiService;
use erp_supply::service::supplier_api::command::CommandIdentity;
use erp_support::{BackgroundJob, BulkJobExt, SupplierGovernanceJobKind, SupplierGovernanceJobSpec};
use id_generator::next_id;
use persistence_core::Transactional;

use super::SupplierApiGovernanceProcess;
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
        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        let connection_id_value = id.to_string();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let domain = SupplierApiService::new(db.clone());
                    let (connection, capabilities) = domain
                        .prepare_job_target(
                            &connection_id_value,
                            expected_version,
                            SupplierConnectionAction::RunHealthCheck,
                            executor,
                        )
                        .await?;
                    let job = BackgroundJob::for_supplier_governance(SupplierGovernanceJobSpec {
                        job_id: BackgroundJobId::new(next_id()),
                        connection_id: connection_id_value.clone(),
                        kind: SupplierGovernanceJobKind::HealthCheck,
                        requested_by: actor.id().to_string(),
                        idempotency_hash: identity.idempotency_hash.clone(),
                    })?;
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
                    persist_command_receipt(
                        &db,
                        CommandReceiptWrite {
                            connection: &connection,
                            action: SupplierConnectionAction::RunHealthCheck,
                            identity: &identity,
                            outcome: SupplierCommandOutcome::Processing,
                            job_id: Some(job.base.id.clone()),
                            actor: &actor,
                        },
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
        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        let connection_id_value = id.to_string();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let domain = SupplierApiService::new(db.clone());
                    let (connection, _capabilities) = domain
                        .prepare_job_target(
                            &connection_id_value,
                            expected_version,
                            SupplierConnectionAction::StartCatalogSync,
                            executor,
                        )
                        .await?;
                    let job = BackgroundJob::for_supplier_governance(SupplierGovernanceJobSpec {
                        job_id: BackgroundJobId::new(next_id()),
                        connection_id: connection_id_value.clone(),
                        kind: SupplierGovernanceJobKind::CatalogSync,
                        requested_by: actor.id().to_string(),
                        idempotency_hash: identity.idempotency_hash.clone(),
                    })?;
                    db.background_jobs().create(&job, executor).await?;
                    persist_command_receipt(
                        &db,
                        CommandReceiptWrite {
                            connection: &connection,
                            action: SupplierConnectionAction::StartCatalogSync,
                            identity: &identity,
                            outcome: SupplierCommandOutcome::Processing,
                            job_id: Some(job.base.id.clone()),
                            actor: &actor,
                        },
                        executor,
                    )
                    .await
                })
            })
            .await
    }
}
