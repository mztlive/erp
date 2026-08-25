//! W20 供应商连接治理强命令、动作投影与后台任务执行。

use std::time::Instant as MonotonicInstant;

use database::{
    AccessControlExt, BulkJobExt, IntegrationOpsExt, NoTransaction, SupplierApiExt, Transactional,
    WorkItemExt,
};
use entities::bulk_job::{BackgroundJob, BackgroundJobData, JobStatus, JobType};
use entities::common::time::Instant;
use entities::ids::{
    BackgroundJobId, IntegrationErrorTaskId, SupplierApiCapabilityId, SupplierApiConnectionId,
};
use entities::integration_ops::{ErrorClass, IntegrationErrorTask, IntegrationErrorTaskData};
use entities::supplier_api::{
    ensure_unique_capability_codes, BusinessCapabilityConfirmation, BusinessCapabilityConfirmationData,
    CapabilityVersionSnapshot, HealthCheckResult, SupplierApiCapability, SupplierApiCapabilityData,
    SupplierApiCapabilityStatus, SupplierApiConnection, SupplierApiConnectionStatus, SupplierCommandOutcome,
    SupplierConnectionAction, SupplierConnectionCommandReceipt, SupplierConnectionCommandReceiptData,
    SupplierConnectionGovernance, SupplierGovernanceBlocker, SupplierHealthCheckRun,
    SupplierHealthCheckRunData, SupplierHealthCheckType,
};
use entities::Permission;
use id_generator::next_id;
use sha2::{Digest, Sha256};
use validator::Validate;

use super::dto::{
    ConfirmBusinessCapabilityRequirementCommand, ConfirmBusinessCapabilityRequirementResult,
    RelatedImpactView, SafeReferenceView, SafeReferencesView, SupplierActionBlockerView,
    SupplierApiCapabilityView, SupplierApiConnectionDetailView, SupplierApiConnectionListParams,
    SupplierApiConnectionView, SupplierConnectionCommand, SupplierConnectionCommandResult,
    SupplierConnectionJobView, SupplierHealthCheckRunView, UpdateSupplierCapabilitiesCommand,
    UpdateSupplierCapabilitiesResult,
};
use super::{
    ClassifiedError, PageView, ResolvedSupplierReference, SupplierApiService, SupplierReferenceKind,
};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::iam::subject;
use crate::integration_ops::{error_owner_role, error_work_item};

const HEALTH_JOB_TYPE: &str = "SUPPLIER_HEALTH_CHECK";
const CATALOG_JOB_TYPE: &str = "SUPPLIER_CATALOG_SYNC";
const CONFIRM_CAPABILITY_ACTION: &str = "CONFIRM_BUSINESS_CAPABILITY_REQUIREMENT";
const UPDATE_CAPABILITIES_ACTION: &str = "UPDATE_CAPABILITIES";

type SupplierConnectionImpact = <mongodb::Database as SupplierApiExt>::SupplierConnectionImpact;

struct GovernanceContext {
    capabilities: Vec<SupplierApiCapability>,
    confirmations: Vec<BusinessCapabilityConfirmation>,
    health_runs: Vec<SupplierHealthCheckRun>,
    impact: SupplierConnectionImpact,
}

impl SupplierApiService {
    /// 按当前操作人的权限与服务端业务事实返回连接分页投影。
    ///
    /// # Errors
    /// 查询、授权源或动作投影失败时返回错误。
    pub async fn connection_list_for_actor(
        &self,
        params: &SupplierApiConnectionListParams,
        actor: &AuditActor,
    ) -> Result<PageView<SupplierApiConnectionView>> {
        let mut page = self.connection_list(params).await?;
        for item in &mut page.items {
            let detail = self.connection_detail_for_actor(&item.id, actor).await?;
            *item = detail.connection;
        }
        Ok(page)
    }

