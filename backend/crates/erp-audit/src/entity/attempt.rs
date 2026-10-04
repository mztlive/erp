//! 事务回滚之外保存的命令尝试；不得作为幂等或业务终态依据。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::AccountKind;
use serde::{Deserialize, Serialize};

/// 命令尝试的明确执行分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAttemptResult {
    Failed,
    Rejected,
    Unknown,
}

/// 安全的命令尝试快照；不保存请求正文、错误正文或业务实体。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Entity)]
pub struct AuditAttempt {
    #[serde(flatten)]
    pub base: BaseModel,
    pub schema_version: u16,
    pub event_kind: AuditAttemptKind,
    pub action_code: String,
    pub action_version: u16,
    pub action_label: String,
    pub actor_id: String,
    pub actor_account: String,
    pub actor_type: AccountKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_name_snapshot: Option<String>,
    pub resource_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_number_snapshot: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub result: AuditAttemptResult,
}

/// 与成功业务事件分开的持久化用途。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAttemptKind {
    CommandAttempt,
}
