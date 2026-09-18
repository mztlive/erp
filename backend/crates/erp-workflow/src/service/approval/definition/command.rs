use std::time::Duration;

use application_core::AuditActor;
use bpm::graph::DefinitionGraph;
use bpm::ids::{ApprovalCommandReceiptId, ApprovalProcessDefinitionId};
use bpm::model::types::{ApprovalCommandKind, ApprovalDefinitionStatus, ModelError};
use bpm::model::{
    ApprovalCommandIdentity, ApprovalCommandReceipt, ApprovalNodeDefinition, ApprovalProcessDefinition,
    CanonicalCommandPayload, CommandPayloadField, IdempotencyKey,
};
use bpm::{ParticipantId, Timestamp};
use chrono::Utc;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use sha2::{Digest, Sha256};

use super::super::definition_dto::{DefinitionDetailView, DefinitionNodeRequest, DraftSource};
use super::super::policy::{ProcessRequiredApprovalPolicy, require_process_required};
use super::super::process_kind::{document_type_of, process_kind_of};
use super::super::scope::definition_management_visibility_with_executor;
use super::ApprovalDefinitionService;
use super::mapping::detail_view;
use crate::entity::document_registry::DocumentType;
use crate::error::{Error, ErrorCode, Result};
use crate::ports::PreparedWorkflowAudit;
use crate::repository::BpmExt;
use crate::repository::bpm::{APPROVAL_COMMAND_RECEIPT_IDEMPOTENCY_INDEX, CasWriteOutcome};

const CREATE_DRAFT_COMMAND_DOMAIN: &str = "approval.definition.create-draft";
const REPLACE_NODES_COMMAND_DOMAIN: &str = "approval.definition.replace-nodes";
pub(super) const PUBLISH_DEFINITION_COMMAND_DOMAIN: &str = "approval.definition.publish";
pub(super) const RETIRE_DEFINITION_COMMAND_DOMAIN: &str = "approval.definition.retire";
const DEFINITION_RESULT_REF_PREFIX: &str = "definition-v1:";
const DEFINITION_COMMAND_RECOVERY_ATTEMPTS: usize = 8;

/// 当前 v3 命令身份与唯一允许查询的旧格式候选。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PreparedDefinitionIdentity {
    pub(super) current: ApprovalCommandIdentity,
    pub(super) legacy: Option<LegacyDefinitionIdentity>,
}

/// 历史定义命令的精确 scope/digest 配对与结果证明方式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LegacyDefinitionIdentity {
    command_kind: ApprovalCommandKind,
    scope_id: String,
    payload_digest: String,
    proof: LegacyDefinitionProof,
}

/// 旧收据只有能由结果中的不可变事实证明完整载荷时才可回放。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LegacyDefinitionProof {
    /// 创建来源不是不可变结果事实，必须失败关闭。
    Unprovable,
    /// 发布结果可由定义身份、发布人、状态和锁版本证明。
    Published { definition_id: String, expected_lock: u64, actor_id: String },
    /// 退役结果可由定义身份、退役人、状态和锁版本证明。
    Retired { definition_id: String, expected_lock: u64, actor_id: String },
}

/// v3 收据中版本化、可校验的定义结果引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DefinitionCommandResultRef {
    definition_id: String,
    definition_lock_version: u64,
}

/// 回放时必须证明结果仍属于命令原始资源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DefinitionResultExpectation {
    ProcessKind(bpm::ProcessKind),
    DefinitionId(String),
}

impl<A: crate::ports::WorkflowAuthorizationPort> ApprovalDefinitionService<A> {
    /// 校验操作人具备该类型定义管理权。
    ///
    /// # 错误
    /// 缺少类型级权限时返回禁止。
    pub(crate) async fn ensure_definition_admin(
        &self,
        actor: &AuditActor,
        policy: &ProcessRequiredApprovalPolicy,
    ) -> Result<()> {
        ensure_definition_admin_permission(&self.auth, actor, policy, &mut NoTransaction).await
    }