    /// 返回服务端权威动作、阻塞原因和安全引用投影的连接详情。
    ///
    /// # Errors
    /// 连接不存在、查询失败或 RBAC 无法取得稳定快照时返回错误。
    pub async fn connection_detail_for_actor(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<SupplierApiConnectionDetailView> {
        let connection = self.load_connection(id, &mut NoTransaction).await?;
        let context = self.governance_context(&connection, &mut NoTransaction).await?;
        self.detail_view(connection, context, actor).await
    }

    /// 执行固定连接治理命令并返回可幂等重放的正式回执。
    ///
    /// HTTP 请求只登记健康检查或目录同步任务；外部调用由
    /// [`Self::process_connection_job`] 在后台执行。
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
        match command.action {
            SupplierConnectionAction::UpdateBusinessProfile
            | SupplierConnectionAction::BindEndpointReference
            | SupplierConnectionAction::BindCredentialReference => {
                self.execute_reference_command(id, command, identity, actor).await
            }
            SupplierConnectionAction::RunHealthCheck => {
                self.create_health_job(id, command, identity, actor).await
            }
            SupplierConnectionAction::Enable | SupplierConnectionAction::Disable => {
                self.execute_status_command(id, command, identity, actor).await
            }
            SupplierConnectionAction::StartCatalogSync => {
                self.create_catalog_job(id, command, identity, actor).await
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
            return replay_confirmation(existing, &fingerprint);
        }

        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        let operation_id = command.operation_id.trim().to_string();
        let connection_id_value = id.to_string();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut connection = db
                        .supplier_api()
                        .connection(&SupplierApiConnectionId::new(&connection_id_value), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
                    ensure_version(connection.base.version, command.expected_connection_version)?;
                    let capability = db
                        .supplier_api()
                        .connection_capability(
                            &SupplierApiConnectionId::new(&connection_id_value),
                            command.capability_code,
                            session,
                        )
                        .await?
                        .ok_or_else(|| Error::NotFound("连接能力不存在".to_string()))?;
                    ensure_version(capability.base.version, command.expected_capability_version)?;
                    let confirmation = BusinessCapabilityConfirmation::new(
                        format!("w20-confirm-{}", digest(&[&connection_id_value, &operation_id])),
                        BusinessCapabilityConfirmationData {
                            connection_id: SupplierApiConnectionId::new(connection_id_value.clone()),
                            capability_id: SupplierApiCapabilityId::new(capability.base.id.clone()),
                            capability_code: command.capability_code,
                            requirement: command.requirement,
                            applicability_reference: command.applicability_reference,
                            evidence_references: command.evidence_references,
                            reason_code: command.reason_code,
                            connection_version: connection.base.version,
                            capability_version: capability.base.version,
                            operation_id: operation_id.clone(),
                            idempotency_key_hash: idempotency_hash,
                            request_fingerprint: fingerprint.clone(),
                            confirmed_by: actor.id().to_string(),
                            confirmed_at: Instant::now(),
                        },
                    )?;
                    connection.touch_business_confirmation(actor.id());
                    db.supplier_api_connections()
                        .update(&mut connection, session)
                        .await?;
                    let audit_id = format!("w20-audit-{}", digest(&[&confirmation.base.id]));
                    let audit = actor.clone().resource_log_with_id(
                        audit_id.clone(),
                        "supplier_api_capability.confirm_requirement",
                        "supplier_api_capability",
                        capability.base.id.clone(),
                        Some(format!("request_sha256={fingerprint}")),
                    )?;
                    db.supplier_api_business_confirmations()
                        .create(&confirmation, session)
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
        ensure_unique_capability_codes(command.capability_changes.iter().map(|change| change.code))
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        let fingerprint = capability_update_fingerprint(id, &command);
        let audit_id = format!(
            "w20-cap-audit-{}",
            digest(&[actor.id(), id, command.idempotency_key.trim()])
        );
        if let Some(audit) = self
            .db
            .supplier_api()
            .governance_audit(&audit_id, &mut NoTransaction)
            .await?
        {
            ensure_audit_fingerprint(audit.message.as_deref(), &fingerprint)?;
            let detail = self.connection_detail_for_actor(id, actor).await?;
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
                    let mut connection = db
                        .supplier_api()
                        .connection(&SupplierApiConnectionId::new(&connection_id_value), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
                    ensure_version(connection.base.version, command.expected_connection_version)?;
                    if connection.stable.status == SupplierApiConnectionStatus::Active {
                        return Err(Error::BusinessLogicError(
                            "连接启用期间不能修改能力，请先停用连接".to_string(),
                        ));
                    }
                    let mut capabilities = db
                        .supplier_api()
                        .connection_capabilities(
                            &SupplierApiConnectionId::new(connection_id_value.clone()),
                            session,
                        )
                        .await?;
                    let confirmations = db
                        .supplier_api()
                        .business_confirmations(
                            &SupplierApiConnectionId::new(connection_id_value.clone()),
                            session,
                        )
                        .await?;
                    apply_capability_changes(
                        &db,
                        &connection_id_value,
                        &command,
                        &confirmations,
                        &mut capabilities,
                        session,
                    )
                    .await?;
                    connection.record_capability_configuration(actor_tx.id())?;
                    db.supplier_api_connections()
                        .update(&mut connection, session)
                        .await?;
                    let audit = actor_tx.clone().resource_log_with_id(
                        audit_id_tx.clone(),
                        "supplier_api_capability.update",
                        "supplier_api_connection",
                        connection_id_value.clone(),
                        Some(format!("request_sha256={fingerprint}")),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok(connection.base.version)
                })
            })
            .await?;
        let detail = self.connection_detail_for_actor(id, &actor).await?;
        Ok(UpdateSupplierCapabilitiesResult {
            outcome: SupplierCommandOutcome::Succeeded,
            operation_id,
            connection_version: result,
            capabilities: detail.capabilities,
            audit_event_id: audit_id,
        })
    }

    /// 查询连接下健康检查或目录同步后台任务的当前终态/进度。
    ///
    /// # Errors
    /// 任务不存在或不属于指定连接时返回 `NotFound`。
    pub async fn connection_job(
        &self,
        connection_id: &str,
        job_id: &str,
    ) -> Result<SupplierConnectionJobView> {
        let job = self
            .db
            .supplier_api()
            .connection_job(
                &SupplierApiConnectionId::new(connection_id),
                job_id,
                &[HEALTH_JOB_TYPE, CATALOG_JOB_TYPE],
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::NotFound("连接后台任务不存在".to_string()))?;
        Ok(job_view(job))
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
            Some(HEALTH_JOB_TYPE) => self.process_health_job(job, actor).await,
            Some(CATALOG_JOB_TYPE) => self.process_catalog_job(job, actor).await,
            _ => Err(Error::BusinessLogicError("任务不属于 W20 连接治理".to_string())),
        }
    }

    async fn execute_reference_command(
        &self,
        id: &str,
        command: SupplierConnectionCommand,
        identity: CommandIdentity,
        actor: &AuditActor,
    ) -> Result<SupplierConnectionCommandResult> {
        let payload_reference = required(command.payload_reference.as_deref(), "缺少不透明引用")?;
        let connection = self.load_connection(id, &mut NoTransaction).await?;
        ensure_version(connection.base.version, command.expected_version)?;
        if connection.stable.status == SupplierApiConnectionStatus::Active {
            return Err(Error::BusinessLogicError(
                "连接启用期间不能变更配置，请先停用连接".to_string(),
            ));
        }
        let kind = match command.action {
            SupplierConnectionAction::UpdateBusinessProfile => SupplierReferenceKind::BusinessProfile,
            SupplierConnectionAction::BindEndpointReference => SupplierReferenceKind::Endpoint,
            SupplierConnectionAction::BindCredentialReference => SupplierReferenceKind::Credential,
            _ => return Err(Error::Internal("引用命令分派错误".to_string())),
        };
        let resolved = self
            .reference_registry
            .resolve(kind, payload_reference, connection.environment)
            .await
            .map_err(reference_error)?;
        self.commit_reference_command(id, command, identity, resolved, actor)
            .await
    }

