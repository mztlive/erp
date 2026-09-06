use bpm::ids::ApprovalProcessDefinitionId;
use bpm::model::types::ModelError;
use bpm::model::{ApprovalCommandReceipt, Timestamp};
use database::repository::bpm::DefinitionGraph;
use database::{BpmExt, DocumentRegistryExt};
use entities::document_registry::business_document::{
    ApprovalBindingUpgradeError, ApprovalBindingUpgradeInput, ApprovalDefinitionBinding,
};
use entities::document_registry::workflow_action::ApprovalBindingActionContext;
use entities::document_registry::{
    BusinessDocument, BusinessDocumentId, WorkflowAction, WorkflowActionData, WorkflowActionType,
};
use erp_audit::AuditExt;
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;

use crate::errors::{Error, Result};
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_identity::SharedRbacService;

use super::super::execution::idempotency::{payload_conflict_error, ReceiptBranch};
use super::super::execution::{
    map_receipt_first_write_error, upgrade_binding_identity, PreparedCommandIdentity,
};
use super::super::policy::{require_process_required, ProcessRequiredApprovalPolicy};
use super::super::scope::{
    approval_actor_is_active_with_executor, approval_binding_upgrade_authorization_with_executor,
};
use super::super::upgrade_subject::{
    ensure_initial_unsubmitted_approval_upgrade_subject, load_approval_upgrade_subject_facts,
    ApprovalUpgradeSubjectFacts,
};
use super::bind::{load_published_graph, load_registered_document};
use super::revalidate::revalidate_binding_graph;
use super::types::{
    AuthorizedUpgradeContext, UpgradeBindingOutcome, UpgradeBindingResultView, UpgradeBindingView,
    UpgradeUnsubmittedDefinitionCommand,
};
use super::DEFINITION_UPGRADED_AUDIT_ACTION;

