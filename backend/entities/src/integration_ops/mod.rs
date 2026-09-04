//! 域 D34 `integration_ops`：inbox_message、integration_error_task、reconciliation_difference(+_resolution)（页面：W29）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 P0 公共基元。
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
mod inbox_message;
mod integration_error_task;
mod reconciliation_difference;
mod reconciliation_difference_resolution;
mod w29_close;
mod w29_work_items;

pub use decision_policy::{
    difference_terminal_policy, error_terminal_policy, next_actions_after_outcome,
    project_difference_actions, project_error_actions, reconciliation_reason_registry, ActionBlocker,
    DecidedAction, DifferenceActionProjection, ErrorActionProjection, FundsImpact, ProjectionOutcome,
    ProjectionSubject, ReasonRegistry, RegisteredReason, RequiredEvidenceKind, TerminalEvidencePolicy,
    DIFFERENCE_POLICY_ID, ERROR_POLICY_ID, EVIDENCE_POLICY_VERSION, REASON_REGISTRY_ID,
    REASON_REGISTRY_VERSION,
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
    difference_owner_role, error_owner_role, error_priority, error_work_item_type, new_difference_work_item,
    new_error_work_item, DIFFERENCE_INITIAL_SUBJECT_VERSION, DIFFERENCE_WORK_ITEM_OBJECT_TYPE,
    ERROR_WORK_ITEM_OBJECT_TYPE, W29_FINANCE_ROLE, W29_OPERATIONS_ROLE, W29_OWNER_ORGANIZATION,
    W29_PROCUREMENT_ROLE, W29_SYSADMIN_ROLE,
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
}

impl IntegrationCommandIdentity {
    /// 从命令身份字段与序列化载荷构造不可逆收据身份。
    ///
    /// 原始幂等键只参与 SHA-256，不进入收据 ID 或持久化结果；完整载荷独立
    /// 形成指纹，用于拒绝同键异参。
    ///
    /// # 参数
    /// * `actor_id` - 命令操作人
    /// * `action` - 稳定动作名
    /// * `resource_type` - 资源类型
    /// * `resource_id` - 资源 ID
    /// * `idempotency_key` - 客户端幂等键
    /// * `payload` - 命令序列化字节
    ///
    /// # 返回
    /// 返回不暴露原始幂等键的稳定命令身份。
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
        }
    }

    /// 返回稳定收据 ID。
    ///
    /// # 返回
    /// 返回不含原始幂等键的 SHA-256 派生收据 ID。
    pub fn receipt_id(&self) -> &str {
        &self.receipt_id
    }

    /// 返回完整命令载荷指纹。
    ///
    /// # 返回
    /// 返回用于拒绝同键异参的 SHA-256 指纹。
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// 返回命令动作名。
    ///
    /// # 返回
    /// 返回构造身份时冻结的稳定动作名。
    pub fn action(&self) -> &str {
        &self.action
    }

    /// 返回命令资源类型。
    ///
    /// # 返回
    /// 返回构造身份时冻结的资源类型。
    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }

    /// 返回命令资源 ID。
    ///
    /// # 返回
    /// 返回构造身份时冻结的资源 ID。
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    /// 校验已持久化审计记录的稳定命令身份字段。
    ///
    /// # 参数
    /// * `actor_id` - 审计操作人
    /// * `action` - 审计动作
    /// * `resource_type` - 审计资源类型
    /// * `resource_id` - 审计资源 ID
    ///
    /// # 返回
    /// 全部身份字段与命令冻结值一致时返回 `true`。
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

fn sha256_hex(value: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(value);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use crate::ids::{
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