    async fn commit_reference_command(
        &self,
        id: &str,
        command: SupplierConnectionCommand,
        identity: CommandIdentity,
        resolved: ResolvedSupplierReference,
        actor: &AuditActor,
    ) -> Result<SupplierConnectionCommandResult> {
        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        let connection_id_value = id.to_string();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut connection = db
                        .supplier_api()
                        .connection(&SupplierApiConnectionId::new(&connection_id_value), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
                    ensure_version(connection.base.version, command.expected_version)?;
                    if connection.stable.status == SupplierApiConnectionStatus::Active {
                        return Err(Error::BusinessLogicError(
                            "连接启用期间不能变更配置，请先停用连接".to_string(),
                        ));
                    }
                    match command.action {
                        SupplierConnectionAction::UpdateBusinessProfile => {
                            connection.update_business_profile(resolved.internal_reference, actor.id())?
                        }
                        SupplierConnectionAction::BindEndpointReference => {
                            connection.bind_endpoint_reference(resolved.internal_reference, actor.id())?
                        }
                        SupplierConnectionAction::BindCredentialReference => {
                            connection.bind_credential_reference(resolved.internal_reference, actor.id())?
                        }
                        _ => return Err(Error::Internal("引用命令分派错误".to_string())),
                    }
                    db.supplier_api_connections()
                        .update(&mut connection, session)
                        .await?;
                    persist_command_receipt(
                        &db,
                        CommandReceiptWrite {
                            connection: &connection,
                            action: command.action,
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

    async fn execute_status_command(
        &self,
        id: &str,
        command: SupplierConnectionCommand,
        identity: CommandIdentity,
        actor: &AuditActor,
    ) -> Result<SupplierConnectionCommandResult> {
        if command.action == SupplierConnectionAction::Disable {
            required(command.reason_code.as_deref(), "停用原因不能为空")?;
        }
        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        let connection_id_value = id.to_string();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut connection = db
                        .supplier_api()
                        .connection(&SupplierApiConnectionId::new(&connection_id_value), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
                    ensure_version(connection.base.version, command.expected_version)?;
                    let context = db
                        .supplier_api()
                        .governance_data(&SupplierApiConnectionId::new(&connection_id_value), 50, session)
                        .await?;
                    let governance = SupplierConnectionGovernance {
                        connection: &connection,
                        capabilities: &context.capabilities,
                        confirmations: &context.confirmations,
                        health_runs: &context.health_runs,
                    };
                    let blockers = governance.blockers(command.action, context.impact, true);
                    if let Some(blocker) = blockers.first() {
                        return Err(Error::BusinessLogicError(blocker.message.clone()));
                    }
                    match command.action {
                        SupplierConnectionAction::Enable => connection.enable(actor.id()),
                        SupplierConnectionAction::Disable => connection.disable(actor.id()),
                        _ => return Err(Error::Internal("状态命令分派错误".to_string())),
                    }
                    db.supplier_api_connections()
                        .update(&mut connection, session)
                        .await?;
                    persist_command_receipt(
                        &db,
                        CommandReceiptWrite {
                            connection: &connection,
                            action: command.action,
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

    async fn create_health_job(
        &self,
        id: &str,
        command: SupplierConnectionCommand,
        identity: CommandIdentity,
        actor: &AuditActor,
    ) -> Result<SupplierConnectionCommandResult> {
        let check_type = command
            .check_type
            .ok_or_else(|| Error::ValidationError("健康检查类型不能为空".to_string()))?;
        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        let connection_id_value = id.to_string();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let connection = db
                        .supplier_api()
                        .connection(&SupplierApiConnectionId::new(&connection_id_value), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
                    ensure_version(connection.base.version, command.expected_version)?;
                    let capabilities = db
                        .supplier_api()
                        .connection_capabilities(
                            &SupplierApiConnectionId::new(connection_id_value.clone()),
                            session,
                        )
                        .await?;
                    let governance = SupplierConnectionGovernance {
                        connection: &connection,
                        capabilities: &capabilities,
                        confirmations: &[],
                        health_runs: &[],
                    };
                    if let Some(blocker) = governance
                        .blockers(command.action, Default::default(), true)
                        .first()
                    {
                        return Err(Error::BusinessLogicError(blocker.message.clone()));
                    }
                    let job = build_job(
                        &connection_id_value,
                        HEALTH_JOB_TYPE,
                        command.action,
                        actor.id(),
                        &identity,
                    )?;
                    let run = SupplierHealthCheckRun::new(
                        format!("w20-health-{}", digest(&[&job.base.id])),
                        SupplierHealthCheckRunData {
                            connection_id: SupplierApiConnectionId::new(connection_id_value.clone()),
                            background_job_id: job.base.id.clone(),
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
                            requested_by: actor.id().to_string(),
                            idempotency_key_hash: identity.idempotency_hash.clone(),
                            request_fingerprint: identity.fingerprint.clone(),
                        },
                    )?;
                    db.background_jobs().create(&job, session).await?;
                    db.supplier_api_health_check_runs().create(&run, session).await?;
                    persist_command_receipt(
                        &db,
                        CommandReceiptWrite {
                            connection: &connection,
                            action: command.action,
                            identity: &identity,
                            outcome: SupplierCommandOutcome::Processing,
                            job_id: Some(job.base.id.clone()),
                            actor: &actor,
                        },
                        session,
                    )
                    .await
                })
            })
            .await
    }

    async fn create_catalog_job(
        &self,
        id: &str,
        command: SupplierConnectionCommand,
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
                    let connection = db
                        .supplier_api()
                        .connection(&SupplierApiConnectionId::new(&connection_id_value), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
                    ensure_version(connection.base.version, command.expected_version)?;
                    let capabilities = db
                        .supplier_api()
                        .connection_capabilities(
                            &SupplierApiConnectionId::new(connection_id_value.clone()),
                            session,
                        )
                        .await?;
                    let governance = SupplierConnectionGovernance {
                        connection: &connection,
                        capabilities: &capabilities,
                        confirmations: &[],
                        health_runs: &[],
                    };
                    let blockers = governance.blockers(command.action, Default::default(), true);
                    if let Some(blocker) = blockers.first() {
                        return Err(Error::BusinessLogicError(blocker.message.clone()));
                    }
                    let job = build_job(
                        &connection_id_value,
                        CATALOG_JOB_TYPE,
                        command.action,
                        actor.id(),
                        &identity,
                    )?;
                    db.background_jobs().create(&job, session).await?;
                    persist_command_receipt(
                        &db,
                        CommandReceiptWrite {
                            connection: &connection,
                            action: command.action,
                            identity: &identity,
                            outcome: SupplierCommandOutcome::Processing,
                            job_id: Some(job.base.id.clone()),
                            actor: &actor,
                        },
                        session,
                    )
                    .await
                })
            })
            .await
    }

    async fn process_health_job(&self, job: BackgroundJob, actor: &AuditActor) -> Result<()> {
        let (connection, started_job, started_run) = self.start_health_job(job).await?;
        let started = MonotonicInstant::now();
        let outcome = self.gateway.health_check(&connection).await;
        let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.finish_health_job(started_job, started_run, outcome, latency_ms, actor)
            .await
    }

    async fn start_health_job(
        &self,
        job: BackgroundJob,
    ) -> Result<(SupplierApiConnection, BackgroundJob, SupplierHealthCheckRun)> {
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut job = db
                        .supplier_api()
                        .governance_job(&job.base.id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("健康检查任务不存在".to_string()))?;
                    if job.status != JobStatus::Pending {
                        return Err(Error::ConflictError("健康检查任务已开始或已结束".to_string()));
                    }
                    let mut run = db
                        .supplier_api()
                        .health_run_for_job(&job.base.id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("健康检查运行记录不存在".to_string()))?;
                    let connection = db
                        .supplier_api()
                        .connection(&run.connection_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
                    let at = Instant::now();
                    job.start(at)?;
                    run.start(at)?;
                    db.background_jobs().update(&mut job, session).await?;
                    db.supplier_api_health_check_runs()
                        .update(&mut run, session)
                        .await?;
                    Ok((connection, job, run))
                })
            })
            .await
    }

    async fn finish_health_job(
        &self,
        job: BackgroundJob,
        _run: SupplierHealthCheckRun,
        outcome: std::result::Result<(), ClassifiedError>,
        latency_ms: u64,
        actor: &AuditActor,
    ) -> Result<()> {
        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut job = db
                        .supplier_api()
                        .governance_job(&job.base.id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("健康检查任务不存在".to_string()))?;
                    let mut run = db
                        .supplier_api()
                        .health_run_for_job(&job.base.id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("健康检查运行记录不存在".to_string()))?;
                    let mut connection = db
                        .supplier_api()
                        .connection(&run.connection_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
                    let at = Instant::now();
                    let config_changed = connection.technical_config_version != run.technical_config_version;
                    if config_changed {
                        let error = ClassifiedError {
                            class: ErrorClass::ResultUnknown,
                            code: "TECHNICAL_CONFIG_CHANGED".to_string(),
                            summary: "检查期间技术配置已变化，本次结果不能作为启用依据".to_string(),
                        };
                        settle_health_failure(&mut job, &mut run, at, latency_ms, &error)?;
                        persist_health_failure_task(&db, &connection, &job, &error, &actor, session).await?;
                    } else if let Err(error) = &outcome {
                        settle_health_failure(&mut job, &mut run, at, latency_ms, error)?;
                        connection.record_health(HealthCheckResult::Failed, at);
                        connection.stable.touch(actor.id());
                        db.supplier_api_connections()
                            .update(&mut connection, session)
                            .await?;
                        persist_health_failure_task(&db, &connection, &job, error, &actor, session).await?;
                    } else {
                        job.record_progress(1, 0, 0, at)?;
                        job.mark_succeeded(at)?;
                        run.succeed(at, latency_ms)?;
                        connection.record_health(HealthCheckResult::Healthy, at);
                        connection.stable.touch(actor.id());
                        db.supplier_api_connections()
                            .update(&mut connection, session)
                            .await?;
                    }
                    db.background_jobs().update(&mut job, session).await?;
                    db.supplier_api_health_check_runs()
                        .update(&mut run, session)
                        .await?;
                    let audit = actor.clone().resource_log_with_id(
                        format!("w20-health-audit-{}", digest(&[&job.base.id])),
                        "supplier_api_connection.health_check.settle",
                        "supplier_api_connection",
                        connection.base.id,
                        Some(format!("job_id={};status={}", job.base.id, job.status.as_str())),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok(())
                })
            })
            .await
    }

    async fn process_catalog_job(&self, job: BackgroundJob, actor: &AuditActor) -> Result<()> {
        let connection_id = job
            .domain_job_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("目录同步任务缺少连接ID".to_string()))?;
        let connection = self.load_connection(&connection_id, &mut NoTransaction).await?;
        let started_job = self.start_background_job(job).await?;
        let outcome = self.gateway.catalog_sync(&connection).await;
        self.finish_catalog_job(started_job, connection, outcome, actor)
            .await
    }

    async fn start_background_job(&self, job: BackgroundJob) -> Result<BackgroundJob> {
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut job = db
                        .supplier_api()
                        .governance_job(&job.base.id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("后台任务不存在".to_string()))?;
                    if job.status != JobStatus::Pending {
                        return Err(Error::ConflictError("后台任务已开始或已结束".to_string()));
                    }
                    job.start(Instant::now())?;
                    db.background_jobs().update(&mut job, session).await?;
                    Ok(job)
                })
            })
            .await
    }

    async fn finish_catalog_job(
        &self,
        job: BackgroundJob,
        connection: SupplierApiConnection,
        outcome: std::result::Result<(), ClassifiedError>,
        actor: &AuditActor,
    ) -> Result<()> {
        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut job = db
                        .supplier_api()
                        .governance_job(&job.base.id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("目录同步任务不存在".to_string()))?;
                    let at = Instant::now();
                    if let Err(error) = &outcome {
                        job.record_progress(0, 0, 1, at)?;
                        job.mark_failed(Some(format!("{}: {}", error.code, error.summary)), at)?;
                        persist_health_failure_task(&db, &connection, &job, error, &actor, session).await?;
                    } else {
                        job.record_progress(1, 0, 0, at)?;
                        job.mark_succeeded(at)?;
                    }
                    db.background_jobs().update(&mut job, session).await?;
                    let audit = actor.clone().resource_log_with_id(
                        format!("w20-catalog-audit-{}", digest(&[&job.base.id])),
                        "supplier_api_connection.catalog_sync.settle",
                        "supplier_api_connection",
                        connection.base.id,
                        Some(format!("job_id={};status={}", job.base.id, job.status.as_str())),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok(())
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
                .supplier_api()
                .governance_job(job_id, &mut NoTransaction)
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

    async fn governance_context(
        &self,
        connection: &SupplierApiConnection,
        executor: &mut dyn database::Executor,
    ) -> Result<GovernanceContext> {
        let data = self
            .db
            .supplier_api()
            .governance_data(&SupplierApiConnectionId::new(&connection.base.id), 50, executor)
            .await?;
        Ok(GovernanceContext {
            capabilities: data.capabilities,
            confirmations: data.confirmations,
            health_runs: data.health_runs,
            impact: data.impact,
        })
    }

    async fn detail_view(
        &self,
        connection: SupplierApiConnection,
        context: GovernanceContext,
        actor: &AuditActor,
    ) -> Result<SupplierApiConnectionDetailView> {
        let reference_visible = self
            .has_permission(actor, "supplier_api_connection:view_reference_metadata")
            .await?;
        let governance = SupplierConnectionGovernance {
            connection: &connection,
            capabilities: &context.capabilities,
            confirmations: &context.confirmations,
            health_runs: &context.health_runs,
        };
        let latest_success = governance.latest_successful_health_run();
        let can_confirm = self
            .has_permission(actor, "supplier_api_capability:confirm_requirement")
            .await?;
        let can_update_capability = self
            .has_permission(actor, "supplier_api_capability:update")
            .await?;
        let mut capabilities = Vec::with_capacity(context.capabilities.len());
        for capability in &context.capabilities {
            let confirmation = governance.latest_confirmation(capability.capability_code);
            let verified = latest_success.is_some_and(|run| run.verifies(capability));
            let mut allowed_actions = Vec::new();
            let mut action_blockers = Vec::new();
            if can_confirm {
                allowed_actions.push(CONFIRM_CAPABILITY_ACTION.to_string());
            }
            if can_update_capability {
                if connection.stable.status == SupplierApiConnectionStatus::Active {
                    action_blockers.push(blocker(
                        UPDATE_CAPABILITIES_ACTION,
                        "CONNECTION_ENABLED",
                        "请先停用连接，再修改能力配置",
                        None,
                    ));
                } else {
                    allowed_actions.push(UPDATE_CAPABILITIES_ACTION.to_string());
                }
            }
            capabilities.push(SupplierApiCapabilityView {
                id: capability.base.id.clone(),
                connection_id: connection.base.id.clone(),
                capability_code: capability.capability_code,
                status: capability.status,
                version: capability.base.version,
                created_at: capability.base.created_at,
                constraint_summary: capability.constraint_snapshot.clone(),
                business_requirement: confirmation.map(|value| value.requirement),
                business_confirmation_version: confirmation.map(|value| value.base.version),
                technically_verified: verified,
                verified_at: latest_success
                    .filter(|_| verified)
                    .and_then(|run| run.finished_at)
                    .map(|at| at.unix_secs() as u64),
                allowed_actions,
                action_blockers,
            });
        }

        let (allowed_actions, action_blockers) = self
            .connection_action_projection(&connection, &context, actor)
            .await?;
        let mut connection_view: SupplierApiConnectionView = connection.clone().into();
        connection_view.safe_references = SafeReferencesView {
            endpoint: safe_reference(connection.endpoint_reference_bound, reference_visible),
            credential: safe_reference(connection.credential_reference_bound, reference_visible),
        };
        connection_view.allowed_actions = allowed_actions;
        connection_view.action_blockers = action_blockers;
        Ok(SupplierApiConnectionDetailView {
            connection: connection_view,
            capabilities,
            health_records: context.health_runs.iter().map(health_run_view).collect(),
            health_check_types: vec![
                SupplierHealthCheckType::Connectivity,
                SupplierHealthCheckType::Authentication,
                SupplierHealthCheckType::CapabilityMetadata,
            ],
            related_impact: impact_view(context.impact),
        })
    }

    async fn connection_action_projection(
        &self,
        connection: &SupplierApiConnection,
        context: &GovernanceContext,
        actor: &AuditActor,
    ) -> Result<(Vec<String>, Vec<SupplierActionBlockerView>)> {
        let mut allowed = Vec::new();
        let mut blocked = Vec::new();
        let governance = SupplierConnectionGovernance {
            connection,
            capabilities: &context.capabilities,
            confirmations: &context.confirmations,
            health_runs: &context.health_runs,
        };
        for action in SupplierConnectionAction::all() {
            if !self.has_action_permission(actor, action).await? {
                continue;
            }
            let blockers =
                governance.blockers(action, context.impact, self.reference_registry.is_available());
            if blockers.is_empty() {
                allowed.push(action.as_str().to_string());
            } else {
                blocked.extend(blockers.into_iter().map(governance_blocker_view));
            }
        }
        Ok((allowed, blocked))
    }

    async fn ensure_action_permission(
        &self,
        actor: &AuditActor,
        action: SupplierConnectionAction,
    ) -> Result<()> {
        let permission = action_permission(action);
        self.ensure_permission(actor, permission).await
    }

    async fn has_action_permission(
        &self,
        actor: &AuditActor,
        action: SupplierConnectionAction,
    ) -> Result<bool> {
        self.has_permission(actor, action_permission(action)).await
    }

    async fn ensure_permission(&self, actor: &AuditActor, permission: &str) -> Result<()> {
        if self.has_permission(actor, permission).await? {
            return Ok(());
        }
        Err(Error::Forbidden("当前角色不能执行该连接治理动作".to_string()))
    }

    async fn has_permission(&self, actor: &AuditActor, permission: &str) -> Result<bool> {
        let Some(rbac) = self.rbac.as_ref() else {
            return Ok(false);
        };
        let permission = Permission::parse(permission)?;
        rbac.enforce(&subject(actor.kind(), actor.id()), &permission)
            .await
    }

    async fn load_connection(
        &self,
        id: &str,
        executor: &mut dyn database::Executor,
    ) -> Result<SupplierApiConnection> {
        self.db
            .supplier_api()
            .connection(&SupplierApiConnectionId::new(id), executor)
            .await?
            .ok_or_else(|| Error::NotFound("连接不存在".to_string()))
    }
}

struct CommandIdentity {
    connection_id: String,
    actor_id: String,
    action: SupplierConnectionAction,
    idempotency_hash: String,
    fingerprint: String,
    receipt_id: String,
    audit_id: String,
}

impl CommandIdentity {
    fn new(id: &str, actor_id: &str, command: &SupplierConnectionCommand) -> Result<Self> {
        required(Some(command.idempotency_key.as_str()), "幂等键不能为空")?;
        let idempotency_hash = digest(&[
            actor_id,
            id,
            command.action.as_str(),
            command.idempotency_key.trim(),
        ]);
        let fingerprint = command_fingerprint(id, command);
        Ok(Self {
            connection_id: id.to_string(),
            actor_id: actor_id.to_string(),
            action: command.action,
            receipt_id: format!("w20-command-{idempotency_hash}"),
            audit_id: format!("w20-audit-{idempotency_hash}"),
            idempotency_hash,
            fingerprint,
        })
    }
}

struct CommandReceiptWrite<'a> {
    connection: &'a SupplierApiConnection,
    action: SupplierConnectionAction,
    identity: &'a CommandIdentity,
    outcome: SupplierCommandOutcome,
    job_id: Option<String>,
    actor: &'a AuditActor,
}