    /// 读取定义图，缺失时失败关闭。
    ///
    /// # 错误
    /// 定义不存在时返回未找到。
    pub(super) async fn require_graph(&self, definition_id: &str) -> Result<DefinitionGraph> {
        self.db
            .bpm_workflow()
            .load_definition_graph(
                &ApprovalProcessDefinitionId::new(definition_id.to_string()),
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(definition_not_found)
    }

    /// 单一回放入口：按策略表先查当前 V3，再查已登记历史候选。
    ///
    /// 调用方只传期望结果约束；身份证明规则集中在
    /// `replay_prepared_definition_receipt` 一处（当前 V3 与历史候选顺序固定）。
    ///
    /// # 参数
    /// * `identity` - 当前 V3 身份与唯一历史候选
    /// * `expectation` - 回放结果仍属原始资源的期望约束
    ///
    /// # 返回
    /// 同载荷收据回读详情；无收据返回 `None`。
    ///
    /// # 错误
    /// 异载荷、结果引用漂移或旧结果无法证明完整载荷时返回稳定冲突。
    pub(super) async fn replay_prepared_if_receipt(
        &self,
        identity: &PreparedDefinitionIdentity,
        expectation: DefinitionResultExpectation,
    ) -> Result<Option<DefinitionDetailView>> {
        replay_prepared_definition_receipt(&self.db, identity, &expectation, &mut NoTransaction).await
    }

    /// 收据竞争、瞬态事务或提交结果未知后，在新会话有限回读胜者。
    ///
    /// # 错误
    /// 找不到胜者时返回原错误；回放前当前定义管理权失效时返回禁止。
    pub(super) async fn recover_definition_command(
        &self,
        outcome: Result<DefinitionDetailView>,
        policy: &ProcessRequiredApprovalPolicy,
        actor: &AuditActor,
        identity: PreparedDefinitionIdentity,
        expectation: DefinitionResultExpectation,
    ) -> Result<DefinitionDetailView> {
        let Err(error) = outcome else {
            return outcome;
        };
        if !definition_command_may_have_committed(&error) {
            return Err(error);
        }
        for attempt in 0..DEFINITION_COMMAND_RECOVERY_ATTEMPTS {
            let db = self.db.clone();
            let rbac = self.auth.clone();
            let actor = actor.clone();
            let policy = policy.clone();
            let identity = identity.clone();
            let expectation = expectation.clone();
            let recovered = self
                .db
                .client()
                .with_transaction(move |executor| {
                    Box::pin(async move {
                        ensure_definition_admin_permission(&rbac, &actor, &policy, executor).await?;
                        replay_prepared_definition_receipt(&db, &identity, &expectation, executor).await
                    })
                })
                .await;
            match recovered {
                Ok(Some(view)) => return Ok(view),
                Ok(None) => {},
                Err(recovery_error) if definition_command_may_have_committed(&recovery_error) => {},
                Err(recovery_error) => return Err(recovery_error),
            }
            if attempt + 1 < DEFINITION_COMMAND_RECOVERY_ATTEMPTS {
                tokio::time::sleep(definition_recovery_delay(attempt)).await;
            }
        }
        Err(error)
    }
}

/// 事务内若已有同载荷 v3 收据则回读仍未漂移的结果版本。
async fn replay_current_receipt(
    db: &Database,
    identity: &ApprovalCommandIdentity,
    expectation: &DefinitionResultExpectation,
    session: &mut dyn Executor,
) -> Result<Option<DefinitionDetailView>> {
    let Some(receipt) = db
        .bpm_workflow()
        .find_command_receipt(
            identity.command_kind(),
            identity.scope().as_str(),
            identity.idempotency_key(),
            session,
        )
        .await?
    else {
        return Ok(None);
    };
    receipt.reconcile_identity(identity).map_err(map_model_error)?;
    let result_ref = DefinitionCommandResultRef::parse(&receipt.result_ref)?;
    let graph = db
        .bpm_workflow()
        .load_definition_graph(&ApprovalProcessDefinitionId::new(result_ref.definition_id.clone()), session)
        .await?
        .ok_or_else(idempotency_payload_conflict)?;
    result_ref.ensure_matches(&graph, expectation)?;
    Ok(Some(detail_view(&graph)))
}

/// 单一回放策略表（当前 V3 → 已登记历史候选），各命令入口共用。
///
/// 事务内先读取 v3 收据；仅在 v3 不存在时读取命令声明的精确旧格式候选。
/// 当前格式一旦存在即由 `reconcile_identity` 决定回放或冲突，不得降级至旧格式。
pub(super) async fn replay_prepared_definition_receipt(
    db: &Database,
    identity: &PreparedDefinitionIdentity,
    expectation: &DefinitionResultExpectation,
    session: &mut dyn Executor,
) -> Result<Option<DefinitionDetailView>> {
    if let Some(view) = replay_current_receipt(db, &identity.current, expectation, session).await? {
        return Ok(Some(view));
    }
    let Some(legacy) = identity.legacy.as_ref() else {
        return Ok(None);
    };
    replay_legacy_receipt(db, identity.current.idempotency_key(), legacy, session).await
}

/// 精确读取一个旧 scope/digest 候选，并以不可变结果事实证明完整载荷。
async fn replay_legacy_receipt(
    db: &Database,
    key: &IdempotencyKey,
    legacy: &LegacyDefinitionIdentity,
    session: &mut dyn Executor,
) -> Result<Option<DefinitionDetailView>> {
    let Some(receipt) =
        db.bpm_workflow().find_command_receipt(legacy.command_kind, &legacy.scope_id, key, session).await?
    else {
        return Ok(None);
    };
    receipt.reconcile(&legacy.payload_digest).map_err(map_model_error)?;
    let graph = db
        .bpm_workflow()
        .load_definition_graph(&ApprovalProcessDefinitionId::new(receipt.result_ref.clone()), session)
        .await?
        .ok_or_else(idempotency_payload_conflict)?;
    ensure_legacy_result(&receipt, &graph, &legacy.proof)?;
    Ok(Some(detail_view(&graph)))
}

/// 写入命令收据。
pub(super) async fn write_receipt(
    db: &Database,
    identity: &ApprovalCommandIdentity,
    result_ref: &str,
    session: &mut dyn Executor,
) -> Result<()> {
    let receipt =
        ApprovalCommandReceipt::new(ApprovalCommandReceiptId::new(next_id()), identity, result_ref, now()?)
            .map_err(map_model_error)?;
    db.bpm_workflow().insert_command_receipt(&receipt, session).await.map_err(mark_receipt_duplicate)
}

/// 写入定义变更审计。
pub(super) async fn write_definition_audit(
    audit_port: &dyn crate::ports::WorkflowAuditPort,
    actor: &AuditActor,
    action: &str,
    graph: &DefinitionGraph,
    expected_lock: Option<u64>,
    extra: Option<&str>,
    session: &mut dyn Executor,
) -> Result<()> {
    let document_type = document_type_of(graph.definition.process_kind);
    let message = format!(
        "document_type={} version={} expected_lock={:?} actual_lock={} nodes={} extra={:?}",
        document_type.as_str(),
        graph.definition.definition_version,
        expected_lock,
        graph.definition.definition_lock_version(),
        node_summary(&graph.nodes),
        extra
    );
    let audit = PreparedWorkflowAudit::resource_with_message(
        actor.clone(),
        action,
        "approval_process_definition",
        graph.definition.base.id.clone(),
        Some(message),
    )?;
    audit_port.persist(&audit, session).await?;
    Ok(())
}

/// 取出 CAS 成功后的定义。
pub(super) fn applied_definition(
    outcome: CasWriteOutcome<ApprovalProcessDefinition>,
) -> Result<ApprovalProcessDefinition> {
    match outcome {
        CasWriteOutcome::Applied(definition) => Ok(definition),
        CasWriteOutcome::VersionConflict(_) => Err(stale_lock_error()),
        CasWriteOutcome::StatusChanged(_) => {
            Err(Error::from_approval_code(ErrorCode::ApprovalDefinitionNotDraft))
        },
        CasWriteOutcome::NotFound => Err(definition_not_found()),
    }
}

/// 校验定义锁版本并映射陈旧锁错误。
///
/// # 参数
/// * `definition` - 当前定义快照
/// * `expected` - 调用方期望的锁版本
///
/// # 返回
/// 锁版本一致时返回 `Ok(())`。
///
/// # 错误
/// 锁版本不一致时返回“未写入任何节点”的稳定冲突错误。
///
/// # 关键业务约束
/// 不得使用业务版本替代 `definition_lock_version`。
pub(super) fn ensure_lock(definition: &ApprovalProcessDefinition, expected: u64) -> Result<()> {
    definition.ensure_lock_version(expected).map_err(|_| stale_lock_error())
}

/// 由定义的流程种类读取必须审批政策。
pub(super) fn policy_for_definition(
    definition: &ApprovalProcessDefinition,
) -> Result<ProcessRequiredApprovalPolicy> {
    require_process_required(document_type_of(definition.process_kind))
}

/// 节点摘要，供审计使用。
pub(super) fn node_summary(nodes: &[ApprovalNodeDefinition]) -> String {
    nodes.iter().map(|node| format!("{}:{}", node.display_order, node.node_key)).collect::<Vec<_>>().join(",")
}

/// 在任何 Repository 读取前把外部幂等键转换为规范值对象。
pub(super) fn parse_idempotency_key(raw: &str) -> Result<IdempotencyKey> {
    IdempotencyKey::parse(raw.to_string()).map_err(|error| match error {
        ModelError::InvalidField(message) => Error::ValidationError(message.to_string()),
        other => Error::Internal(format!("审批命令幂等键构造失败: {other}")),
    })
}

/// 创建草稿的 v3 身份与唯一旧格式候选。
pub(super) fn create_draft_identity(
    key: IdempotencyKey,
    document_type: DocumentType,
    name: &str,
    draft_source: DraftSource,
    actor_id: &str,
) -> Result<PreparedDefinitionIdentity> {
    let process_kind = process_kind_of(document_type);
    let current = ApprovalCommandIdentity::new(
        ApprovalCommandKind::DefinitionWrite,
        CREATE_DRAFT_COMMAND_DOMAIN,
        key,
        CanonicalCommandPayload::new().field(CommandPayloadField::Text(process_kind.as_str())),
        CanonicalCommandPayload::new()
            .field(CommandPayloadField::Text(document_type.as_str()))
            .field(CommandPayloadField::Text(name))
            .field(CommandPayloadField::Text(draft_source.as_str()))
            .field(CommandPayloadField::Text(actor_id)),
    )
    .map_err(map_identity_error)?;
    Ok(PreparedDefinitionIdentity {
        current,
        legacy: Some(LegacyDefinitionIdentity {
            command_kind: ApprovalCommandKind::DefinitionWrite,
            scope_id: process_kind.as_str().to_string(),
            payload_digest: legacy_payload_digest(&[
                document_type.as_str(),
                name,
                draft_source.as_str(),
                actor_id,
            ]),
            proof: LegacyDefinitionProof::Unprovable,
        }),
    })
}

/// 整组节点替换的 v3 身份；Create/Replace 即使资源文本相同也使用不同 domain。
pub(super) fn replace_nodes_identity(
    key: IdempotencyKey,
    definition_id: &str,
    expected_lock: u64,
    nodes: &[DefinitionNodeRequest],
    actor_id: &str,
) -> Result<PreparedDefinitionIdentity> {
    let node_fields = nodes
        .iter()
        .map(|node| {
            CommandPayloadField::Sequence(vec![
                CommandPayloadField::OptionalText(node.node_id.as_deref()),
                CommandPayloadField::Text(&node.node_name),
                CommandPayloadField::U32(node.display_order),
                CommandPayloadField::Text(&node.assignee_user_id),
            ])
        })
        .collect();
    let current = ApprovalCommandIdentity::new(
        ApprovalCommandKind::DefinitionWrite,
        REPLACE_NODES_COMMAND_DOMAIN,
        key,
        CanonicalCommandPayload::new().field(CommandPayloadField::Text(definition_id)),
        CanonicalCommandPayload::new()
            .field(CommandPayloadField::Text(definition_id))
            .field(CommandPayloadField::U64(expected_lock))
            .field(CommandPayloadField::Sequence(node_fields))
            .field(CommandPayloadField::Text(actor_id)),
    )
    .map_err(map_identity_error)?;
    Ok(PreparedDefinitionIdentity { current, legacy: None })
}

/// 发布或退役的 v3 身份与可证明旧格式候选。
pub(super) fn lock_command_identity(
    key: IdempotencyKey,
    command_kind: ApprovalCommandKind,
    domain: &'static str,
    definition_id: &str,
    expected_lock: u64,
    actor_id: &str,
) -> Result<PreparedDefinitionIdentity> {
    let current = ApprovalCommandIdentity::new(
        command_kind,
        domain,
        key,
        CanonicalCommandPayload::new().field(CommandPayloadField::Text(definition_id)),
        CanonicalCommandPayload::new()
            .field(CommandPayloadField::Text(definition_id))
            .field(CommandPayloadField::U64(expected_lock))
            .field(CommandPayloadField::Text(actor_id)),
    )
    .map_err(map_identity_error)?;
    let proof = match command_kind {
        ApprovalCommandKind::PublishDefinition => LegacyDefinitionProof::Published {
            definition_id: definition_id.to_string(),
            expected_lock,
            actor_id: actor_id.to_string(),
        },
        ApprovalCommandKind::RetireDefinition => LegacyDefinitionProof::Retired {
            definition_id: definition_id.to_string(),
            expected_lock,
            actor_id: actor_id.to_string(),
        },
        _ => {
            return Err(Error::Internal("定义锁命令种类不属于发布或退役".to_string()));
        },
    };
    Ok(PreparedDefinitionIdentity {
        current,
        legacy: Some(LegacyDefinitionIdentity {
            command_kind,
            scope_id: definition_id.to_string(),
            payload_digest: legacy_payload_digest(&[&expected_lock.to_string(), actor_id]),
            proof,
        }),
    })
}

/// 只用于精确读取旧收据候选的历史 U+001F 摘要。
fn legacy_payload_digest(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            hasher.update([0x1f]);
        }
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())
}

