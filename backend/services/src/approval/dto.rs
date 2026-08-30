//! 目标审批运行编排与恢复授权 DTO。
//!
//! 旧步骤实例、旧决定枚举和旧责任模式投影已删除。

use entities::AccountKind;
use serde::{Deserialize, Serialize};

/// 阻塞审批恢复的不可伪造授权锚点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRecoveryAuthorization {
    /// 已认证账号类型。
    pub actor_kind: AccountKind,
    /// Casbin 持久化策略单调版本。
    pub policy_revision: u64,
    /// 在该版本下实际授予运行恢复权的角色。
    pub granting_role_ids: Vec<String>,
    /// `None` 表示公司级；`Some` 仅允许列出的组织或团队。
    pub organization_ids: Option<Vec<String>>,
}

/// 目标运行编排的启动命令。不得包含 definition key 或审批人。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalStartCommand {
    /// 业务对象种类。
    pub subject_kind: String,
    /// 业务对象 ID。
    pub subject_id: String,
    /// 冻结提交版本。
    pub subject_version: u32,
    /// 启动人。
    #[serde(skip)]
    pub actor_id: String,
    /// 幂等键。
    pub idempotency_key: String,
}

/// 目标决定命令。只允许合同 §14.3 字段。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalDecisionCommand {
    /// 当前待办。
    pub work_item_id: String,
    /// 通过或驳回。
    pub decision: bpm::model::types::ApprovalDecision,
    /// 驳回必填原因。
    pub reason: Option<String>,
    /// 期望任务版本。
    pub expected_task_version: u64,
    /// 幂等键。
    pub idempotency_key: String,
    /// 已认证决定人。
    #[serde(skip)]
    pub actor_id: String,
}

/// 目标取消命令。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalCancelCommand {
    /// 实例 ID。
    pub approval_process_instance_id: String,
    /// 冻结提交版本。
    pub expected_subject_version: u32,
    /// 期望实例版本。
    pub expected_instance_version: u64,
    /// 期望执行版本。
    pub expected_execution_version: u64,
    /// 可空任务版本。
    pub expected_task_version: Option<u64>,
    /// 非空撤回原因。
    pub reason: String,
    /// 幂等键。
    pub idempotency_key: String,
    /// 已认证取消人。
    #[serde(skip)]
    pub actor_id: String,
}

/// 恢复原审批人命令。不接受目标用户或恢复动作枚举。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalResumeCommand {
    /// 实例 ID。
    pub approval_process_instance_id: String,
    /// 期望实例版本。
    pub expected_instance_version: u64,
    /// 期望执行版本。
    pub expected_execution_version: u64,
    /// 期望绑定版本。
    pub expected_assignment_version: u64,
    /// 可空已关闭任务版本。
    pub expected_closed_task_version: Option<u64>,
    /// 幂等键。
    pub idempotency_key: String,
    /// 已认证恢复人。
    #[serde(skip)]
    pub actor_id: String,
}

/// 管理员改派命令。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalReassignCommand {
    /// 实例 ID。
    pub approval_process_instance_id: String,
    /// 目标用户。
    pub target_user_id: String,
    /// 非空原因。
    pub reason: String,
    /// 期望实例版本。
    pub expected_instance_version: u64,
    /// 期望执行版本。
    pub expected_execution_version: u64,
    /// 期望绑定版本。
    pub expected_assignment_version: u64,
    /// 可空已关闭任务版本。
    pub expected_task_version: Option<u64>,
    /// 幂等键。
    pub idempotency_key: String,
    /// 已认证改派人。
    #[serde(skip)]
    pub actor_id: String,
}

/// 受阻取消命令。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalCancelBlockedCommand {
    /// 实例 ID。
    pub approval_process_instance_id: String,
    /// 期望实例版本。
    pub expected_instance_version: u64,
    /// 期望执行版本。
    pub expected_execution_version: u64,
    /// 可空任务版本。
    pub expected_task_version: Option<u64>,
    /// 非空原因。
    pub reason: String,
    /// 幂等键。
    pub idempotency_key: String,
    /// 已认证管理员。
    #[serde(skip)]
    pub actor_id: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::ApprovalDecisionCommand;

    #[test]
    fn decision_http_payload_cannot_forge_actor() {
        let command: ApprovalDecisionCommand = serde_json::from_value(json!({
            "work_item_id": "work-item-1",
            "decision": "APPROVE",
            "reason": null,
            "expected_task_version": 1,
            "actor_id": "forged-user",
            "idempotency_key": "request-1"
        }))
        .unwrap();
        assert!(command.actor_id.is_empty());
        assert!(serde_json::to_value(command).unwrap().get("actor_id").is_none());
    }
}
