use database::SupplierApiExt;
use entities::supplier_api::{
    BusinessCapabilityConfirmation, BusinessCapabilityConfirmationData, CapabilityChangeInput,
    CapabilityChangeSet, CapabilityChangeSetRejection, ClassifiedCapabilityChangeSet,
    PreparedSupplierConnectionCommand, SupplierApiCapability, SupplierApiCapabilityData,
    SupplierApiCapabilityStatus, SupplierApiCapabilityUpdate, SupplierApiConnection,
    SupplierApiConnectionStatus, SupplierCommandOutcome, SupplierConnectionAction,
    SupplierConnectionCommandReceipt, SupplierConnectionCommandReceiptData, SupplierConnectionGovernance,
};
use erp_audit::AuditExt;
use erp_core::common::time::Instant;
use erp_core::ids::{SupplierApiCapabilityId, SupplierApiConnectionId};
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use crate::errors::{Error, Result};
use application_core::AuditActor;
use erp_audit::AuditActorLogs;

use super::super::dto::{
    ConfirmBusinessCapabilityRequirementCommand, ConfirmBusinessCapabilityRequirementResult,
    SupplierConnectionCommand, SupplierConnectionCommandResult, UpdateSupplierCapabilitiesCommand,
    UpdateSupplierCapabilitiesResult,
};
use super::super::{ClassifiedError, ResolvedSupplierReference, SupplierApiService, SupplierReferenceKind};
use super::context::{digest, ensure_version, map_command_shape_rejection};

impl SupplierApiService {
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
                    let capabilities = db
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
                    let classified = change_set
                        .classify(&capabilities)
                        .map_err(map_capability_change_rejection)?;
                    let (mut updates, creates) = apply_validated_changes(
                        &connection_id_value,
                        &classified,
                        &confirmations,
                        &capabilities,
                    )?;
                    db.supplier_api()
                        .persist_capability_changes(&mut updates, &creates, session)
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

