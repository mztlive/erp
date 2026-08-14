//! W22 系统安全暂停的不可变操作证据。
//!
//! 一条记录冻结一个可信来源事件及其首次计算出的完整影响集。记录只追加、
//! 不提供更新方法；事件级唯一性由仓储索引
//! `(source_object_type, source_object_id, cause, source_version)` 承接。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{ProductPublicationDeliveryId, ProductPublicationId, ProductPublicationRevisionId};
use crate::validation::normalize_required_text;

const SOURCE_ID_MAX_LEN: usize = 128;
const SOURCE_VERSION_MAX_LEN: usize = 128;
const IDEMPOTENCY_KEY_MAX_LEN: usize = 256;
const REFERENCE_MAX_LEN: usize = 256;
const MESSAGE_MAX_LEN: usize = 512;

/// 系统安全暂停的固定原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SafetyPauseCause {
    /// 供应关系或供应事实已停止。
    SupplierStopped,
    /// 可供数量归零。
    ZeroInventory,
    /// 来源明确不可供。
    SupplyUnavailable,
    /// 可供事实超过新鲜度阈值。
    AvailabilityStale,
    /// 成本字段发生尚未确认的变化。
    CostChangeUnconfirmed,
    /// 其它关键供给字段发生尚未确认的变化。
    CriticalSupplyChangeUnconfirmed,
    /// 反序列化得到未注册原因时保留显式未知态，由应用层失败关闭。
    #[serde(other)]
    Unknown,
}

impl SafetyPauseCause {
    /// 返回持久化使用的稳定代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SupplierStopped => "SUPPLIER_STOPPED",
            Self::ZeroInventory => "ZERO_INVENTORY",
            Self::SupplyUnavailable => "SUPPLY_UNAVAILABLE",
            Self::AvailabilityStale => "AVAILABILITY_STALE",
            Self::CostChangeUnconfirmed => "COST_CHANGE_UNCONFIRMED",
            Self::CriticalSupplyChangeUnconfirmed => "CRITICAL_SUPPLY_CHANGE_UNCONFIRMED",
            Self::Unknown => "UNKNOWN",
        }
    }
}

/// 触发安全暂停的固定来源对象类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SafetyPauseSourceObjectType {
    /// 公司 SKU 的供应商供给。
    SupplierOffering,
    /// 反序列化得到未注册类型时保留显式未知态，由应用层失败关闭。
    #[serde(other)]
    Unknown,
}

impl SafetyPauseSourceObjectType {
    /// 返回持久化使用的稳定代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SupplierOffering => "SUPPLIER_OFFERING",
            Self::Unknown => "UNKNOWN",
        }
    }
}

/// 安全暂停形成的不可变发布子结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SafetyPauseAffectedPublication {
    /// 被暂停的稳定发布。
    pub publication_id: ProductPublicationId,
    /// 新形成的不可变暂停修订。
    pub pause_revision_id: ProductPublicationRevisionId,
    /// 指向暂停修订的待发送投递。
    pub delivery_id: ProductPublicationDeliveryId,
}

/// `SUPPLIER_STOPPED` 唯一后续任务的冻结引用。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SafetyPauseWorkItemRef {
    /// 正式任务 ID。
    pub work_item_id: String,
    /// 创建或复用时的任务版本。
    pub task_version: u64,
    /// 任务业务对象类型，必须与来源对象类型一致。
    pub business_object_type: String,
    /// 任务业务对象 ID，必须与来源对象 ID 一致。
    pub business_object_id: String,
    /// 任务实际冻结的业务版本。
    pub subject_version: String,
    /// 固定注册并路由 W21 的 handler key。
    pub handler_key: String,
}

/// 没有正式人工任务时的固定 blocker 类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SafetyPauseBlockerCode {
    /// 当前政策明确不创建人工后续任务。
    NoManualFollowUpTaskByCurrentPolicy,
    /// 正常复核任务类型尚未注册。
    NormalReviewWorkItemTypeUnregistered,
}

impl SafetyPauseBlockerCode {
    /// 返回持久化使用的稳定代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoManualFollowUpTaskByCurrentPolicy => "NO_MANUAL_FOLLOW_UP_TASK_BY_CURRENT_POLICY",
            Self::NormalReviewWorkItemTypeUnregistered => "NORMAL_REVIEW_WORK_ITEM_TYPE_UNREGISTERED",
        }
    }
}

/// 未注册人工任务分支的不可变 blocker 证据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SafetyPauseBlocker {
    /// 固定 blocker 代码。
    pub code: SafetyPauseBlockerCode,
    /// 面向业务的稳定说明。
    pub message: String,
    /// 指向本次不可变操作证据的引用。
    pub evidence_reference: String,
}

