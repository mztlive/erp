//! 连接后台任务的本域资格、健康快照与命令回执。
use super::{
    command::CommandIdentity,
    context::{digest, ensure_version},
    SupplierApiService,
};
use crate::entity::supplier_api::*;
use crate::repository::SupplierApiExt;
use crate::{Error, Result};
use persistence_core::Executor;
impl SupplierApiService {
    /// 在任务 ID 生成之前执行原连接版本与能力治理资格。
    pub async fn prepare_job_target(
        &self,
        id: &str,
        expected_version: u64,
        action: SupplierConnectionAction,
        executor: &mut dyn Executor,
    ) -> Result<(SupplierApiConnection, Vec<SupplierApiCapability>)> {
        let connection = self.load_connection(id, executor).await?;
        ensure_version(connection.base.version, expected_version)?;
        let capabilities = self
            .db
            .supplier_api()
            .connection_capabilities(&SupplierApiConnectionId::new(id), executor)
            .await?;
        let governance = SupplierConnectionGovernance {
            connection: &connection,
            capabilities: &capabilities,
            confirmations: &[],
            health_runs: &[],
        };
        if let Some(blocker) = governance.blockers(action, Default::default(), true).first() {
            return Err(Error::BusinessLogicError(blocker.message.clone()));
        }
        Ok((connection, capabilities))
    }
    /// 在原后台任务构造之后生成健康运行快照，不接完整 support job。
    pub fn prepare_health_run(
        connection: &SupplierApiConnection,
        capabilities: &[SupplierApiCapability],
        job_id: String,
        check_type: SupplierHealthCheckType,
        actor_id: &str,
        identity: &CommandIdentity,
    ) -> Result<SupplierHealthCheckRun> {
        Ok(SupplierHealthCheckRun::new(
            format!("w20-health-{}", digest(&[&job_id])),
            SupplierHealthCheckRunData {
                connection_id: SupplierApiConnectionId::new(&connection.base.id),
                background_job_id: job_id,
                check_type,
                technical_config_version: connection.technical_config_version,
                capability_versions: capabilities
                    .iter()
                    .filter(|capability| capability.status.is_active())
                    .map(|capability| CapabilityVersionSnapshot {
                        capability_code: capability.capability_code,
                        version: capability.base.version,
                    })
                    .collect(),
                requested_by: actor_id.to_string(),
                idempotency_key_hash: identity.idempotency_hash.clone(),
                request_fingerprint: identity.fingerprint.clone(),
            },
        )?)
    }
    /// 持久化健康运行事实，调用方控制与后台任务及回执的写序。
    pub async fn create_health_run(
        &self,
        run: &SupplierHealthCheckRun,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db
            .supplier_api_health_check_runs()
            .create(run, executor)
            .await?;
        Ok(())
    }
    /// 原 receipt 构造器，不构造或写入审计。
    pub fn prepare_command_receipt(
        connection: &SupplierApiConnection,
        action: SupplierConnectionAction,
        identity: &CommandIdentity,
        outcome: SupplierCommandOutcome,
        job_id: Option<String>,
        actor_id: &str,
    ) -> Result<SupplierConnectionCommandReceipt> {
        Ok(SupplierConnectionCommandReceipt::new(
            identity.receipt_id.clone(),
            SupplierConnectionCommandReceiptData {
                connection_id: SupplierApiConnectionId::new(&connection.base.id),
                action,
                actor_id: actor_id.to_string(),
                idempotency_key_hash: identity.idempotency_hash.clone(),
                request_fingerprint: identity.fingerprint.clone(),
                outcome,
                connection_version: connection.base.version,
                job_id,
                audit_event_id: identity.audit_id.clone(),
            },
        )?)
    }
    /// 在调用方事务写入命令回执，审计和 job_no 读取由外层随后执行。
    pub async fn persist_command_receipt(
        &self,
        receipt: &SupplierConnectionCommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db
            .supplier_api_command_receipts()
            .create(receipt, executor)
            .await?;
        Ok(())
    }
}