    async fn execute_reference_command(
        &self,
        id: &str,
        action: SupplierConnectionAction,
        payload_reference: &str,
        expected_version: u64,
        identity: CommandIdentity,
        actor: &AuditActor,
    ) -> Result<SupplierConnectionCommandResult> {
        let connection = self.load_connection(id, &mut NoTransaction).await?;
        ensure_version(connection.base.version, expected_version)?;
        if connection.stable.status == SupplierApiConnectionStatus::Active {
            return Err(Error::BusinessLogicError(
                "连接启用期间不能变更配置，请先停用连接".to_string(),
            ));
        }
        let kind = match action {
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
        self.commit_reference_command(id, action, expected_version, identity, resolved, actor)
            .await
    }

    async fn commit_reference_command(
        &self,
        id: &str,
        action: SupplierConnectionAction,
        expected_version: u64,
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
                    ensure_version(connection.base.version, expected_version)?;
                    if connection.stable.status == SupplierApiConnectionStatus::Active {
                        return Err(Error::BusinessLogicError(
                            "连接启用期间不能变更配置，请先停用连接".to_string(),
                        ));
                    }
                    match action {
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
                    let mut connection = db
                        .supplier_api()
                        .connection(&SupplierApiConnectionId::new(&connection_id_value), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
                    ensure_version(connection.base.version, expected_version)?;
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
                    let blockers = governance.blockers(action, context.impact, true);
                    if let Some(blocker) = blockers.first() {
                        return Err(Error::BusinessLogicError(blocker.message.clone()));
                    }
                    match action {
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
}

pub(super) struct CommandIdentity {
    pub(super) connection_id: String,
    pub(super) actor_id: String,
    pub(super) action: SupplierConnectionAction,
    pub(super) idempotency_hash: String,
    pub(super) fingerprint: String,
    pub(super) receipt_id: String,
    pub(super) audit_id: String,
}

impl CommandIdentity {
    pub(super) fn new(id: &str, actor_id: &str, command: &SupplierConnectionCommand) -> Result<Self> {
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

pub(super) struct CommandReceiptWrite<'a> {
    pub(super) connection: &'a SupplierApiConnection,
    pub(super) action: SupplierConnectionAction,
    pub(super) identity: &'a CommandIdentity,
    pub(super) outcome: SupplierCommandOutcome,
    pub(super) job_id: Option<String>,
    pub(super) actor: &'a AuditActor,
}

pub(super) async fn persist_command_receipt(
    db: &mongodb::Database,
    write: CommandReceiptWrite<'_>,
    executor: &mut dyn persistence_core::Executor,
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

/// 将已分类能力变更应用于事务内快照，并拆分为更新与新增两组持久化输入。
///
/// 已存在能力逐项重验实时版本与采购确认覆盖后变更内存状态；新增能力以停用
/// 状态构造实体（ID 由调用方注入）。本函数只做内存装配，实际写库由
/// Repository 批量 primitive 在同一执行器下完成；调用方事务保证整体回滚。
///
/// # 参数
/// * `connection_id` - 所属连接 ID（新增实体归属）
/// * `classified` - 已分类变更集（保持输入顺序）
/// * `confirmations` - 最新优先的采购确认历史
/// * `capabilities` - 事务内加载的既有能力快照（只读，不就地变更）
///
/// # 返回
/// 返回 `(待 CAS 写回的已更新实体, 待批量插入的新增实体)`。
///
/// # 错误
/// 当实时版本冲突、启用缺少采购确认或实体构造校验失败时返回错误；任一失败
/// 由调用方事务整体回滚。
///
/// # 约束
/// 不访问数据库、不开事务；跨聚合确认结论只读取不解释归属。
pub(super) fn apply_validated_changes(
    connection_id: &str,
    classified: &ClassifiedCapabilityChangeSet,
    confirmations: &[BusinessCapabilityConfirmation],
    capabilities: &[SupplierApiCapability],
) -> Result<(Vec<SupplierApiCapability>, Vec<SupplierApiCapability>)> {
    let mut updates = Vec::with_capacity(classified.len());
    let mut creates = Vec::new();
    for change in classified.changes() {
        match capabilities
            .iter()
            .find(|capability| capability.capability_code == change.code)
        {
            Some(existing) => {
                ensure_version(existing.base.version, change.expected_version)?;
                if change.enabled
                    && !BusinessCapabilityConfirmation::latest_for(confirmations, change.code)
                        .is_some_and(|confirmation| confirmation.covers(existing))
                {
                    return Err(Error::BusinessLogicError(
                        "能力缺少与当前配置匹配的采购业务确认".to_string(),
                    ));
                }
                let mut updated = existing.clone();
                updated.update(SupplierApiCapabilityUpdate {
                    status: Some(if change.enabled {
                        SupplierApiCapabilityStatus::Active
                    } else {
                        SupplierApiCapabilityStatus::Disabled
                    }),
                    constraint_snapshot: change.constraint_snapshot.clone(),
                })?;
                updates.push(updated);
            }
            None => {
                creates.push(SupplierApiCapability::new(
                    SupplierApiCapabilityId::new(next_id()),
                    SupplierApiCapabilityData {
                        connection_id: SupplierApiConnectionId::new(connection_id),
                        capability_code: change.code,
                        status: SupplierApiCapabilityStatus::Disabled,
                        constraint_snapshot: change.constraint_snapshot.clone(),
                    },
                )?);
            }
        }
    }
    Ok((updates, creates))
}

/// 将能力变更集拒绝映射为历史 Service 错误语义（保持 HTTP 状态与文本）。
///
/// # 参数
/// * `rejection` - 变更集校验拒绝原因
///
/// # 返回
/// 形态问题映射为 `ValidationError`，新能力版本映射为 `ConflictError`，
/// 新能力启用映射为 `BusinessLogicError`。
pub(super) fn map_capability_change_rejection(rejection: CapabilityChangeSetRejection) -> Error {
    match rejection {
        CapabilityChangeSetRejection::EmptyOrTooMany
        | CapabilityChangeSetRejection::DuplicateCodes
        | CapabilityChangeSetRejection::MissingExpectedVersion(_)
        | CapabilityChangeSetRejection::UnexpectedExpectedVersion(_) => {
            Error::ValidationError(rejection.to_string())
        }
        CapabilityChangeSetRejection::NewCapabilityVersionMustBeZero(_) => {
            Error::ConflictError(rejection.to_string())
        }
        CapabilityChangeSetRejection::NewCapabilityMustStartDisabled(_) => {
            Error::BusinessLogicError(rejection.to_string())
        }
    }
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