impl DefinitionCommandResultRef {
    /// 从尚未或已经持久化的图冻结定义身份与结果锁版本。
    pub(super) fn from_graph(graph: &DefinitionGraph) -> Self {
        Self {
            definition_id: graph.definition.base.id.clone(),
            definition_lock_version: graph.definition.definition_lock_version(),
        }
    }

    /// 使用长度定界 ID 编码版本化结果引用。
    pub(super) fn encode(&self) -> String {
        format!(
            "{DEFINITION_RESULT_REF_PREFIX}{}:{}:{}",
            self.definition_id.len(),
            self.definition_id,
            self.definition_lock_version
        )
    }

    /// 解析当前结果引用；旧 raw ID 不得被当前 v3 收据接受。
    fn parse(raw: &str) -> Result<Self> {
        let payload =
            raw.strip_prefix(DEFINITION_RESULT_REF_PREFIX).ok_or_else(idempotency_payload_conflict)?;
        let (length, payload) = payload.split_once(':').ok_or_else(idempotency_payload_conflict)?;
        let length = length.parse::<usize>().map_err(|_| idempotency_payload_conflict())?;
        if payload.len() <= length || !payload.is_char_boundary(length) {
            return Err(idempotency_payload_conflict());
        }
        let definition_id = &payload[..length];
        let version = payload[length..]
            .strip_prefix(':')
            .ok_or_else(idempotency_payload_conflict)?
            .parse::<u64>()
            .map_err(|_| idempotency_payload_conflict())?;
        if definition_id.is_empty() {
            return Err(idempotency_payload_conflict());
        }
        Ok(Self { definition_id: definition_id.to_string(), definition_lock_version: version })
    }