/// 升级未提交且未启动单据的绑定到当前发布版本。
///
/// 目标固定为当前唯一 `PUBLISHED` 定义，禁止客户端提交定义 ID。
/// 本端口不开事务；运行层必须传入同一外层事务执行器。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `command` - 精确对象、强版本、绑定 CAS、原因和预构造幂等身份
/// * `actor` - 已认证操作人；仍需在事务内重验账号与授权
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// Fresh 返回从新动作投影的 `Applied`；同载荷收据返回从原动作
/// 严格重建的 `Replay`。
///
/// # 错误
/// 身份、授权、收据、动作、强实体、注册行、定义图或人员重验失败时
/// 返回错误。任何收据冲突之外的 duplicate 不得进入恢复。
pub async fn upgrade_unsubmitted_document_definition(
    db: &Database,
    rbac: &SharedRbacService,
    command: &UpgradeUnsubmittedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<UpgradeBindingResultView> {
    let reason = normalized_upgrade_reason(command)?;
    ensure_prepared_upgrade_identity(command, actor, reason)?;

    let authorized = load_authorized_upgrade_context(db, rbac, command, actor, executor).await?;

    let receipt = find_upgrade_receipt(db, &command.identity, executor).await?;
    match command.identity.classify(receipt.as_ref()) {
        ReceiptBranch::SamePayload(receipt) => {
            return replay_upgrade_result(db, command, actor, reason, receipt, executor).await;
        }
        ReceiptBranch::PayloadConflict => return Err(payload_conflict_error()),
        ReceiptBranch::Fresh => {}
    }

    authorized
        .facts
        .ensure_expected_business_object_version(command.expected_business_object_version)?;
    ensure_initial_unsubmitted_approval_upgrade_subject(db, &authorized.facts, executor).await?;
    apply_fresh_upgrade(
        db,
        rbac,
        ApplyFreshUpgradeInput {
            command,
            actor,
            reason,
            facts: &authorized.facts,
            policy: &authorized.policy,
            actor_role: &authorized.actor_role,
        },
        executor,
    )
    .await
}

/// 在 unknown/duplicate 恢复的新事务中只执行授权回读。
///
/// # 返回
/// 同载荷 V3 收据及其不可变动作完整时返回 `Some(Replay)`；收据尚不存在
/// 返回 `None`。
///
/// # 错误
/// 强业务身份、当前账号、三重授权、policy revision、收据载荷或动作证明
/// 任一失败时返回错误。
///
/// # 关键业务约束
/// 本端口绝不调用 Fresh 门禁与任何写入；运行层必须为每次恢复尝试传入可用的
/// 新事务执行器。
pub async fn replay_unsubmitted_document_definition_upgrade(
    db: &Database,
    rbac: &SharedRbacService,
    command: &UpgradeUnsubmittedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<UpgradeBindingResultView>> {
    let reason = normalized_upgrade_reason(command)?;
    ensure_prepared_upgrade_identity(command, actor, reason)?;
    let _authorized = load_authorized_upgrade_context(db, rbac, command, actor, executor).await?;
    let receipt = find_upgrade_receipt(db, &command.identity, executor).await?;
    match command.identity.classify(receipt.as_ref()) {
        ReceiptBranch::SamePayload(receipt) => {
            replay_upgrade_result(db, command, actor, reason, receipt, executor)
                .await
                .map(Some)
        }
        ReceiptBranch::PayloadConflict => Err(payload_conflict_error()),
        ReceiptBranch::Fresh => Ok(None),
    }
}

/// 得到命令签署与不可变动作共用的规范化原因。
fn normalized_upgrade_reason(command: &UpgradeUnsubmittedDefinitionCommand) -> Result<&str> {
    let reason = command.reason.trim();
    if reason.is_empty() {
        return Err(Error::ValidationError("升级原因不能为空".to_string()));
    }
    if command.action_id.as_ref().trim().is_empty() || command.receipt_id.as_ref().trim().is_empty() {
        return Err(Error::Internal("绑定升级预生成 ID 不完整".to_string()));
    }
    Ok(reason)
}

/// 先加载精确强事实，再重验账号与当前三重授权。
async fn load_authorized_upgrade_context(
    db: &Database,
    rbac: &SharedRbacService,
    command: &UpgradeUnsubmittedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<AuthorizedUpgradeContext> {
    let facts =
        load_approval_upgrade_subject_facts(db, command.document_type, &command.document_id, executor)
            .await?;
    ensure_active_upgrade_actor(db, actor, executor).await?;
    let policy = require_process_required(facts.document_type)?;
    let authorization = approval_binding_upgrade_authorization_with_executor(
        db,
        rbac,
        actor,
        facts.document_type,
        &policy.definition_admin_permission,
        &facts.responsible_org_id,
        executor,
    )
    .await?;
    Ok(AuthorizedUpgradeContext {
        facts,
        policy,
        actor_role: authorization.actor_role,
    })
}

/// 证明运行层传入的身份精确签署了本命令。
fn ensure_prepared_upgrade_identity(
    command: &UpgradeUnsubmittedDefinitionCommand,
    actor: &AuditActor,
    reason: &str,
) -> Result<()> {
    let expected = upgrade_binding_identity(
        command.document_type.as_str(),
        &command.document_id,
        command.expected_business_object_version,
        command.expected_binding_version,
        reason,
        actor.id(),
        command.identity.idempotency_key().clone(),
    )?;
    let scopes = command.identity.scope_candidates();
    if command.identity.current() != expected.current()
        || scopes.len() != 1
        || scopes.first().copied() != Some(command.identity.current().scope().as_str())
    {
        return Err(payload_conflict_error());
    }
    Ok(())
}

/// 在强业务对象存在性已证明后，事务内重验操作人。
async fn ensure_active_upgrade_actor(
    db: &Database,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    if approval_actor_is_active_with_executor(db, actor, executor).await? {
        return Ok(());
    }
    Err(Error::Forbidden(
        "审批绑定升级账号不存在、已停用或身份已变化".to_string(),
    ))
}

/// 只按预构造 V3 身份查找绑定升级收据。
async fn find_upgrade_receipt(
    db: &Database,
    identity: &PreparedCommandIdentity,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalCommandReceipt>> {
    db.bpm_workflow()
        .find_command_receipt(
            identity.current().command_kind(),
            identity.current().scope().as_str(),
            identity.idempotency_key(),
            executor,
        )
        .await
        .map_err(Into::into)
}

/// 同载荷收据只能回读并严格证明其不可变动作。
async fn replay_upgrade_result(
    db: &Database,
    command: &UpgradeUnsubmittedDefinitionCommand,
    actor: &AuditActor,
    reason: &str,
    receipt: &ApprovalCommandReceipt,
    executor: &mut dyn Executor,
) -> Result<UpgradeBindingResultView> {
    let action = db
        .workflow_actions()
        .find_by_id(&receipt.result_ref, executor)
        .await?
        .ok_or_else(payload_conflict_error)?;
    upgrade_result_from_action(
        command,
        actor,
        reason,
        receipt,
        &action,
        UpgradeBindingOutcome::Replay,
    )
}

/// Fresh 升级写库所需的命令、授权事实与操作人。
///
/// # 用途
/// 打包 [`apply_fresh_upgrade`] 的非基础设施参数。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 调用方须已完成收据分支判定与业务对象版本门禁。
struct ApplyFreshUpgradeInput<'a> {
    /// 已规范化的升级命令。
    command: &'a UpgradeUnsubmittedDefinitionCommand,
    /// 审计操作人。
    actor: &'a AuditActor,
    /// 已规范化升级原因。
    reason: &'a str,
    /// 升级主体强业务事实。
    facts: &'a ApprovalUpgradeSubjectFacts,
    /// 单据审批政策。
    policy: &'a ProcessRequiredApprovalPolicy,
    /// 操作人角色快照。
    actor_role: &'a str,
}

/// Fresh 分支完成全部读取与预构造后，以收据作为第一物理写。
///
/// # 用途
/// 在唯一事务内校验并写入绑定升级收据、单据、动作与审计。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `input` - 命令、主体事实与操作人
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 升级成功后的绑定结果视图。
///
/// # 错误
/// 主体不匹配、定义未变更、CAS 冲突或写入失败时返回错误。
///
/// # 关键业务约束
/// 收据必须作为第一物理写；后续失败随事务整体回滚。
async fn apply_fresh_upgrade(
    db: &Database,
    rbac: &SharedRbacService,
    input: ApplyFreshUpgradeInput<'_>,
    executor: &mut dyn Executor,
) -> Result<UpgradeBindingResultView> {
    let ApplyFreshUpgradeInput {
        command,
        actor,
        reason,
        facts,
        policy,
        actor_role,
    } = input;
    let mut document = load_registered_document(db, &command.document_id, executor).await?;
    ensure_registered_upgrade_subject(&document, facts)?;
    document
        .ensure_unsubmitted_approval_binding_upgrade(command.expected_binding_version, reason)
        .map_err(map_binding_upgrade_error)?;
    let previous = document
        .approval_binding
        .clone()
        .ok_or_else(|| Error::ValidationError("尚未绑定审批定义".to_string()))?;

    let published = load_published_graph(db, facts.document_type, executor).await?;
    revalidate_binding_graph(db, rbac, policy, &facts.binding_context(), &published, executor).await?;
    ensure_upgrade_changes_definition(&previous, &published)?;

    let current_definition_id = ApprovalProcessDefinitionId::new(published.definition.base.id.clone());
    let current_binding_version = previous
        .approval_binding_version
        .checked_add(1)
        .ok_or_else(|| Error::Internal("审批绑定版本溢出".to_string()))?;
    let action = WorkflowAction::new_with_approval_binding_context(
        command.action_id.clone(),
        WorkflowActionData {
            document_id: BusinessDocumentId::new(facts.document_id.clone()),
            action_type: WorkflowActionType::ApprovalDefinitionUpgraded,
            from_status: "DRAFT".to_string(),
            to_status: "DRAFT".to_string(),
            actor_id: actor.id().to_string(),
            actor_role: actor_role.to_string(),
            comment: Some(reason.to_string()),
        },
        ApprovalBindingActionContext {
            previous_definition_id: previous.approval_process_definition_id.clone(),
            previous_definition_version: previous.approval_definition_version,
            previous_binding_version: previous.approval_binding_version,
            current_definition_id: current_definition_id.clone(),
            current_definition_version: published.definition.definition_version,
            current_binding_version,
            business_object_version: facts.business_object_version,
        },
    )?;
    let action_at = action_timestamp(&action)?;
    document
        .upgrade_unsubmitted_approval_binding(ApprovalBindingUpgradeInput {
            approval_process_definition_id: current_definition_id,
            approval_definition_version: published.definition.definition_version,
            expected_binding_version: command.expected_binding_version,
            reason,
            at: action_at,
        })
        .map_err(map_binding_upgrade_error)?;
    ensure_action_matches_upgraded_document(&action, &document)?;

    let receipt = ApprovalCommandReceipt::new(
        command.receipt_id.clone(),
        command.identity.current(),
        action.base.id.clone(),
        Timestamp::from_utc(action_at.as_utc()),
    )
    .map_err(map_model_error)?;
    let audit = upgraded_binding_audit(actor, facts, &previous, &action)?;
    let view = upgrade_result_from_action(
        command,
        actor,
        reason,
        &receipt,
        &action,
        UpgradeBindingOutcome::Applied,
    )?;

    db.bpm_workflow()
        .insert_command_receipt(&receipt, executor)
        .await
        .map_err(map_receipt_first_write_error)?;
    db.business_documents().update(&mut document, executor).await?;
    db.workflow_actions().create(&action, executor).await?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(view)
}

/// 注册投影必须与同一事务已读取的强业务事实精确一致。
pub(super) fn ensure_registered_upgrade_subject(
    document: &BusinessDocument,
    facts: &ApprovalUpgradeSubjectFacts,
) -> Result<()> {
    let conflicting_document_no = !document.document_no.is_empty()
        && !facts.document_no.is_empty()
        && document.document_no != facts.document_no;
    if document.base.id != facts.document_id
        || document.document_type != facts.document_type
        || conflicting_document_no
    {
        return Err(Error::ConflictError(
            "业务单据注册事实与强业务对象不一致".to_string(),
        ));
    }
    Ok(())
}

/// 升级目标必须是不同且更高的发布定义。
fn ensure_upgrade_changes_definition(
    previous: &ApprovalDefinitionBinding,
    published: &DefinitionGraph,
) -> Result<()> {
    if previous.approval_process_definition_id.as_ref() == published.definition.base.id
        || published.definition.definition_version <= previous.approval_definition_version
    {
        return Err(Error::ConflictError(
            "当前绑定已是最新发布定义，禁止空升级或降级".to_string(),
        ));
    }
    Ok(())
}

/// 将不可变动作创建时间转为绑定与收据共用时间。
fn action_timestamp(action: &WorkflowAction) -> Result<Instant> {
    let secs = i64::try_from(action.base.created_at)
        .map_err(|_| Error::Internal("工作流动作时间无法转换为绑定时间".to_string()))?;
    Ok(Instant::from_unix_secs(secs))
}

/// 写入前证明内存中的注册绑定与不可变动作完全一致。
fn ensure_action_matches_upgraded_document(
    action: &WorkflowAction,
    document: &BusinessDocument,
) -> Result<()> {
    let context = action
        .approval_binding_context
        .as_ref()
        .ok_or_else(|| Error::Internal("绑定升级动作缺少结构化上下文".to_string()))?;
    let binding = document
        .approval_binding
        .as_ref()
        .ok_or_else(|| Error::Internal("升级后绑定丢失".to_string()))?;
    if binding.approval_process_definition_id != context.current_definition_id
        || binding.approval_definition_version != context.current_definition_version
        || binding.approval_binding_version != context.current_binding_version
        || binding.approval_definition_bound_at != action_timestamp(action)?
    {
        return Err(Error::Internal("绑定升级动作与注册绑定不一致".to_string()));
    }
    Ok(())
}

/// 在首笔写入前构造升级审计。
fn upgraded_binding_audit(
    actor: &AuditActor,
    facts: &ApprovalUpgradeSubjectFacts,
    previous: &ApprovalDefinitionBinding,
    action: &WorkflowAction,
) -> Result<erp_audit::AuditLog> {
    let context = action
        .approval_binding_context
        .as_ref()
        .ok_or_else(|| Error::Internal("绑定升级动作缺少结构化上下文".to_string()))?;
    let message = format!(
        "document_type={} business_object_version={} from_definition={} from_version={} to_definition={} to_version={} action_id={}",
        facts.document_type.as_str(),
        facts.business_object_version,
        previous.approval_process_definition_id.as_ref(),
        previous.approval_definition_version,
        context.current_definition_id.as_ref(),
        context.current_definition_version,
        action.base.id,
    );
    actor
        .clone()
        .resource_log_with_message(
            DEFINITION_UPGRADED_AUDIT_ACTION,
            "business_document",
            facts.document_id.clone(),
            Some(message),
        )
        .map_err(Into::into)
}

/// 从收据指向的动作严格证明并重建绑定升级结果。
pub(super) fn upgrade_result_from_action(
    command: &UpgradeUnsubmittedDefinitionCommand,
    actor: &AuditActor,
    reason: &str,
    receipt: &ApprovalCommandReceipt,
    action: &WorkflowAction,
    outcome: UpgradeBindingOutcome,
) -> Result<UpgradeBindingResultView> {
    if !matches!(
        command.identity.classify(Some(receipt)),
        ReceiptBranch::SamePayload(_)
    ) {
        return Err(payload_conflict_error());
    }
    let expected_current_binding_version = command
        .expected_binding_version
        .checked_add(1)
        .ok_or_else(payload_conflict_error)?;
    let Some(context) = action.approval_binding_context.as_ref() else {
        return Err(payload_conflict_error());
    };
    let immutable_metadata = action.base.version == 1
        && action.base.deleted_at == 0
        && action.base.created_at == action.base.updated_at
        && receipt.base.version == 1
        && receipt.base.deleted_at == 0
        && receipt.base.created_at == receipt.base.updated_at
        && receipt.base.created_at == action.base.created_at;
    let exact_action = !action.base.id.trim().is_empty()
        && receipt.result_ref == action.base.id
        && action.document_id.as_ref() == command.document_id
        && action.action_type == WorkflowActionType::ApprovalDefinitionUpgraded
        && action.from_status == "DRAFT"
        && action.to_status == "DRAFT"
        && action.actor_id == actor.id()
        && !action.actor_role.trim().is_empty()
        && action.comment.as_deref() == Some(reason)
        && action.approval_context.is_none();
    let exact_context = context.business_object_version == command.expected_business_object_version
        && context.business_object_version > 0
        && context.previous_binding_version == command.expected_binding_version
        && context.current_binding_version == expected_current_binding_version
        && context.previous_binding_version > 0
        && context.previous_definition_version > 0
        && context.current_definition_version > context.previous_definition_version
        && !context.previous_definition_id.as_ref().trim().is_empty()
        && !context.current_definition_id.as_ref().trim().is_empty()
        && context.previous_definition_id != context.current_definition_id;
    if !immutable_metadata || !exact_action || !exact_context {
        return Err(payload_conflict_error());
    }
    Ok(UpgradeBindingResultView {
        document_type: command.document_type,
        document_id: command.document_id.clone(),
        original_business_object_version: context.business_object_version.to_string(),
        new_binding: UpgradeBindingView {
            approval_process_definition_id: context.current_definition_id.to_string(),
            approval_definition_version: context.current_definition_version,
            approval_binding_version: context.current_binding_version.to_string(),
            approval_definition_bound_at: action_timestamp(action).map_err(|_| payload_conflict_error())?,
        },
        action_id: action.base.id.clone(),
        outcome,
    })
}

/// 将 ERP 单据绑定升级错误映射为稳定的 Service 错误语义。
///
/// # 参数
/// * `error` - 实体层返回的升级失败原因
///
/// # 返回
/// 返回保持验证错误、冲突错误与实体不变量分类的 Service 错误。
///
/// # 错误
/// 无；本方法只执行错误分类转换。
///
/// # 关键业务约束
/// 已提交、已启动与双 CAS 失败必须保持冲突语义，缺失绑定和空原因保持校验语义。
fn map_binding_upgrade_error(error: ApprovalBindingUpgradeError) -> Error {
    match error {
        ApprovalBindingUpgradeError::MissingBinding => Error::ValidationError("尚未绑定审批定义".to_string()),
        ApprovalBindingUpgradeError::Formalized => {
            Error::ConflictError("已提交单据不能升级审批绑定".to_string())
        }
        ApprovalBindingUpgradeError::ApprovalStarted => {
            Error::ConflictError("已启动单据不能升级审批绑定".to_string())
        }
        ApprovalBindingUpgradeError::VersionConflict => {
            Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string())
        }
        ApprovalBindingUpgradeError::EmptyReason => Error::ValidationError("升级原因不能为空".to_string()),
        ApprovalBindingUpgradeError::BindingInvariant(error) => Error::Logic(error),
    }
}

/// 映射 BPM 模型错误。
pub(super) fn map_model_error(error: ModelError) -> Error {
    match error {
        ModelError::InvalidField(message) | ModelError::InvalidTransition(message) => {
            Error::ValidationError(message.to_string())
        }
        other => Error::ValidationError(other.to_string()),
    }
}
