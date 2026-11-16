use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::AccessTarget;
use erp_supply::dto::supplier_api::{SupplierConnectionCommand, SupplierConnectionCommandResult};
use erp_supply::entity::supplier_api::{
    HealthCheckResult, PreparedSupplierConnectionCommand, SupplierCommandOutcome, SupplierConnectionAction,
};
use erp_supply::service::supplier_api::SupplierApiService;
use erp_supply::service::supplier_api::command::CommandIdentity;
use erp_supply::service::supplier_api::context::{action_permission, map_command_shape_rejection};
use persistence_core::Transactional;
use validator::Validate;

use super::SupplierApiGovernanceProcess;
use super::receipt::{CommandReceiptWrite, persist_command_receipt};
use crate::Result;

mod replay;
impl SupplierApiGovernanceProcess {
    /// 执行固定连接治理命令并返回可幂等重放的正式回执。
    ///
    /// 手动健康检查与目录同步登记后台任务；开启自动下单时，
    /// 在事务外完成当前配置的只读鉴权检查，再提交状态变更。
    ///
    /// # 参数
    /// * `id` - 供应连接 ID。
    /// * `command` - 固定连接治理命令。
    /// * `actor` - 已通过鉴权的操作人。
    ///
    /// # 返回
    /// 新执行或同命令重放的正式回执。
    ///
    /// # 错误
    /// 请求非法、权限不足、命令形状不合法、版本冲突、引用无法解析或业务前置不满足时返回稳定错误。
    pub async fn execute_connection_command(
        &self,
        id: &str,
        command: SupplierConnectionCommand,
        actor: &AuditActor,
    ) -> Result<SupplierConnectionCommandResult> {
        command.validate()?;
        if let Some(result) = self.replay_command(id, &command, actor).await? {
            return Ok(result);
        }
        let identity = CommandIdentity::new(id, actor.id(), &command)?;
        let prepared = PreparedSupplierConnectionCommand::try_from_parts(
            command.action,
            command.expected_version,
            command.payload_reference.as_deref(),
            command.reason_code.as_deref(),
            command.check_type,
        )
        .map_err(map_command_shape_rejection)?;
        self.dispatch_connection_command(id, prepared, identity, actor).await
    }

    async fn dispatch_connection_command(
        &self,
        id: &str,
        prepared: PreparedSupplierConnectionCommand,
        identity: CommandIdentity,
        actor: &AuditActor,
    ) -> Result<SupplierConnectionCommandResult> {
        match prepared {
            PreparedSupplierConnectionCommand::UpdateBusinessProfile {
                expected_version,
                payload_reference,
            }
            | PreparedSupplierConnectionCommand::BindEndpointReference {
                expected_version,
                payload_reference,
            }
            | PreparedSupplierConnectionCommand::BindCredentialReference {
                expected_version,
                payload_reference,
            } => {
                self.execute_reference_command(
                    id,
                    identity.action,
                    &payload_reference,
                    expected_version,
                    identity,
                    actor,
                )
                .await
            },
            PreparedSupplierConnectionCommand::RunHealthCheck { expected_version, check_type } => {
                self.create_health_job(id, check_type, expected_version, identity, actor).await
            },
            PreparedSupplierConnectionCommand::Enable { expected_version }
            | PreparedSupplierConnectionCommand::Disable { expected_version, .. } => {
                self.execute_status_command(id, identity.action, expected_version, identity, actor).await
            },
            PreparedSupplierConnectionCommand::StartCatalogSync { expected_version } => {
                self.create_catalog_job(id, expected_version, identity, actor).await
            },
        }
    }

    async fn execute_status_command(
        &self,
        id: &str,
        action: SupplierConnectionAction,
        expected_version: u64,
        identity: CommandIdentity,
        actor: &AuditActor,
    ) -> Result<SupplierConnectionCommandResult> {
        let deployment = if action == SupplierConnectionAction::Enable {
            Some(self.checked_deployment(id, expected_version, actor).await?)
        } else {
            None
        };
        let authorization = self.authorization.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        let connection_id_value = id.to_string();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorization
                        .require(&actor, &action_permission(action), AccessTarget::Operation, executor)
                        .await?;
                    let domain = SupplierApiService::new(db.clone());
                    let mut connection = domain
                        .prepare_status_target(&connection_id_value, expected_version, executor)
                        .await?;
                    if let Some(deployment) = &deployment {
                        deployment.apply(&mut connection, actor.id())?;
                        connection.record_health(HealthCheckResult::Healthy, Instant::now());
                    }
                    domain.apply_status_change(&mut connection, action, actor.id(), executor).await?;
                    persist_command_receipt(
                        &db,
                        CommandReceiptWrite {
                            connection: &connection,
                            action,
                            identity: &identity,
                            outcome: SupplierCommandOutcome::Succeeded,
                            job_id: None,
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