    /// 证明当前图仍是该命令冻结的精确结果版本与资源。
    fn ensure_matches(
        &self,
        graph: &DefinitionGraph,
        expectation: &DefinitionResultExpectation,
    ) -> Result<()> {
        let resource_matches = match expectation {
            DefinitionResultExpectation::ProcessKind(process_kind) => {
                graph.definition.process_kind == *process_kind
            },
            DefinitionResultExpectation::DefinitionId(definition_id) => {
                graph.definition.base.id == *definition_id
            },
        };
        if resource_matches
            && graph.definition.base.id == self.definition_id
            && graph.definition.definition_lock_version() == self.definition_lock_version
        {
            return Ok(());
        }
        Err(idempotency_payload_conflict())
    }
}

/// 当前调用方时间。
pub(super) fn now() -> Result<Timestamp> {
    Ok(Timestamp::from_utc(Utc::now()))
}

/// 构造处理人引用。
pub(super) fn participant(actor: &AuditActor) -> Result<ParticipantId> {
    ParticipantId::new(actor.id().to_string()).map_err(map_bpm_error)
}

/// 映射 BPM 模型错误。
pub(super) fn map_model_error(error: ModelError) -> Error {
    match error {
        ModelError::CommandReceiptConflict => {
            Error::from_approval_code(ErrorCode::ApprovalIdempotencyPayloadConflict)
        },
        ModelError::InvalidField(_) | ModelError::InvalidTransition(_) => {
            Error::from_approval_code(ErrorCode::ApprovalDefinitionInvalid)
        },
        ModelError::InvalidStatus(message) => Error::ConflictError(message.to_string()),
        ModelError::Overflow(message) => Error::BusinessLogicError(format!("计数溢出: {message}")),
        other => Error::BusinessLogicError(other.to_string()),
    }
}