async fn persist_command_receipt(
    db: &mongodb::Database,
    write: CommandReceiptWrite<'_>,
    executor: &mut dyn database::Executor,
) -> Result<SupplierConnectionCommandResult> {
    let CommandReceiptWrite {
        connection,
        action,
        identity,
        outcome,
        job_id,
        actor,
    } = write;
    let receipt = SupplierConnectionCommandReceipt::new(
        identity.receipt_id.clone(),
        SupplierConnectionCommandReceiptData {
            connection_id: SupplierApiConnectionId::new(&connection.base.id),
            action,
            actor_id: actor.id().to_string(),
            idempotency_key_hash: identity.idempotency_hash.clone(),
            request_fingerprint: identity.fingerprint.clone(),
            outcome,
            connection_version: connection.base.version,
            job_id: job_id.clone(),
            audit_event_id: identity.audit_id.clone(),
        },
    )?;
    let audit = actor.clone().resource_log_with_id(
        identity.audit_id.clone(),
        &format!("supplier_api_connection.{}", action.as_str().to_ascii_lowercase()),
        "supplier_api_connection",
        connection.base.id.clone(),
        Some(format!("request_sha256={}", identity.fingerprint)),
    )?;
    db.supplier_api_command_receipts()
        .create(&receipt, executor)
        .await?;
    db.audit_logs().create(&audit, executor).await?;
    let job_no = match job_id.as_deref() {
        Some(job_id) => db
            .supplier_api()
            .governance_job(job_id, executor)
            .await?
            .map(|job| job.job_no),
        None => None,
    };
    Ok(SupplierConnectionCommandResult {
        outcome,
        action,
        operation_id: receipt.base.id,
        connection_version: connection.base.version,
        job_id,
        job_no,
        audit_event_id: identity.audit_id.clone(),
    })
}

