//! 域 D34 `integration_ops`：inbox_message、integration_error_task、reconciliation_difference(+_resolution)（页面：W29）。
//!
//! 实体层无跨域依赖：只引用 `erp_core::ids` 的 ID newtype 与 P0 公共基元。
//! 字段字典见数据模型 §6.21；公共字段归属按 §4.3 判定：
//! - `inbox_message` 是已接收外部消息的普通入站记录（消息事实，含幂等键与来源引用），
//!   字典未含 `FactBase` 语义字段 → 只用 `BaseModel`；处理状态按 §6.21 固化取值，
//!   投递状态由 `integration_error_task.status` 表达（§7.7，不另设消息投递状态机）；
//! - `integration_error_task` 是集成任务（普通表），`status` 按 §7.7 实现固定状态机，
//!   终态（已解决/已关闭）无出边，测试采用逐边定向断言；
//! - `reconciliation_difference` 是正式差异事实、`reconciliation_difference_resolution`
//!   是不可变解决记录（§4.5.1 正式事实不设业务软删除，解决记录只追加不更新）；
//!   差异发现时间由字典 `created_at` 承载（≡ `BaseModel.created_at`），不另设
//!   `occurred_at`/`recorded_by` 等字段。
//!
//! 业务规则来源：§6.21（字段字典与约束）、§7.7（投递状态由错误任务表达）、
//! §8.4 第 3 条（inbox 消息去重与业务事实键幂等）、§4.2/§4.3/§4.5（定点数值、
//! 公共字段、事实不软删）、`erp-phase-2.md` §13（接口治理、重试与人工补偿）、
//! `erp-mall-data-mapping.md` §10.4.1（商城关键事实共同信封）。

use sha2::{Digest, Sha256};

mod decision_policy;
mod direct_conclusion;
mod error_classification;
mod evidence_reference;
pub(crate) mod handler;
pub(crate) mod inbox_message;
pub(crate) mod integration_error_task;
pub(crate) mod reconciliation_difference;
pub(crate) mod reconciliation_difference_resolution;
mod w29_close;
mod w29_work_items;

pub use decision_policy::{
    ActionBlocker, DIFFERENCE_POLICY_ID, DecidedAction, DifferenceActionProjection, ERROR_POLICY_ID,
    EVIDENCE_POLICY_VERSION, ErrorActionProjection, FundsImpact, ProjectionOutcome, ProjectionSubject,
    REASON_REGISTRY_ID, REASON_REGISTRY_VERSION, ReasonRegistry, RegisteredReason, RequiredEvidenceKind,
    TerminalEvidencePolicy, difference_terminal_policy, error_terminal_policy, next_actions_after_outcome,
    project_difference_actions, project_error_actions, reconciliation_reason_registry,
};
pub use direct_conclusion::DirectConclusion;
pub use error_classification::{is_result_unknown, normalized_result_unknown_class};
pub use evidence_reference::{
    CanonicalEvidenceReference, CompactEvidenceSet, EvidenceRecordRef, EvidenceReferenceSet,
    EvidenceSubjectBindings, ReplayOriginalReference,
};
pub use inbox_message::*;
pub use integration_error_task::*;
pub use reconciliation_difference::*;
pub use reconciliation_difference_resolution::*;
pub use w29_close::*;
pub use w29_work_items::{
    DIFFERENCE_INITIAL_SUBJECT_VERSION, DIFFERENCE_WORK_ITEM_OBJECT_TYPE, ERROR_WORK_ITEM_OBJECT_TYPE,
    IntegrationResponsibilityKind, IntegrationResponsibilityPriority, IntegrationResponsibilitySpec,
    W29_FINANCE_ROLE, W29_OPERATIONS_ROLE, W29_PROCUREMENT_ROLE, W29_SYSADMIN_ROLE, difference_owner_role,
    difference_responsibility, error_owner_role, error_priority, error_responsibility, error_work_item_type,
};

/// W29 命令的稳定幂等身份与载荷指纹。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationCommandIdentity {
    receipt_id: String,
    actor_id: String,
    action: String,
    resource_type: String,
    resource_id: String,
    fingerprint: String,
    idempotency_key_hash: String,
}