/// 映射 BPM 边界错误。
pub(super) fn map_bpm_error(error: bpm::Error) -> Error {
    let _ = error;
    Error::from_approval_code(ErrorCode::ApprovalDefinitionInvalid)
}

/// v3 身份构造失败只可能来自服务端固定 domain，按内部错误处理。
fn map_identity_error(error: ModelError) -> Error {
    Error::Internal(format!("审批定义命令身份构造失败: {error}"))
}

/// 返回稳定幂等载荷冲突。
fn idempotency_payload_conflict() -> Error {
    Error::from_approval_code(ErrorCode::ApprovalIdempotencyPayloadConflict)
}

/// 证明旧收据结果完整对应历史载荷；不可证明时不得降级回放。
pub(super) fn ensure_legacy_result(
    receipt: &ApprovalCommandReceipt,
    graph: &DefinitionGraph,
    proof: &LegacyDefinitionProof,
) -> Result<()> {
    let definition = &graph.definition;
    let proven = match proof {
        LegacyDefinitionProof::Unprovable => false,
        LegacyDefinitionProof::Published { definition_id, expected_lock, actor_id } => {
            let published_lock = expected_lock.checked_add(2);
            let retired_lock = expected_lock.checked_add(3);
            let lock_matches = match definition.status {
                ApprovalDefinitionStatus::Published => {
                    published_lock == Some(definition.definition_lock_version())
                },
                ApprovalDefinitionStatus::Retired => {
                    retired_lock == Some(definition.definition_lock_version())
                },
                ApprovalDefinitionStatus::Draft => false,
            };
            receipt.result_ref == *definition_id
                && definition.base.id == *definition_id
                && definition.published_by.as_ref().is_some_and(|actor| actor.as_str() == actor_id)
                && definition.published_at.is_some()
                && lock_matches
        },
        LegacyDefinitionProof::Retired { definition_id, expected_lock, actor_id } => {
            receipt.result_ref == *definition_id
                && definition.base.id == *definition_id
                && definition.status == ApprovalDefinitionStatus::Retired
                && expected_lock.checked_add(1) == Some(definition.definition_lock_version())
                && definition.retired_by.as_ref().is_some_and(|actor| actor.as_str() == actor_id)
                && definition.retired_at.is_some()
        },
    };
    if proven {
        return Ok(());
    }
    Err(idempotency_payload_conflict())
}

