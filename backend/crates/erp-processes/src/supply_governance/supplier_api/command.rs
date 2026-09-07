use super::receipt::{persist_command_receipt, CommandReceiptWrite};
use super::SupplierApiGovernanceProcess;
use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_supply::dto::supplier_api::*;
use erp_supply::entity::supplier_api::{
    CapabilityChangeInput, CapabilityChangeSet, PreparedSupplierConnectionCommand, SupplierApiConnectionId,
    SupplierCommandOutcome, SupplierConnectionAction,
};
use erp_supply::repository::SupplierApiExt;
use erp_supply::service::supplier_api::{
    command::{
        capability_update_fingerprint, confirmation_fingerprint, ensure_audit_fingerprint,
        map_capability_change_rejection, replay_confirmation, CommandIdentity,
    },
    context::{digest, map_command_shape_rejection},
    SupplierApiService,
};
use erp_support::BulkJobExt;
use persistence_core::{NoTransaction, Transactional};
use services::{Error, Result};
use validator::Validate;
impl SupplierApiGovernanceProcess {
    /// 执行固定连接治理命令并返回可幂等重放的正式回执。
    ///
    /// HTTP 请求只登记健康检查或目录同步任务；外部调用由
    /// 流程层 `SupplierConnectionExecutionProcess::process_connection_job` 在后台执行。
    ///
    /// # Errors
    /// 权限不足、版本冲突、引用无法解析或业务前置不满足时返回稳定错误。
    pub async fn execute_connection_command(
        &self,
        id: &str,
        command: SupplierConnectionCommand,
        actor: &AuditActor,
    ) -> Result<SupplierConnectionCommandResult> {
        command.validate()?;
        self.ensure_action_permission(actor, command.action).await?;
        let identity = CommandIdentity::new(id, actor.id(), &command)?;
        if let Some(result) = self.replay_command(&identity).await? {
            return Ok(result);
        }
        let prepared = PreparedSupplierConnectionCommand::try_from_parts(
            command.action,
            command.expected_version,
            command.payload_reference.as_deref(),
            command.reason_code.as_deref(),
            command.check_type,
        )
        .map_err(map_command_shape_rejection)?;
        match prepared {
            PreparedSupplierConnectionCommand::UpdateBusinessProfile {
                expected_version,
                payload_reference,
            } => {
                self.execute_reference_command(
                    id,
                    SupplierConnectionAction::UpdateBusinessProfile,
                    &payload_reference,
                    expected_version,
                    identity,
                    actor,
                )
                .await
            }
            PreparedSupplierConnectionCommand::BindEndpointReference {
                expected_version,
                payload_reference,
            } => {
                self.execute_reference_command(
                    id,
                    SupplierConnectionAction::BindEndpointReference,
                    &payload_reference,
                    expected_version,
                    identity,
                    actor,
                )
                .await
            }
            PreparedSupplierConnectionCommand::BindCredentialReference {
                expected_version,
                payload_reference,
            } => {
                self.execute_reference_command(
                    id,
                    SupplierConnectionAction::BindCredentialReference,
                    &payload_reference,
                    expected_version,
                    identity,
                    actor,
                )
                .await
            }
            PreparedSupplierConnectionCommand::RunHealthCheck {
                expected_version,
                check_type,
            } => {
                self.create_health_job(id, check_type, expected_version, identity, actor)
                    .await
            }
            PreparedSupplierConnectionCommand::Enable { expected_version } => {
                self.execute_status_command(
                    id,
                    SupplierConnectionAction::Enable,
                    expected_version,
                    identity,
                    actor,
                )
                .await
            }
            PreparedSupplierConnectionCommand::Disable { expected_version, .. } => {
                self.execute_status_command(
                    id,
                    SupplierConnectionAction::Disable,
                    expected_version,
                    identity,
                    actor,
                )
                .await
            }
            PreparedSupplierConnectionCommand::StartCatalogSync { expected_version } => {
                self.create_catalog_job(id, expected_version, identity, actor)
                    .await
            }
        }
    }

