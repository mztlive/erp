//! 连接后台任务的本域资格、健康快照与命令回执。
use persistence_core::Executor;

use super::SupplierApiService;
use super::command::CommandIdentity;
use super::context::{digest, ensure_version};
use crate::entity::supplier_api::*;
use crate::repository::SupplierApiExt;
use crate::{Error, Result};
impl SupplierApiService {
    /// 在任务 ID 生成之前执行原连接版本与能力治理资格。
    /// 返回通过资格校验的连接及其能力列表。
    /// 连接不存在、版本不一致或能力读取失败时返回对应错误；治理规则给出阻塞时返回 `BusinessLogicError`。
    pub async fn prepare_job_target(
        &self,
        id: &str,
        expected_version: u64,
        action: SupplierConnectionAction,
        executor: &mut dyn Executor,
    ) -> Result<(SupplierApiConnection, Vec<SupplierApiCapability>)> {
        let connection = self.load_connection(id, executor).await?;
        ensure_version(connection.base.version, expected_version)?;

        let capabilities = Vec::new();
        let governance = SupplierConnectionGovernance {
            connection: &connection,
            capabilities: &capabilities,
            confirmations: &[],
            health_runs: &[],
        };
        if let Some(blocker) = governance.blockers(action, Default::default(), true).into_iter().next() {
            return Err(Error::BusinessLogicError(blocker.message));
        }
        Ok((connection, capabilities))
    }
    /// 在原后台任务构造之后生成健康运行快照，不接完整 support job。
    /// 返回尚未持久化的健康运行记录。
    /// 健康运行实体构造失败时返回对应错误。
    /// `capabilities`：仅启用态进入版本快照。
    /// `identity`：命令幂等摘要与请求指纹。
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
    /// 健康运行事实已写入。
    /// 仓储写入失败时向上传播错误。
    pub async fn create_health_run(
        &self,
        run: &SupplierHealthCheckRun,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.supplier_api_health_check_runs().create(run, executor).await?;
        Ok(())
    }
    /// 将连接版本、动作结果、命令指纹及审计身份封装为待写回执，供提交结果恢复时查证。仅有关联后台任务时
    /// 保存任务 ID；实体校验失败不返回回执，此处不写审计。
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
    /// 回执已写入。
    /// 仓储写入失败时向上传播错误。
    pub async fn persist_command_receipt(
        &self,
        receipt: &SupplierConnectionCommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.supplier_api_command_receipts().create(receipt, executor).await?;
        Ok(())
    }
}