/// 使用当前启用角色重新验证目标类型定义管理权。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `actor` - 已认证操作人
/// * `policy` - 目标单据类型必须审批政策
/// * `executor` - 调用方事务或非事务执行器
///
/// # 返回
/// 当前启用角色授予该类型定义管理权时返回 `Ok(())`。
///
/// # 错误
/// 缺少权限或仅剩禁用角色残留 Casbin grant 时返回禁止。
///
/// # 关键业务约束
/// 回放与 Fresh 必须共用本检查，禁止用账号级 Casbin 判定绕过停用角色。
pub(super) async fn ensure_definition_admin_permission(
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    actor: &AuditActor,
    policy: &ProcessRequiredApprovalPolicy,
    executor: &mut dyn Executor,
) -> Result<()> {
    let visibility = definition_management_visibility_with_executor(rbac, actor, executor).await?;
    ensure_definition_admin_allowed(visibility.can_define(policy.document_type))
}

/// 仅把 receipt-first 的唯一键竞争标记为可恢复错误。
pub(super) fn mark_receipt_duplicate(error: persistence_core::Error) -> Error {
    match error {
        error @ persistence_core::Error::DuplicateKey(_)
            if error.duplicate_index_name() == Some(APPROVAL_COMMAND_RECEIPT_IDEMPOTENCY_INDEX) =>
        {
            Error::ReceiptDuplicate(error)
        },
        other => Error::from(other),
    }
}

/// 只有收据竞争、瞬态事务和提交结果未知允许退出失败会话后回读。
pub(super) fn definition_command_may_have_committed(error: &Error) -> bool {
    matches!(error, Error::ReceiptDuplicate(_) | Error::TransientTransaction(_) | Error::OutcomeUnknown(_))
}

/// 并发胜者可能仍在提交，使用有界指数退避等待新会话可见。
fn definition_recovery_delay(attempt: usize) -> Duration {
    Duration::from_millis(5_u64 << attempt.min(5))
}

/// 写端口类型级定义管理权闸门。
///
/// # 错误
/// 缺少 `definition_admin` 时禁止写入。
pub(super) fn ensure_definition_admin_allowed(allowed: bool) -> Result<()> {
    if allowed {
        return Ok(());
    }
    Err(Error::Forbidden("没有该单据类型的流程定义管理权限".to_string()))
}

/// 陈旧锁错误。
pub(super) fn stale_lock_error() -> Error {
    Error::from_approval_code(ErrorCode::ApprovalDefinitionVersionConflict)
}

/// 不泄露存在性的未找到错误。
pub(super) fn definition_not_found() -> Error {
    Error::NotFound("审批流程定义不存在".to_string())
}

#[cfg(test)]
mod tests {
    use bpm::ProcessKind;
    use mongodb::error::{Error as MongoError, ErrorKind, WriteError, WriteFailure};
    use serde_json::json;

    use super::super::replace::{next_transition_ids, prepare_definition_nodes};
    use super::super::test_support::two_node_publish_graph;
    use super::*;

    fn duplicate_key_error(index: Option<&str>) -> persistence_core::Error {
        let message = index.map_or_else(
            || "E11000 duplicate key".to_string(),
            |index| format!("E11000 duplicate key index: {index} dup key"),
        );
        let write_error: WriteError = serde_json::from_value(json!({
            "code": 11000,
            "codeName": "DuplicateKey",
            "errmsg": message,
            "errInfo": null,
        }))
        .expect("duplicate fixture");
        let mongo: MongoError = ErrorKind::Write(WriteFailure::WriteError(write_error)).into();
        persistence_core::Error::DuplicateKey(mongo)
    }