fn build_job(
    connection_id: &str,
    domain_job_type: &str,
    _action: SupplierConnectionAction,
    actor_id: &str,
    identity: &CommandIdentity,
) -> Result<BackgroundJob> {
    let prefix = if domain_job_type == HEALTH_JOB_TYPE {
        "W20-HC"
    } else {
        "W20-CS"
    };
    BackgroundJob::new(
        BackgroundJobId::new(next_id()),
        BackgroundJobData {
            job_no: format!("{prefix}-{}", &identity.idempotency_hash[..16]),
            job_type: JobType::Sync,
            domain_job_type: Some(domain_job_type.to_string()),
            domain_job_id: Some(connection_id.to_string()),
            selection_snapshot_id: None,
            requested_by: actor_id.to_string(),
            request_id: format!("w20:{}", identity.idempotency_hash),
            input_file_asset_id: None,
            result_file_asset_id: None,
            total_count: 1,
        },
    )
    .map_err(Into::into)
}

async fn apply_capability_changes(
    db: &mongodb::Database,
    connection_id: &str,
    command: &UpdateSupplierCapabilitiesCommand,
    confirmations: &[BusinessCapabilityConfirmation],
    capabilities: &mut Vec<SupplierApiCapability>,
    executor: &mut dyn database::Executor,
) -> Result<()> {
    for change in &command.capability_changes {
        let key = change.code.as_str();
        let expected = command
            .expected_capability_versions
            .get(key)
            .copied()
            .ok_or_else(|| Error::ValidationError(format!("缺少能力 {key} 的期望版本")))?;
        if let Some(capability) = capabilities
            .iter_mut()
            .find(|capability| capability.capability_code == change.code)
        {
            ensure_version(capability.base.version, expected)?;
            if change.enabled
                && !BusinessCapabilityConfirmation::latest_for(confirmations, change.code)
                    .is_some_and(|confirmation| confirmation.covers(capability))
            {
                return Err(Error::BusinessLogicError(
                    "能力缺少与当前配置匹配的采购业务确认".to_string(),
                ));
            }
            capability.update(entities::supplier_api::SupplierApiCapabilityUpdate {
                status: Some(if change.enabled {
                    SupplierApiCapabilityStatus::Active
                } else {
                    SupplierApiCapabilityStatus::Disabled
                }),
                constraint_snapshot: change.constraint_snapshot.clone(),
            })?;
            db.supplier_api_capabilities()
                .update(capability, executor)
                .await?;
            continue;
        }
        if expected != 0 {
            return Err(Error::ConflictError(format!("新能力 {key} 的期望版本必须为0")));
        }
        if change.enabled {
            return Err(Error::BusinessLogicError(
                "新能力必须先以停用状态登记，再由采购确认后启用".to_string(),
            ));
        }
        let capability = SupplierApiCapability::new(
            SupplierApiCapabilityId::new(next_id()),
            SupplierApiCapabilityData {
                connection_id: SupplierApiConnectionId::new(connection_id),
                capability_code: change.code,
                status: SupplierApiCapabilityStatus::Disabled,
                constraint_snapshot: change.constraint_snapshot.clone(),
            },
        )?;
        db.supplier_api_capabilities()
            .create(&capability, executor)
            .await?;
        capabilities.push(capability);
    }
    Ok(())
}