impl IntegrationCommandIdentity {
    /// 从命令身份字段与序列化载荷构造不可逆收据身份。
    ///
    /// 原始幂等键只参与 SHA-256，不进入收据 ID 或持久化结果；完整载荷独立形成指纹，用于拒绝同键异参。
    ///
    /// 返回不暴露原始幂等键的稳定命令身份。
    ///
    /// `action` 为稳定动作名。
    pub fn new(
        actor_id: &str,
        action: &str,
        resource_type: &str,
        resource_id: &str,
        idempotency_key: &str,
        payload: &[u8],
    ) -> Self {
        let receipt_id = format!(
            "w29_{}",
            sha256_hex(
                format!("{actor_id}|{action}|{resource_type}|{resource_id}|{idempotency_key}").as_bytes()
            )
        );
        Self {
            receipt_id,
            actor_id: actor_id.to_string(),
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id: resource_id.to_string(),
            fingerprint: sha256_hex(payload),
            idempotency_key_hash: sha256_hex(idempotency_key.as_bytes()),
        }
    }

    /// 借用 SHA-256 派生的稳定收据 ID，供按原命令查找结果；标识本身不包含原始幂等键。
    pub fn receipt_id(&self) -> &str {
        &self.receipt_id
    }

    /// 返回原始幂等键的 SHA-256 摘要，不保存原始键。
    ///
    /// 返回原始幂等键字节的 64 位小写十六进制摘要；构造时不对键做去空白或其他规范化。
    pub fn idempotency_key_hash(&self) -> &str {
        &self.idempotency_key_hash
    }

    /// 借用完整请求载荷的 SHA-256 指纹，供重试时识别使用同一幂等键但内容不同的请求。
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// 借用命令发起时固定的动作名，用来核对重试是否执行同一种业务操作。
    pub fn action(&self) -> &str {
        &self.action
    }

    /// 借用命令身份中的资源类型，供回执比对区分不同业务对象。
    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }

    /// 借用命令绑定的资源 ID，供重放时确认请求仍指向原业务对象。
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    /// 校验已持久化审计记录的稳定命令身份字段。
    ///
    /// 操作人、动作、资源类型都与冻结值一致，且 `resource_id` 为 `Some` 并等于冻结资源 ID 时返回 `true`。
    pub fn matches_receipt(
        &self,
        actor_id: &str,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
    ) -> bool {
        self.actor_id == actor_id
            && self.action == action
            && self.resource_type == resource_type
            && resource_id == Some(self.resource_id.as_str())
    }
}

// 计算稳定 SHA-256 十六进制文本（与 `format!("{byte:02x}")` 同形态）。返回 64 位小写十六进制摘要。
fn sha256_hex(value: &[u8]) -> String {
    Sha256::digest(value).iter().map(|byte| format!("{byte:02x}")).collect()
}

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 erp_core::ids）。
pub use erp_core::ids::{
    InboxMessageId, IntegrationErrorTaskId, ReconciliationDifferenceId, ReconciliationDifferenceResolutionId,
    SourceSystemId,
};

#[cfg(test)]
mod command_identity_tests {
    use super::IntegrationCommandIdentity;

    #[test]
    fn command_identity_hides_key_and_rejects_changed_identity() {
        let identity = IntegrationCommandIdentity::new(
            "actor-1",
            "integration.task_action",
            "work_item",
            "wi-1",
            "raw-secret-key",
            br#"{"action":"QUERY"}"#,
        );

        assert!(!identity.receipt_id().contains("raw-secret-key"));
        assert_eq!(identity.fingerprint().len(), 64);
        assert!(identity.matches_receipt("actor-1", "integration.task_action", "work_item", Some("wi-1")));
        assert!(!identity.matches_receipt("actor-2", "integration.task_action", "work_item", Some("wi-1")));
    }
}

mod command_receipt;
pub use command_receipt::{
    ActionReceiptResult, CompletionReceiptResult, DirectReceiptResult, IntegrationCommandReceipt,
    IntegrationCommandResult, IntegrationReceiptPayload,
};