    /// 追加采购业务能力确认；不修改能力启停且不创建工作项。
    ///
    /// # Errors
    /// 权限、连接/能力版本、同键异参或数据一致性校验失败时返回错误。
    pub async fn confirm_business_capability_requirement(
        &self,
        id: &str,
        command: ConfirmBusinessCapabilityRequirementCommand,
        actor: &AuditActor,
    ) -> Result<ConfirmBusinessCapabilityRequirementResult> {
        command.validate()?;
        self.ensure_permission(actor, "supplier_api_capability:confirm_requirement")
            .await?;
        let connection_id = SupplierApiConnectionId::new(id);
        let idempotency_hash = digest(&[actor.id(), id, command.idempotency_key.trim()]);
        let fingerprint = confirmation_fingerprint(id, &command);
        if let Some(existing) = self
            .db
            .supplier_api()
            .business_confirmation_receipt(&connection_id, actor.id(), &idempotency_hash, &mut NoTransaction)
            .await?
        {
            return Ok(replay_confirmation(existing, &fingerprint)?);
        }

        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        let operation_id = command.operation_id.trim().to_string();
        let connection_id_value = id.to_string();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let prepared = SupplierApiService::new(db.clone())
                        .prepare_business_confirmation(
                            erp_supply::service::supplier_api::confirmation::BusinessConfirmationInput {
                                id: connection_id_value,
                                command,
                                operation_id: operation_id.clone(),
                                idempotency_hash,
                                fingerprint: fingerprint.clone(),
                                actor_id: actor.id(),
                            },
                            session,
                        )
                        .await?;
                    let erp_supply::service::supplier_api::confirmation::PreparedBusinessConfirmation {
                        connection,
                        capability,
                        confirmation,
                    } = prepared;
                    let audit_id = format!("w20-audit-{}", digest(&[&confirmation.base.id]));
                    let audit = actor.clone().resource_log_with_id(
                        audit_id.clone(),
                        "supplier_api_capability.confirm_requirement",
                        "supplier_api_capability",
                        capability.base.id.clone(),
                        Some(format!("request_sha256={fingerprint}")),
                    )?;
                    SupplierApiService::new(db.clone())
                        .persist_business_confirmation(&confirmation, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok(ConfirmBusinessCapabilityRequirementResult {
                        outcome: SupplierCommandOutcome::Succeeded,
                        operation_id,
                        confirmation_id: confirmation.base.id,
                        confirmation_version: confirmation.base.version,
                        connection_version: connection.base.version,
                        capability_version: capability.base.version,
                        audit_event_id: audit_id,
                    })
                })
            })
            .await
    }

    /// 使用连接版本与逐能力版本原子更新固定能力配置。
    ///
    /// # Errors
    /// 权限不足、版本冲突、重复能力代码或启用能力缺少采购确认时返回错误。
    pub async fn update_capabilities(
        &self,
        id: &str,
        command: UpdateSupplierCapabilitiesCommand,
        actor: &AuditActor,
    ) -> Result<UpdateSupplierCapabilitiesResult> {
        command.validate()?;
        self.ensure_permission(actor, "supplier_api_capability:update")
            .await?;
        let change_set = CapabilityChangeSet::new(
            command
                .capability_changes
                .iter()
                .map(|change| CapabilityChangeInput {
                    code: change.code,
                    enabled: change.enabled,
                    constraint_snapshot: change.constraint_snapshot.clone(),
                })
                .collect(),
            &command.expected_capability_versions,
        )
        .map_err(map_capability_change_rejection)?;
        let fingerprint = capability_update_fingerprint(id, &command);
        let audit_id = format!(
            "w20-cap-audit-{}",
            digest(&[actor.id(), id, command.idempotency_key.trim()])
        );
        if let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(&audit_id, &mut NoTransaction)
            .await?
        {
            ensure_audit_fingerprint(audit.message.as_deref(), &fingerprint)?;
            let detail = self.reads().connection_detail_for_actor(id, actor).await?;
            return Ok(UpdateSupplierCapabilitiesResult {
                outcome: SupplierCommandOutcome::Succeeded,
                operation_id: command.operation_id,
                connection_version: detail.connection.version,
                capabilities: detail.capabilities,
                audit_event_id: audit_id,
            });
        }

        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        let actor_tx = actor.clone();
        let operation_id = command.operation_id.clone();
        let connection_id_value = id.to_string();
        let audit_id_tx = audit_id.clone();
        let result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let connection = SupplierApiService::new(db.clone())
                        .apply_capability_changes(
                            &connection_id_value,
                            command.expected_connection_version,
                            change_set,
                            actor_tx.id(),
                            session,
                        )
                        .await?;
                    let audit = actor_tx.clone().resource_log_with_id(
                        audit_id_tx.clone(),
                        "supplier_api_capability.update",
                        "supplier_api_connection",
                        connection_id_value.clone(),
                        Some(format!("request_sha256={fingerprint}")),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<u64, Error>(connection.base.version)
                })
            })
            .await?;
        let detail = self.reads().connection_detail_for_actor(id, &actor).await?;
        Ok(UpdateSupplierCapabilitiesResult {
            outcome: SupplierCommandOutcome::Succeeded,
            operation_id,
            connection_version: result,
            capabilities: detail.capabilities,
            audit_event_id: audit_id,
        })
    }

    async fn execute_status_command(
        &self,
        id: &str,
        action: SupplierConnectionAction,
        expected_version: u64,
        identity: CommandIdentity,
        actor: &AuditActor,
    ) -> Result<SupplierConnectionCommandResult> {
        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        let connection_id_value = id.to_string();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let domain = SupplierApiService::new(db.clone());
                    let (mut connection, context) = domain
                        .prepare_status_target(&connection_id_value, expected_version, session)
                        .await?;
                    let active_sync_jobs = db
                        .background_jobs()
                        .count_active_supplier_catalog_jobs(&connection_id_value, session)
                        .await?;
                    domain
                        .apply_status_change(
                            &mut connection,
                            &context,
                            active_sync_jobs,
                            action,
                            actor.id(),
                            session,
                        )
                        .await?;
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
                        session,
                    )
                    .await
                })
            })
            .await
    }

    async fn replay_command(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<SupplierConnectionCommandResult>> {
        let Some(receipt) = self
            .db
            .supplier_api()
            .command_receipt(
                &SupplierApiConnectionId::new(&identity.connection_id),
                identity.action,
                &identity.actor_id,
                &identity.idempotency_hash,
                &mut NoTransaction,
            )
            .await?
        else {
            return Ok(None);
        };
        if receipt.request_fingerprint != identity.fingerprint {
            return Err(Error::ConflictError("同一幂等键不能提交不同参数".to_string()));
        }
        let job_no = match receipt.job_id.as_deref() {
            Some(job_id) => self
                .db
                .background_jobs()
                .find_by_id(job_id, &mut NoTransaction)
                .await?
                .map(|job| job.job_no),
            None => None,
        };
        Ok(Some(SupplierConnectionCommandResult {
            outcome: receipt.outcome,
            action: receipt.action,
            operation_id: receipt.base.id,
            connection_version: receipt.connection_version,
            job_id: receipt.job_id,
            job_no,
            audit_event_id: receipt.audit_event_id,
        }))
    }
}