fn action_permission(action: SupplierConnectionAction) -> &'static str {
    match action {
        SupplierConnectionAction::UpdateBusinessProfile => "supplier_api_connection:update_business_profile",
        SupplierConnectionAction::BindEndpointReference => "supplier_api_connection:bind_endpoint_reference",
        SupplierConnectionAction::BindCredentialReference => {
            "supplier_api_connection:manage_credential_reference"
        }
        SupplierConnectionAction::RunHealthCheck => "supplier_api_connection:health_check",
        SupplierConnectionAction::Enable => "supplier_api_connection:enable",
        SupplierConnectionAction::Disable => "supplier_api_connection:disable",
        SupplierConnectionAction::StartCatalogSync => "supplier_api_connection:catalog_sync",
    }
}

fn blocker(action: &str, code: &str, message: &str, destination: Option<&str>) -> SupplierActionBlockerView {
    SupplierActionBlockerView {
        action: action.to_string(),
        code: code.to_string(),
        message: message.to_string(),
        destination_workspace_id: destination.map(str::to_string),
    }
}

/// 将实体层治理阻塞原因转换为服务响应视图。
fn governance_blocker_view(blocker: SupplierGovernanceBlocker) -> SupplierActionBlockerView {
    SupplierActionBlockerView {
        action: blocker.action.as_str().to_string(),
        code: blocker.code.to_string(),
        message: blocker.message,
        destination_workspace_id: blocker.destination_workspace_id.map(str::to_string),
    }
}

