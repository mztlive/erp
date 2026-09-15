//! 连接命令身份、版本指纹与能力内存变更；不执行外部 I/O。
use erp_core::ids::SupplierApiCapabilityId;
use id_generator::next_id;

pub use super::context::digest;
use super::context::ensure_version;
use crate::dto::supplier_api::*;
use crate::entity::supplier_api::*;
use crate::{Error, Result};
/// 连接命令的确定性身份；原始幂等键不进入持久化事实。
pub struct CommandIdentity {
    pub connection_id: String,
    pub actor_id: String,
    pub action: SupplierConnectionAction,
    pub idempotency_hash: String,
    pub fingerprint: String,
    pub receipt_id: String,
    pub audit_id: String,
}
impl CommandIdentity {
    /// 按操作人、连接、动作和原始命令生成原幂等摘要。
    pub fn new(id: &str, actor_id: &str, command: &SupplierConnectionCommand) -> Result<Self> {
        required(Some(command.idempotency_key.as_str()), "幂等键不能为空")?;
        let idempotency_hash =
            digest(&[actor_id, id, command.action.as_str(), command.idempotency_key.trim()]);
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
        match capabilities.iter().find(|capability| capability.capability_code == change.code) {
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
            },
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
            },
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
pub fn map_capability_change_rejection(rejection: CapabilityChangeSetRejection) -> Error {
    match rejection {
        CapabilityChangeSetRejection::EmptyOrTooMany
        | CapabilityChangeSetRejection::DuplicateCodes
        | CapabilityChangeSetRejection::MissingExpectedVersion(_)
        | CapabilityChangeSetRejection::UnexpectedExpectedVersion(_) => {
            Error::ValidationError(rejection.to_string())
        },
        CapabilityChangeSetRejection::NewCapabilityVersionMustBeZero(_) => {
            Error::ConflictError(rejection.to_string())
        },
        CapabilityChangeSetRejection::NewCapabilityMustStartDisabled(_) => {
            Error::BusinessLogicError(rejection.to_string())
        },
    }
}

pub fn replay_confirmation(
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

fn command_fingerprint(id: &str, command: &SupplierConnectionCommand) -> String {
    digest(&[
        id,
        command.action.as_str(),
        &command.expected_version.to_string(),
        command.payload_reference.as_deref().unwrap_or_default(),
        command.reason_code.as_deref().unwrap_or_default(),
        command.check_type.map(|value| format!("{value:?}")).as_deref().unwrap_or_default(),
    ])
}

pub fn confirmation_fingerprint(id: &str, command: &ConfirmBusinessCapabilityRequirementCommand) -> String {
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

pub fn capability_update_fingerprint(id: &str, command: &UpdateSupplierCapabilitiesCommand) -> String {
    let payload = serde_json::to_string(command).unwrap_or_default();
    digest(&[id, &payload])
}

pub fn ensure_audit_fingerprint(message: Option<&str>, fingerprint: &str) -> Result<()> {
    if message == Some(format!("request_sha256={fingerprint}").as_str()) {
        return Ok(());
    }
    Err(Error::ConflictError("同一幂等键不能提交不同参数".to_string()))
}
