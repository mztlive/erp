//! W20 供应商连接治理强命令、动作投影与后台任务执行。

use std::collections::HashMap;
use std::time::Instant as MonotonicInstant;

use database::{
    AccessControlExt, BulkJobExt, IntegrationOpsExt, NoTransaction, PartyExt, SupplierApiExt, SupplierExt,
    Transactional, WorkItemExt,
};
use entities::bulk_job::{
    BackgroundJob, JobStatus, SupplierGovernanceJobKind, SupplierGovernanceJobSpec,
    SUPPLIER_CATALOG_SYNC_JOB_TYPE, SUPPLIER_HEALTH_CHECK_JOB_TYPE,
};
use entities::common::time::Instant;
use entities::ids::{
    BackgroundJobId, IntegrationErrorTaskId, PartyId, SupplierAccountId, SupplierApiCapabilityId,
    SupplierApiConnectionId,
};
use entities::integration_ops::{ErrorClass, IntegrationErrorTask, IntegrationErrorTaskData};
use entities::supplier_api::{
    BusinessCapabilityConfirmation, BusinessCapabilityConfirmationData, CapabilityChangeInput,
    CapabilityChangeSet, CapabilityChangeSetRejection, CapabilityVersionSnapshot,
    ClassifiedCapabilityChangeSet, HealthCheckResult, PreparedSupplierConnectionCommand,
    SupplierApiCapability, SupplierApiCapabilityData, SupplierApiCapabilityStatus,
    SupplierApiCapabilityUpdate, SupplierApiConnection, SupplierApiConnectionStatus, SupplierCommandOutcome,
    SupplierCommandShapeRejection, SupplierConnectionAction, SupplierConnectionCommandReceipt,
    SupplierConnectionCommandReceiptData, SupplierConnectionGovernance, SupplierGovernanceBlocker,
    SupplierHealthCheckRun, SupplierHealthCheckRunData, SupplierHealthCheckType,
};
use entities::Permission;
use id_generator::next_id;
use sha2::{Digest, Sha256};
use validator::Validate;