fn safe_reference(bound: bool, visible: bool) -> SafeReferenceView {
    SafeReferenceView {
        state: if bound { "BOUND" } else { "MISSING" },
        alias: None,
        version: None,
        visible,
    }
}

fn impact_view(impact: SupplierConnectionImpact) -> RelatedImpactView {
    RelatedImpactView {
        active_offerings: impact.active_offerings,
        active_publications: impact.active_publications,
        open_supplier_orders: impact.open_supplier_orders,
        active_sync_jobs: impact.active_sync_jobs,
    }
}

fn health_run_view(run: &SupplierHealthCheckRun) -> SupplierHealthCheckRunView {
    SupplierHealthCheckRunView {
        id: run.base.id.clone(),
        job_id: run.background_job_id.clone(),
        check_type: run.check_type,
        status: run.status,
        technical_config_version: run.technical_config_version,
        requested_by: run.requested_by.clone(),
        started_at: run.started_at.map(|at| at.unix_secs() as u64),
        finished_at: run.finished_at.map(|at| at.unix_secs() as u64),
        latency_ms: run.latency_ms,
        error_code: run.error_code.clone(),
        error_summary: run.error_summary.clone(),
    }
}

fn job_view(job: BackgroundJob) -> SupplierConnectionJobView {
    SupplierConnectionJobView {
        job_id: job.base.id,
        job_no: job.job_no,
        action: job.domain_job_type.unwrap_or_default(),
        status: job.status,
        total: job.total_count,
        processed: job.processed_count,
        succeeded: job.success_count,
        failed: job.failed_count,
        error_summary: job.error_summary,
        created_at: job.base.created_at,
        finished_at: job.finished_at.map(|at| at.unix_secs() as u64),
    }
}