/// 安全暂停的后续责任结论。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SafetyPauseFollowUp {
    /// `SUPPLIER_STOPPED` 的唯一正式任务。
    WorkItem(SafetyPauseWorkItemRef),
    /// 其它原因的强类型 blocker；禁止伪造人工任务。
    Blocker(SafetyPauseBlocker),
}

/// 创建不可变安全暂停操作所需的数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SystemSafetyPauseOperationData {
    /// 暂停原因。
    pub cause: SafetyPauseCause,
    /// 来源对象类型。
    pub source_object_type: SafetyPauseSourceObjectType,
    /// 来源对象 ID。
    pub source_object_id: String,
    /// 来源事实版本。
    pub source_version: String,
    /// 调用链幂等键；事件唯一性不依赖该字段。
    pub idempotency_key: String,
    /// 首次冻结的完整非空影响集。
    pub affected_publications: Vec<SafetyPauseAffectedPublication>,
    /// 与原因严格匹配的后续责任结论。
    pub follow_up: SafetyPauseFollowUp,
    /// 可信来源事实发生时间。
    pub occurred_at: Instant,
    /// 本地事务提交使用的统一业务时间。
    pub committed_at: Instant,
}

/// W22 系统安全暂停不可变操作。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct SystemSafetyPauseOperation {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 暂停原因。
    pub cause: SafetyPauseCause,
    /// 来源对象类型。
    pub source_object_type: SafetyPauseSourceObjectType,
    /// 来源对象 ID。
    pub source_object_id: String,
    /// 来源事实版本。
    pub source_version: String,
    /// 调用链幂等键。
    pub idempotency_key: String,
    /// 首次冻结的完整影响集。
    pub affected_publications: Vec<SafetyPauseAffectedPublication>,
    /// 后续任务或 blocker 证据。
    pub follow_up: SafetyPauseFollowUp,
    /// 可信来源事实发生时间。
    pub occurred_at: Instant,
    /// 本地提交时间。
    pub committed_at: Instant,
}

impl SystemSafetyPauseOperation {
    /// 创建并校验不可变安全暂停操作。
    ///
    /// # 错误
    /// 来源身份、版本或幂等键为空，影响集为空/重复，原因与后续结论不匹配，
    /// 或来源任务引用不一致时返回错误。
    pub fn new(id: impl Into<String>, data: SystemSafetyPauseOperationData) -> Result<Self> {
        if data.cause == SafetyPauseCause::Unknown {
            return Err(Error::from("未知安全暂停原因必须失败关闭"));
        }
        if data.source_object_type == SafetyPauseSourceObjectType::Unknown {
            return Err(Error::from("未知安全暂停来源对象必须失败关闭"));
        }
        let source_object_id = normalize_required_text(
            data.source_object_id,
            "安全暂停来源对象不能为空",
            SOURCE_ID_MAX_LEN,
            "安全暂停来源对象过长",
        )?;
        let source_version = normalize_required_text(
            data.source_version,
            "安全暂停来源版本不能为空",
            SOURCE_VERSION_MAX_LEN,
            "安全暂停来源版本过长",
        )?;
        let idempotency_key = normalize_required_text(
            data.idempotency_key,
            "安全暂停幂等键不能为空",
            IDEMPOTENCY_KEY_MAX_LEN,
            "安全暂停幂等键过长",
        )?;
        ensure_affected_publications(&data.affected_publications)?;
        let follow_up = normalize_follow_up(
            data.cause,
            data.source_object_type,
            &source_object_id,
            &source_version,
            data.follow_up,
        )?;

        Ok(Self {
            base: BaseModel::new(id.into()),
            cause: data.cause,
            source_object_type: data.source_object_type,
            source_object_id,
            source_version,
            idempotency_key,
            affected_publications: data.affected_publications,
            follow_up,
            occurred_at: data.occurred_at,
            committed_at: data.committed_at,
        })
    }
}

fn ensure_affected_publications(affected: &[SafetyPauseAffectedPublication]) -> Result<()> {
    if affected.is_empty() {
        return Err(Error::from("安全暂停必须冻结至少一个在售发布"));
    }
    let mut ids = std::collections::HashSet::with_capacity(affected.len());
    if affected
        .iter()
        .any(|item| !ids.insert(item.publication_id.to_string()))
    {
        return Err(Error::from("安全暂停影响集不得包含重复发布"));
    }
    Ok(())
}