use super::dto::{
    ConfirmBusinessCapabilityRequirementCommand, ConfirmBusinessCapabilityRequirementResult,
    RelatedImpactView, SafeReferenceView, SafeReferencesView, SupplierActionBlockerView,
    SupplierApiCapabilitySummaryView, SupplierApiCapabilityView, SupplierApiConnectionDetailView,
    SupplierApiConnectionListItemView, SupplierApiConnectionListParams, SupplierApiConnectionView,
    SupplierConnectionCommand, SupplierConnectionCommandResult, SupplierConnectionJobView,
    SupplierHealthCheckRunView, UpdateSupplierCapabilitiesCommand, UpdateSupplierCapabilitiesResult,
};
use super::{
    ClassifiedError, PageView, ResolvedSupplierReference, SupplierApiService, SupplierReferenceKind,
};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::iam::subject;
use crate::integration_ops::{error_owner_role, error_work_item};

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
        _actor: &AuditActor,
    ) -> Result<PageView<SupplierApiConnectionListItemView>> {
        let page = self.connection_list(params).await?;
        let connection_ids = page
            .items
            .iter()
            .map(|item| SupplierApiConnectionId::new(&item.id))
            .collect::<Vec<_>>();
        let capabilities = self
            .db
            .supplier_api_capabilities()
            .find_capabilities_by_connections(&connection_ids, &mut NoTransaction)
            .await?;
        let capabilities_by_connection = capabilities.into_iter().fold(
            HashMap::<String, Vec<SupplierApiCapabilitySummaryView>>::new(),
            |mut grouped, capability| {
                grouped
                    .entry(capability.connection_id.to_string())
                    .or_default()
                    .push(SupplierApiCapabilitySummaryView {
                        capability_code: capability.capability_code,
                        status: capability.status,
                    });
                grouped
            },
        );
        let supplier_names = self.supplier_names_for_connections(&page.items).await?;
        let items = page
            .items
            .into_iter()
            .map(|connection| SupplierApiConnectionListItemView {
                supplier_name: supplier_names.get(&connection.supplier_id).cloned(),
                capabilities: capabilities_by_connection
                    .get(&connection.id)
                    .cloned()
                    .unwrap_or_default(),
                connection,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: page.page,
            page_size: page.page_size,
        })
    }

    async fn supplier_names_for_connections(
        &self,
        connections: &[SupplierApiConnectionView],
    ) -> Result<HashMap<String, String>> {
        let supplier_ids = connections
            .iter()
            .map(|item| SupplierAccountId::new(&item.supplier_id))
            .collect::<Vec<_>>();
        let accounts = self
            .db
            .supplier_accounts()
            .find_accounts_by_ids(&supplier_ids, &mut NoTransaction)
            .await?;
        let party_ids = accounts
            .iter()
            .map(|account| PartyId::new(account.party_id.to_string()))
            .collect::<Vec<_>>();
        let (parties, revisions) = self
            .db
            .party()
            .list_with_current_revisions(&party_ids, &mut NoTransaction)
            .await?;
        let revisions_by_id = revisions
            .into_iter()
            .map(|revision| (revision.base.id.clone(), revision.legal_name))
            .collect::<HashMap<_, _>>();
        let names_by_party = parties
            .into_iter()
            .filter_map(|party| {
                let revision_id = party.stable.current_revision_id?;
                let name = revisions_by_id.get(&revision_id)?.clone();
                Some((party.base.id, name))
            })
            .collect::<HashMap<_, _>>();
        Ok(accounts
            .into_iter()
            .filter_map(|account| {
                let name = names_by_party.get(account.party_id.as_ref())?.clone();
                Some((account.base.id, name))
            })
            .collect())
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
                &[SUPPLIER_HEALTH_CHECK_JOB_TYPE, SUPPLIER_CATALOG_SYNC_JOB_TYPE],
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
            Some(SUPPLIER_HEALTH_CHECK_JOB_TYPE) => self.process_health_job(job, actor).await,
            Some(SUPPLIER_CATALOG_SYNC_JOB_TYPE) => self.process_catalog_job(job, actor).await,
            _ => Err(Error::BusinessLogicError("任务不属于 W20 连接治理".to_string())),
        }
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

    async fn create_health_job(
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
            .with_transaction(move |session| {
                Box::pin(async move {
                    let connection = db
                        .supplier_api()
                        .connection(&SupplierApiConnectionId::new(&connection_id_value), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
                    ensure_version(connection.base.version, expected_version)?;
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
                        .blockers(SupplierConnectionAction::RunHealthCheck, Default::default(), true)
                        .first()
                    {
                        return Err(Error::BusinessLogicError(blocker.message.clone()));
                    }
                    let job = BackgroundJob::for_supplier_governance(SupplierGovernanceJobSpec {
                        job_id: BackgroundJobId::new(next_id()),
                        connection_id: connection_id_value.clone(),
                        kind: SupplierGovernanceJobKind::HealthCheck,
                        requested_by: actor.id().to_string(),
                        idempotency_hash: identity.idempotency_hash.clone(),
                    })?;
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
                            action: SupplierConnectionAction::RunHealthCheck,
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
                    let connection = db
                        .supplier_api()
                        .connection(&SupplierApiConnectionId::new(&connection_id_value), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
                    ensure_version(connection.base.version, expected_version)?;
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
                    let blockers = governance.blockers(
                        SupplierConnectionAction::StartCatalogSync,
                        Default::default(),
                        true,
                    );
                    if let Some(blocker) = blockers.first() {
                        return Err(Error::BusinessLogicError(blocker.message.clone()));
                    }
                    let job = BackgroundJob::for_supplier_governance(SupplierGovernanceJobSpec {
                        job_id: BackgroundJobId::new(next_id()),
                        connection_id: connection_id_value.clone(),
                        kind: SupplierGovernanceJobKind::CatalogSync,
                        requested_by: actor.id().to_string(),
                        idempotency_hash: identity.idempotency_hash.clone(),
                    })?;
                    db.background_jobs().create(&job, session).await?;
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
fn apply_validated_changes(
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
fn map_capability_change_rejection(rejection: CapabilityChangeSetRejection) -> Error {
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

/// 将命令形态拒绝映射为参数校验错误（保持历史必填校验语义）。
///
/// # 参数
/// * `rejection` - 命令形态校验拒绝原因
///
/// # 返回
/// 一律映射为 `ValidationError`（含新增的多余字段拒绝）。
pub(crate) fn map_command_shape_rejection(rejection: SupplierCommandShapeRejection) -> Error {
    Error::ValidationError(rejection.to_string())
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
    use entities::supplier_api::{
        BusinessCapabilityRequirement, SupplierApiCapabilityCode, SupplierApiCapabilityData,
    };
    use std::collections::BTreeMap;

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

    #[test]
    fn capability_change_rejections_keep_legacy_error_semantics() {
        assert!(matches!(
            map_capability_change_rejection(CapabilityChangeSetRejection::DuplicateCodes),
            Error::ValidationError(message) if message == "能力变更代码不能重复"
        ));
        assert!(matches!(
            map_capability_change_rejection(
                CapabilityChangeSetRejection::MissingExpectedVersion("order")
            ),
            Error::ValidationError(message) if message == "缺少能力 order 的期望版本"
        ));
        assert!(matches!(
            map_capability_change_rejection(CapabilityChangeSetRejection::UnexpectedExpectedVersion(
                "product".to_string()
            )),
            Error::ValidationError(_)
        ));
        assert!(matches!(
            map_capability_change_rejection(
                CapabilityChangeSetRejection::NewCapabilityVersionMustBeZero("product")
            ),
            Error::ConflictError(message) if message == "新能力 product 的期望版本必须为0"
        ));
        assert!(matches!(
            map_capability_change_rejection(CapabilityChangeSetRejection::NewCapabilityMustStartDisabled(
                "product"
            )),
            Error::BusinessLogicError(_)
        ));
    }

    #[test]
    fn command_shape_rejections_map_to_validation_errors() {
        let error = map_command_shape_rejection(SupplierCommandShapeRejection::TechnicalReferenceOnCreate);
        assert!(matches!(error, Error::ValidationError(_)));
    }

    /// 构造既有能力声明测试夹具（版本固定为 `1`）。
    fn existing_capability(code: SupplierApiCapabilityCode) -> SupplierApiCapability {
        SupplierApiCapability::new(
            SupplierApiCapabilityId::new(format!("cap-{}", code.as_str())),
            SupplierApiCapabilityData {
                connection_id: SupplierApiConnectionId::new("conn-1"),
                capability_code: code,
                status: SupplierApiCapabilityStatus::Disabled,
                constraint_snapshot: None,
            },
        )
        .unwrap()
    }

    /// 构造覆盖指定能力的采购确认测试夹具。
    fn covering_confirmation(capability: &SupplierApiCapability) -> BusinessCapabilityConfirmation {
        BusinessCapabilityConfirmation::new(
            format!("confirm-{}", capability.capability_code.as_str()),
            BusinessCapabilityConfirmationData {
                connection_id: SupplierApiConnectionId::new("conn-1"),
                capability_id: SupplierApiCapabilityId::new(capability.base.id.clone()),
                capability_code: capability.capability_code,
                requirement: BusinessCapabilityRequirement::Required,
                applicability_reference: None,
                evidence_references: vec![],
                reason_code: "REQUIRED".to_string(),
                connection_version: 1,
                capability_version: capability.base.version,
                operation_id: "operation-1".to_string(),
                idempotency_key_hash: "hash-1".to_string(),
                request_fingerprint: "fingerprint-1".to_string(),
                confirmed_by: "buyer-1".to_string(),
                confirmed_at: Instant::from_unix_secs(1),
            },
        )
        .unwrap()
    }

    #[test]
    fn apply_validated_changes_splits_updates_and_creates() {
        let mut capabilities = vec![existing_capability(SupplierApiCapabilityCode::Order)];
        let confirmations = vec![covering_confirmation(&capabilities[0])];
        let classified = CapabilityChangeSet::new(
            vec![
                CapabilityChangeInput {
                    code: SupplierApiCapabilityCode::Order,
                    enabled: true,
                    constraint_snapshot: None,
                },
                CapabilityChangeInput {
                    code: SupplierApiCapabilityCode::Product,
                    enabled: false,
                    constraint_snapshot: None,
                },
            ],
            &BTreeMap::from([("order".to_string(), 1_u64), ("product".to_string(), 0_u64)]),
        )
        .unwrap()
        .classify(&capabilities)
        .unwrap();

        let (updates, creates) =
            apply_validated_changes("conn-1", &classified, &confirmations, &mut capabilities).unwrap();
        assert_eq!(updates.len(), 1);
        assert!(updates[0].is_active());
        assert_eq!(creates.len(), 1);
        assert_eq!(creates[0].capability_code, SupplierApiCapabilityCode::Product);
        assert!(!creates[0].is_active());
    }

    #[test]
    fn apply_validated_changes_rejects_version_conflict_and_missing_confirmation() {
        let mut capabilities = vec![existing_capability(SupplierApiCapabilityCode::Order)];
        let classified = CapabilityChangeSet::new(
            vec![CapabilityChangeInput {
                code: SupplierApiCapabilityCode::Order,
                enabled: true,
                constraint_snapshot: None,
            }],
            &BTreeMap::from([("order".to_string(), 1_u64)]),
        )
        .unwrap()
        .classify(&capabilities)
        .unwrap();

        assert!(matches!(
            apply_validated_changes("conn-1", &classified, &[], &mut capabilities.clone()),
            Err(Error::BusinessLogicError(_))
        ));

        capabilities[0].base.version = 99;
        assert!(matches!(
            apply_validated_changes(
                "conn-1",
                &classified,
                &[covering_confirmation(&existing_capability(
                    SupplierApiCapabilityCode::Order
                ))],
                &mut capabilities
            ),
            Err(Error::ConflictError(_))
        ));
    }
}