    /// 四条定义命令固定 v3 golden，Create/Replace domain 隔离且完整载荷可区分。
    #[test]
    fn canonical_payloads_are_stable() {
        assert!(matches!(parse_idempotency_key("  "), Err(Error::ValidationError(_))));
        assert!(matches!(parse_idempotency_key(&"界".repeat(43)), Err(Error::ValidationError(_))));
        let key = parse_idempotency_key("  key-1  ").expect("规范 key");
        assert_eq!(key.as_str(), "key-1");
        let create = create_draft_identity(
            key.clone(),
            DocumentType::StockAdjustment,
            "库存调整",
            DraftSource::Empty,
            "admin-1",
        )
        .expect("创建身份");
        assert_eq!(
            create.current.scope().as_str(),
            "v3:dfdad7669efdc17e5e2b4a013c9e3ff9fe2fc972e2a1f730557e11039fa77574"
        );
        assert_eq!(
            create.current.digest().as_str(),
            "v3:987f7ac0bf5bbd761d38f649dfe68a98f6e6c923605ce31e22a08b714cd470a6"
        );

        let nodes = prepare_definition_nodes(vec![
            DefinitionNodeRequest {
                node_id: None,
                node_name: " 财务 ".to_string(),
                display_order: 2,
                assignee_user_id: " u2 ".to_string(),
            },
            DefinitionNodeRequest {
                node_id: Some(" node-1 ".to_string()),
                node_name: " 仓储复核 ".to_string(),
                display_order: 1,
                assignee_user_id: " u1 ".to_string(),
            },
        ])
        .expect("节点规范化");
        assert_eq!(nodes[0].node_name, "仓储复核");
        assert_eq!(nodes[0].node_id.as_deref(), Some("node-1"));
        assert_eq!(nodes[1].assignee_user_id, "u2");
        let replace = replace_nodes_identity(key.clone(), "def-1", 3, &nodes, "admin-1").expect("替换身份");
        assert_eq!(
            replace.current.scope().as_str(),
            "v3:8fb38d7db619da0a2f1f164fa6575056b8ecadc45c066396f07aaba97c5a06f3"
        );
        assert_eq!(
            replace.current.digest().as_str(),
            "v3:6623c0836bdd568ef761a797ee6b73dbb799bae019eb20a479fe1f101a5dc3f5"
        );

        let publish = lock_command_identity(
            key.clone(),
            ApprovalCommandKind::PublishDefinition,
            PUBLISH_DEFINITION_COMMAND_DOMAIN,
            "def-1",
            3,
            "admin-1",
        );
        let publish = publish.expect("发布身份");
        assert_eq!(
            publish.current.scope().as_str(),
            "v3:5001966af5c0b5d438e4adb337a5f052689071e4fad50ab18800eb5055394786"
        );
        assert_eq!(
            publish.current.digest().as_str(),
            "v3:fe9b47c8cd4d4e5b98c9b4ebab65b8b4756668aed1d8068705b7bd0024c5b345"
        );

        let retire = lock_command_identity(
            key.clone(),
            ApprovalCommandKind::RetireDefinition,
            RETIRE_DEFINITION_COMMAND_DOMAIN,
            "def-1",
            3,
            "admin-1",
        )
        .expect("退役身份");
        assert_eq!(
            retire.current.scope().as_str(),
            "v3:c89adeafb2fcbb835edf67a65ad722c7fdf172e8c44be3c54372e2aa9e00fb86"
        );
        assert_eq!(
            retire.current.digest().as_str(),
            "v3:6ae60ca970d53104d79c8ff103c64542fa44118fb0436416eaccd6e9e8fe34db"
        );

        assert_ne!(create.current.scope(), replace.current.scope());
        let create_changed = create_draft_identity(
            key.clone(),
            DocumentType::StockAdjustment,
            "库存\u{1f}调整",
            DraftSource::Empty,
            "admin-1",
        )
        .expect("分隔符文本");
        assert_ne!(create.current.digest(), create_changed.current.digest());
        let literal_null_nodes = prepare_definition_nodes(vec![DefinitionNodeRequest {
            node_id: Some("NULL".to_string()),
            node_name: "审批".to_string(),
            display_order: 1,
            assignee_user_id: "用户甲".to_string(),
        }])
        .expect("字面量 NULL");
        let absent_nodes = prepare_definition_nodes(vec![DefinitionNodeRequest {
            node_id: None,
            node_name: "审批".to_string(),
            display_order: 1,
            assignee_user_id: "用户甲".to_string(),
        }])
        .expect("空可选值");
        assert_ne!(
            replace_nodes_identity(key.clone(), "def-1", 3, &literal_null_nodes, "admin-1")
                .unwrap()
                .current
                .digest(),
            replace_nodes_identity(key.clone(), "def-1", 3, &absent_nodes, "admin-1")
                .unwrap()
                .current
                .digest()
        );

        assert!(matches!(
            map_model_error(ModelError::CommandReceiptConflict),
            Error::Coded(ErrorCode::ApprovalIdempotencyPayloadConflict)
        ));
        let result_ref =
            DefinitionCommandResultRef { definition_id: "def-1".to_string(), definition_lock_version: 1 }
                .encode();
        let same = ApprovalCommandReceipt::new(
            ApprovalCommandReceiptId::new("r1"),
            &create.current,
            result_ref,
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap();
        same.reconcile_identity(&create.current).expect("同载荷必须回读");
        assert!(matches!(
            map_model_error(same.reconcile_identity(&create_changed.current).unwrap_err()),
            Error::Coded(ErrorCode::ApprovalIdempotencyPayloadConflict)
        ));
    }

    /// 只有幂等三元组唯一索引冲突可进入胜者回读，ID 或未知索引必须失败关闭。
    #[test]
    fn receipt_duplicate_recovery_requires_exact_idempotency_index() {
        let idempotency =
            mark_receipt_duplicate(duplicate_key_error(Some(APPROVAL_COMMAND_RECEIPT_IDEMPOTENCY_INDEX)));
        assert!(matches!(idempotency, Error::ReceiptDuplicate(_)));
        assert!(definition_command_may_have_committed(&idempotency));

        let id_collision =
            mark_receipt_duplicate(duplicate_key_error(Some("uk_approval_command_receipts_id")));
        assert!(matches!(id_collision, Error::ConflictError(_)));
        assert!(!definition_command_may_have_committed(&id_collision));

        let unknown = mark_receipt_duplicate(duplicate_key_error(None));
        assert!(matches!(unknown, Error::ConflictError(_)));
        assert!(!definition_command_may_have_committed(&unknown));
    }

    /// 当前结果引用冻结 UTF-8 字节长度与锁版本，资源或版本漂移必须失败关闭。
    #[test]
    fn definition_result_reference_is_versioned_and_exact() {
        let unicode = DefinitionCommandResultRef {
            definition_id: "定义-一".to_string(),
            definition_lock_version: 7,
        };
        assert_eq!(DefinitionCommandResultRef::parse(&unicode.encode()).unwrap(), unicode);
        assert!(DefinitionCommandResultRef::parse("def-1").is_err());
        assert!(DefinitionCommandResultRef::parse("definition-v1:99:def-1:1").is_err());

        let graph = two_node_publish_graph();
        let result_ref = DefinitionCommandResultRef::from_graph(&graph);
        result_ref
            .ensure_matches(
                &graph,
                &DefinitionResultExpectation::DefinitionId(graph.definition.base.id.clone()),
            )
            .expect("精确结果");
        assert!(
            result_ref
                .ensure_matches(&graph, &DefinitionResultExpectation::DefinitionId("other".to_string()),)
                .is_err()
        );
        let mut drifted = graph.clone();
        drifted.definition.base.version += 1;
        assert!(
            result_ref
                .ensure_matches(
                    &drifted,
                    &DefinitionResultExpectation::ProcessKind(ProcessKind::StockAdjustment),
                )
                .is_err()
        );
    }

    /// 旧收据只按精确候选读取；发布/退役可由不可变事实证明，创建来源不可证明。
    #[test]
    fn legacy_definition_receipts_require_complete_immutable_proof() {
        let key = IdempotencyKey::parse("key-legacy").unwrap();
        let identity = lock_command_identity(
            key,
            ApprovalCommandKind::PublishDefinition,
            PUBLISH_DEFINITION_COMMAND_DOMAIN,
            "def-1",
            2,
            "admin",
        )
        .unwrap();
        let mut receipt = ApprovalCommandReceipt::new(
            ApprovalCommandReceiptId::new("receipt-legacy"),
            &identity.current,
            "def-1",
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap();
        receipt.scope_id = "def-1".to_string();
        receipt.payload_digest = legacy_payload_digest(&["2", "admin"]);

        let draft = two_node_publish_graph();
        let refreshed = DefinitionGraph::rebuild_draft(
            &draft.definition,
            draft.nodes,
            next_transition_ids(2),
            Timestamp::from_unix_secs(2).unwrap(),
        )
        .unwrap();
        let mut published = refreshed;
        published
            .definition
            .publish(ParticipantId::new("admin").unwrap(), Timestamp::from_unix_secs(3).unwrap())
            .unwrap();
        ensure_legacy_result(
            &receipt,
            &published,
            &LegacyDefinitionProof::Published {
                definition_id: "def-1".to_string(),
                expected_lock: 2,
                actor_id: "admin".to_string(),
            },
        )
        .expect("发布旧结果可证明");
        assert!(
            ensure_legacy_result(
                &receipt,
                &published,
                &LegacyDefinitionProof::Published {
                    definition_id: "def-1".to_string(),
                    expected_lock: 2,
                    actor_id: "other".to_string(),
                },
            )
            .is_err()
        );
        assert!(ensure_legacy_result(&receipt, &published, &LegacyDefinitionProof::Unprovable,).is_err());

        let retire_expected = published.definition.definition_lock_version();
        published
            .definition
            .retire(ParticipantId::new("admin").unwrap(), Timestamp::from_unix_secs(4).unwrap())
            .unwrap();
        ensure_legacy_result(
            &receipt,
            &published,
            &LegacyDefinitionProof::Retired {
                definition_id: "def-1".to_string(),
                expected_lock: retire_expected,
                actor_id: "admin".to_string(),
            },
        )
        .expect("退役旧结果可证明");
    }
}