fn normalize_follow_up(
    cause: SafetyPauseCause,
    source_object_type: SafetyPauseSourceObjectType,
    source_object_id: &str,
    source_version: &str,
    follow_up: SafetyPauseFollowUp,
) -> Result<SafetyPauseFollowUp> {
    match (cause, follow_up) {
        (SafetyPauseCause::SupplierStopped, SafetyPauseFollowUp::WorkItem(mut item)) => {
            item.work_item_id = required_reference(item.work_item_id, "后续任务ID")?;
            item.business_object_type = required_reference(item.business_object_type, "任务业务对象类型")?;
            item.business_object_id = required_reference(item.business_object_id, "任务业务对象ID")?;
            item.subject_version = required_reference(item.subject_version, "任务对象版本")?;
            item.handler_key = required_reference(item.handler_key, "任务处理器")?;
            if item.business_object_type != source_object_type.as_str()
                || item.business_object_id != source_object_id
                || item.subject_version != source_version
                || item.handler_key != "supplier_supply_exception"
            {
                return Err(Error::from(
                    "安全暂停任务必须绑定触发来源对象、来源版本及 W21 固定处理器",
                ));
            }
            Ok(SafetyPauseFollowUp::WorkItem(item))
        }
        (SafetyPauseCause::SupplierStopped, SafetyPauseFollowUp::Blocker(_)) => {
            Err(Error::from("供应停止必须形成唯一正式后续任务"))
        }
        (_, SafetyPauseFollowUp::WorkItem(_)) => Err(Error::from("非供应停止原因不得伪造人工任务")),
        (cause, SafetyPauseFollowUp::Blocker(mut blocker)) => {
            let expected = match cause {
                SafetyPauseCause::ZeroInventory
                | SafetyPauseCause::SupplyUnavailable
                | SafetyPauseCause::AvailabilityStale => {
                    SafetyPauseBlockerCode::NoManualFollowUpTaskByCurrentPolicy
                }
                SafetyPauseCause::CostChangeUnconfirmed
                | SafetyPauseCause::CriticalSupplyChangeUnconfirmed => {
                    SafetyPauseBlockerCode::NormalReviewWorkItemTypeUnregistered
                }
                SafetyPauseCause::SupplierStopped | SafetyPauseCause::Unknown => unreachable!(),
            };
            if blocker.code != expected {
                return Err(Error::from("安全暂停 blocker 与原因不匹配"));
            }
            blocker.message = normalize_required_text(
                blocker.message,
                "安全暂停 blocker 说明不能为空",
                MESSAGE_MAX_LEN,
                "安全暂停 blocker 说明过长",
            )?;
            blocker.evidence_reference = required_reference(blocker.evidence_reference, "证据引用")?;
            Ok(SafetyPauseFollowUp::Blocker(blocker))
        }
    }
}

fn required_reference(value: String, field: &str) -> Result<String> {
    normalize_required_text(
        value,
        &format!("{field}不能为空"),
        REFERENCE_MAX_LEN,
        &format!("{field}过长"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn affected() -> Vec<SafetyPauseAffectedPublication> {
        vec![SafetyPauseAffectedPublication {
            publication_id: ProductPublicationId::new("pub-1"),
            pause_revision_id: ProductPublicationRevisionId::new("rev-2"),
            delivery_id: ProductPublicationDeliveryId::new("delivery-2"),
        }]
    }

    #[test]
    fn stopped_requires_matching_work_item() {
        let result = SystemSafetyPauseOperation::new(
            "operation-1",
            SystemSafetyPauseOperationData {
                cause: SafetyPauseCause::SupplierStopped,
                source_object_type: SafetyPauseSourceObjectType::SupplierOffering,
                source_object_id: "offering-1".to_string(),
                source_version: "availability:2".to_string(),
                idempotency_key: "event-1".to_string(),
                affected_publications: affected(),
                follow_up: SafetyPauseFollowUp::Blocker(SafetyPauseBlocker {
                    code: SafetyPauseBlockerCode::NoManualFollowUpTaskByCurrentPolicy,
                    message: "不应出现".to_string(),
                    evidence_reference: "operation-1".to_string(),
                }),
                occurred_at: Instant::from_unix_secs(1),
                committed_at: Instant::from_unix_secs(1),
            },
        );

        assert!(result.is_err());
    }

    #[test]
    fn non_stopped_rejects_fake_work_item() {
        let result = SystemSafetyPauseOperation::new(
            "operation-1",
            SystemSafetyPauseOperationData {
                cause: SafetyPauseCause::SupplyUnavailable,
                source_object_type: SafetyPauseSourceObjectType::SupplierOffering,
                source_object_id: "offering-1".to_string(),
                source_version: "availability:2".to_string(),
                idempotency_key: "event-1".to_string(),
                affected_publications: affected(),
                follow_up: SafetyPauseFollowUp::WorkItem(SafetyPauseWorkItemRef {
                    work_item_id: "work-1".to_string(),
                    task_version: 1,
                    business_object_type: "SUPPLIER_OFFERING".to_string(),
                    business_object_id: "offering-1".to_string(),
                    subject_version: "availability:2".to_string(),
                    handler_key: "supplier_supply_exception".to_string(),
                }),
                occurred_at: Instant::from_unix_secs(1),
                committed_at: Instant::from_unix_secs(1),
            },
        );

        assert!(result.is_err());
    }
}