fn settle_health_failure(
    job: &mut BackgroundJob,
    run: &mut SupplierHealthCheckRun,
    at: Instant,
    latency_ms: u64,
    error: &ClassifiedError,
) -> Result<()> {
    job.record_progress(0, 0, 1, at)?;
    job.mark_failed(Some(format!("{}: {}", error.code, error.summary)), at)?;
    if error.class == ErrorClass::ResultUnknown {
        run.mark_unknown(at, latency_ms, error.code.clone(), error.summary.clone())?;
    } else {
        run.fail(at, latency_ms, error.code.clone(), error.summary.clone())?;
    }
    Ok(())
}

async fn persist_health_failure_task(
    db: &mongodb::Database,
    connection: &SupplierApiConnection,
    job: &BackgroundJob,
    error: &ClassifiedError,
    actor: &AuditActor,
    executor: &mut dyn database::Executor,
) -> Result<()> {
    let task = IntegrationErrorTask::new(
        IntegrationErrorTaskId::new(format!("w20-error-{}", digest(&[&job.base.id]))),
        IntegrationErrorTaskData {
            message_id: None,
            business_object_id: Some(connection.base.id.clone()),
            error_class: error.class,
            owner_role: Some(error_owner_role(error.class).to_string()),
            owner_user_id: Some(actor.id().to_string()),
        },
    )?;
    let work_item = error_work_item(&task, actor.id())?;
    let work_item_audit = actor.clone().resource_log_with_id(
        format!("w20-work-audit-{}", digest(&[&job.base.id])),
        "integration_error_task.work_item.create",
        "work_item",
        work_item.base.id.clone(),
        Some(format!("job_id={}", job.base.id)),
    )?;
    db.integration_error_tasks().create(&task, executor).await?;
    db.work_items().create(&work_item, executor).await?;
    db.audit_logs().create(&work_item_audit, executor).await?;
    Ok(())
}

fn replay_confirmation(
    confirmation: BusinessCapabilityConfirmation,
    fingerprint: &str,
) -> Result<ConfirmBusinessCapabilityRequirementResult> {
    if confirmation.request_fingerprint != fingerprint {
        return Err(Error::ConflictError("同一幂等键不能提交不同参数".to_string()));
    }
    Ok(ConfirmBusinessCapabilityRequirementResult {
        outcome: SupplierCommandOutcome::Succeeded,
        operation_id: confirmation.operation_id,
        confirmation_id: confirmation.base.id.clone(),
        confirmation_version: confirmation.base.version,
        connection_version: confirmation.connection_version.saturating_add(1),
        capability_version: confirmation.capability_version,
        audit_event_id: format!("w20-audit-{}", digest(&[&confirmation.base.id])),
    })
}

fn ensure_version(actual: u64, expected: u64) -> Result<()> {
    if actual == expected {
        return Ok(());
    }
    Err(Error::ConflictError(
        "数据已被其他请求修改，请刷新后重试".to_string(),
    ))
}

fn required<'a>(value: Option<&'a str>, message: &str) -> Result<&'a str> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::ValidationError(message.to_string()))
}

fn reference_error(error: ClassifiedError) -> Error {
    Error::BusinessLogicError(format!("{}: {}", error.code, error.summary))
}

fn command_fingerprint(id: &str, command: &SupplierConnectionCommand) -> String {
    digest(&[
        id,
        command.action.as_str(),
        &command.expected_version.to_string(),
        command.payload_reference.as_deref().unwrap_or_default(),
        command.reason_code.as_deref().unwrap_or_default(),
        command
            .check_type
            .map(|value| format!("{value:?}"))
            .as_deref()
            .unwrap_or_default(),
    ])
}

fn confirmation_fingerprint(id: &str, command: &ConfirmBusinessCapabilityRequirementCommand) -> String {
    let mut evidence = command.evidence_references.clone();
    evidence.sort();
    digest(&[
        id,
        command.capability_code.as_str(),
        &format!("{:?}", command.requirement),
        command.applicability_reference.as_deref().unwrap_or_default(),
        &evidence.join("\u{1f}"),
        command.reason_code.trim(),
        &command.expected_connection_version.to_string(),
        &command.expected_capability_version.to_string(),
        command.operation_id.trim(),
    ])
}

fn capability_update_fingerprint(id: &str, command: &UpdateSupplierCapabilitiesCommand) -> String {
    let payload = serde_json::to_string(command).unwrap_or_default();
    digest(&[id, &payload])
}

fn ensure_audit_fingerprint(message: Option<&str>, fingerprint: &str) -> Result<()> {
    if message == Some(format!("request_sha256={fingerprint}").as_str()) {
        return Ok(());
    }
    Err(Error::ConflictError("同一幂等键不能提交不同参数".to_string()))
}

fn digest(parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    hex::encode(digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_identity_hashes_raw_idempotency_key() {
        let command = SupplierConnectionCommand {
            action: SupplierConnectionAction::RunHealthCheck,
            expected_version: 1,
            payload_reference: None,
            reason_code: None,
            check_type: Some(SupplierHealthCheckType::Connectivity),
            idempotency_key: "raw-secret-like-key".to_string(),
        };
        let identity = CommandIdentity::new("connection-1", "actor-1", &command).unwrap();
        assert!(!identity.idempotency_hash.contains("raw-secret-like-key"));
        assert_eq!(identity.idempotency_hash.len(), 64);
    }
}
