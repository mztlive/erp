//! 审批运行时端口与阻塞管理 HTTP 共用的 DTO。

use entities::{
    approval::{
        ApprovalDecision, ApprovalInstance, ApprovalInstanceStatus, ApprovalRuntimeKind,
        ApprovalStepInstance, ApprovalStepStatus,
    },
    common::time::Instant,
    work_item::{AssignmentMode, WorkItem, WorkItemStatus, WorkItemType},
    AccountKind,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// 启动一个已注册审批定义。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StartApprovalCommand {
    /// 编译期注册的定义编码。
    pub definition_key: String,
    /// 业务对象类型。
    pub business_object_type: String,
    /// 业务对象 ID。
    pub business_object_id: String,
    /// 不可变提交或业务版本。
    pub subject_version: String,
    /// 冻结的责任组织。
    pub owner_organization_id: String,
    /// 启动人。
    pub started_by: String,
    /// 业务请求幂等键。
    pub idempotency_key: String,
}

impl StartApprovalCommand {
    /// 返回本次启动可重复计算的确定性审批实例 ID。
    ///
    /// 调用方必须先保证 `definition_key + business_object_type/id + subject_version`
    /// 是同一业务提交的稳定身份；幂等键仍用于业务审计和结果查询，不参与长期主键。
    pub fn deterministic_instance_id(&self) -> String {
        let identity = format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}",
            self.definition_key, self.business_object_type, self.business_object_id, self.subject_version
        );
        format!("approval-instance-{:x}", Sha256::digest(identity.as_bytes()))
    }
}

/// 提交当前审批步骤的正式决定。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubmitDecisionCommand {
    /// 当前待办 ID。
    pub work_item_id: String,
    /// 审批实例 ID。
    pub approval_instance_id: String,
    /// 查询所得当前审批步骤实例 ID。
    pub approval_step_instance_id: String,
    /// 查询所得待办版本。
    pub expected_task_version: u64,
    /// 查询所得实例版本。
    pub expected_instance_version: u64,
    /// 查询所得步骤版本。
    pub expected_step_version: u64,
    /// 查询所得不可变业务版本。
    pub expected_subject_version: String,
    /// 服务端已注册步骤允许的决定。
    pub decision: ApprovalDecision,
    /// 决定原因；驳回和终止时必填。
    pub reason: Option<String>,
    /// 已认证决定人。
    #[serde(skip)]
    pub actor_id: String,
    /// 业务请求幂等键。
    pub idempotency_key: String,
}

/// 取消仍可撤回的审批实例。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CancelApprovalCommand {
    /// 审批实例 ID。
    pub approval_instance_id: String,
    /// 当前步骤实例 ID。
    pub current_step_instance_id: String,
    /// 存在开放待办时的待办 ID。
    pub current_work_item_id: Option<String>,
    /// 查询所得实例版本。
    pub expected_instance_version: u64,
    /// 查询所得步骤版本。
    pub expected_step_version: u64,
    /// 存在开放待办时的查询版本。
    pub expected_task_version: Option<u64>,
    /// 查询所得不可变业务版本。
    pub expected_subject_version: String,
    /// 已认证撤回人。
    #[serde(skip)]
    pub actor_id: String,
    /// 结构化撤回原因。
    pub reason: String,
    /// 业务请求幂等键。
    pub idempotency_key: String,
}

/// 阻塞恢复当前唯一允许的固定动作。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalRecoveryAction {
    /// 重新解析并恢复原当前步骤。
    RetryCurrentStep,
}

/// 管理员恢复阻塞审批的命令。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoverApprovalCommand {
    /// 审批实例 ID，由 HTTP 路径注入。
    #[serde(skip)]
    pub approval_instance_id: String,
    /// 必须仍是实例当前步骤。
    pub current_step_instance_id: String,
    /// 查询所得实例版本。
    pub expected_instance_version: u64,
    /// 查询所得步骤版本。
    pub expected_step_version: u64,
    /// 存在开放待办时的查询版本。
    pub expected_task_version: Option<u64>,
    /// 固定为 `RETRY_CURRENT_STEP`。
    pub recovery_action: ApprovalRecoveryAction,
    /// 结构化恢复原因。
    pub reason: String,
    /// 业务请求幂等键。
    pub idempotency_key: String,
    /// 已认证恢复人，由 HTTP 身份注入。
    #[serde(skip)]
    pub actor_id: String,
    /// HTTP 边界在稳定 policy 版本下形成的恢复授权锚点；运行时必须在事务内复验。
    #[serde(skip)]
    pub authorization: Option<ApprovalRecoveryAuthorization>,
}

/// 阻塞审批恢复的不可伪造授权锚点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRecoveryAuthorization {
    /// 已认证账号类型。
    pub actor_kind: AccountKind,
    /// Casbin 持久化策略单调版本。
    pub policy_revision: u64,
    /// 在该版本下实际授予 `approval_instance:recover` 的角色。
    pub granting_role_ids: Vec<String>,
    /// `None` 表示公司级；`Some` 仅允许列出的组织或团队。
    pub organization_ids: Option<Vec<String>>,
}

/// 审批实例对外稳定视图。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalInstanceView {
    /// 实例 ID。
    pub id: String,
    /// 定义编码。
    pub definition_key: String,
    /// 冻结定义版本。
    pub definition_version: u32,
    /// 运行时类型。
    pub runtime_kind: ApprovalRuntimeKind,
    /// 业务对象类型。
    pub business_object_type: String,
    /// 业务对象 ID。
    pub business_object_id: String,
    /// 冻结业务版本。
    pub subject_version: String,
    /// 启动时冻结的责任组织。
    pub owner_organization_id: String,
    /// 实例状态。
    pub status: ApprovalInstanceStatus,
    /// 当前步骤实例 ID。
    pub current_step_instance_id: Option<String>,
    /// API 乐观锁版本。
    pub instance_version: String,
    /// 权限安全的结构化阻塞码。
    pub blocker_code: Option<String>,
    /// 阻塞时间。
    pub blocked_at: Option<Instant>,
    /// 启动人。
    pub started_by: String,
    /// 启动时间。
    pub started_at: Instant,
    /// 结束时间。
    pub ended_at: Option<Instant>,
}

impl From<ApprovalInstance> for ApprovalInstanceView {
    fn from(instance: ApprovalInstance) -> Self {
        Self {
            id: instance.base.id,
            definition_key: instance.definition_key,
            definition_version: instance.definition_version,
            runtime_kind: instance.runtime_kind,
            business_object_type: instance.business_object_type,
            business_object_id: instance.business_object_id,
            subject_version: instance.subject_version,
            owner_organization_id: instance.owner_organization_id,
            status: instance.status,
            current_step_instance_id: instance.current_step_instance_id.map(|id| id.to_string()),
            instance_version: instance.base.version.to_string(),
            blocker_code: instance.blocker_code,
            blocked_at: instance.blocked_at,
            started_by: instance.started_by,
            started_at: instance.started_at,
            ended_at: instance.ended_at,
        }
    }
}

/// 当前审批步骤对外稳定视图。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalStepInstanceView {
    /// 步骤实例 ID。
    pub id: String,
    /// 所属审批实例 ID。
    pub approval_instance_id: String,
    /// 冻结步骤编码。
    pub step_key: String,
    /// 串行序号。
    pub sequence_no: u32,
    /// 步骤状态。
    pub status: ApprovalStepStatus,
    /// API 乐观锁版本。
    pub step_version: String,
    /// 已形成的决定。
    pub decision: Option<ApprovalDecision>,
    /// 决定原因。
    pub decision_reason: Option<String>,
    /// 决定人。
    pub decided_by: Option<String>,
    /// 决定时间。
    pub decided_at: Option<Instant>,
    /// 权限安全的结构化阻塞码。
    pub blocker_code: Option<String>,
    /// 阻塞时间。
    pub blocked_at: Option<Instant>,
}

impl From<ApprovalStepInstance> for ApprovalStepInstanceView {
    fn from(step: ApprovalStepInstance) -> Self {
        Self {
            id: step.base.id,
            approval_instance_id: step.approval_instance_id.to_string(),
            step_key: step.step_key,
            sequence_no: step.sequence_no,
            status: step.status,
            step_version: step.base.version.to_string(),
            decision: step.decision,
            decision_reason: step.decision_reason,
            decided_by: step.decided_by,
            decided_at: step.decided_at,
            blocker_code: step.blocker_code,
            blocked_at: step.blocked_at,
        }
    }
}

/// 审批接口使用的当前待办安全摘要。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalWorkItemView {
    /// 待办 ID。
    pub id: String,
    /// 固定任务类型。
    pub work_item_type: WorkItemType,
    /// 待办绑定的审批步骤实例；非审批任务为空。
    pub approval_step_instance_id: Option<String>,
    /// 生命周期状态。
    pub status: WorkItemStatus,
    /// 分派模式。
    pub assignment_mode: AssignmentMode,
    /// 责任角色。
    pub owner_role: String,
    /// 责任组织。
    pub owner_organization_id: String,
    /// 当前个人责任人。
    pub owner_user_id: Option<String>,
    /// API 乐观锁版本。
    pub task_version: String,
}

impl From<WorkItem> for ApprovalWorkItemView {
    fn from(item: WorkItem) -> Self {
        Self {
            id: item.base.id,
            work_item_type: item.work_item_type,
            approval_step_instance_id: item.approval_step_instance_id,
            status: item.status,
            assignment_mode: item.assignment_mode,
            owner_role: item.owner_role,
            owner_organization_id: item.owner_organization_id,
            owner_user_id: item.owner_user_id,
            task_version: item.base.version.to_string(),
        }
    }
}

/// 稳定运行时端口的统一结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalRuntimeView {
    /// 最新审批实例事实。
    pub instance: ApprovalInstanceView,
    /// 最新当前或刚完成步骤事实。
    pub step: ApprovalStepInstanceView,
    /// 存在时返回当前开放或刚完成待办摘要。
    pub work_item: Option<ApprovalWorkItemView>,
}

/// 阻塞管理列表查询。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BlockedApprovalListParams {
    /// 必须显式为 `BLOCKED`。
    pub status: Option<ApprovalInstanceStatus>,
    /// 一起始页码。
    #[serde(default = "default_page")]
    pub page: u64,
    /// 单页条数。
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

/// 单条阻塞审批诊断视图。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockedApprovalView {
    /// 阻塞审批实例 ID。
    pub approval_instance_id: String,
    /// 查询所得实例版本。
    pub instance_version: String,
    /// 当前阻塞步骤实例 ID。
    pub current_step_instance_id: String,
    /// 查询所得步骤版本。
    pub step_version: String,
    /// 权限安全的业务对象标签；领域未提供标题时回落稳定 ID。
    pub business_object_label: String,
    /// 结构化阻塞码。
    pub blocker_code: String,
    /// 权限安全且不可用于程序判断的阻塞说明。
    pub blocker_message: String,
    /// 阻塞时间。
    pub blocked_at: Instant,
    /// 阻塞前已存在时返回原待办；不返回任意猜测任务。
    pub work_item: Option<ApprovalWorkItemView>,
    /// 当前身份可执行的管理动作；无恢复权限时为空。
    pub allowed_actions: Vec<ApprovalRecoveryAction>,
}

/// 阻塞审批分页响应。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockedApprovalPage {
    /// 当前页数据。
    pub items: Vec<BlockedApprovalView>,
    /// 授权范围内总数。
    pub total: u64,
    /// 当前页码。
    pub page: u64,
    /// 当前单页条数。
    pub page_size: u32,
}

const fn default_page() -> u64 {
    1
}

const fn default_page_size() -> u32 {
    20
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
    /// 当前 blocker。
    pub blocker_code: String,
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
    use entities::{approval::ApprovalDecision, common::time::Instant};
    use serde_json::json;

    use super::{BlockedApprovalListParams, BlockedApprovalView, SubmitDecisionCommand};

    #[test]
    fn decision_http_payload_cannot_forge_actor_and_keeps_step_identity() {
        let command: SubmitDecisionCommand = serde_json::from_value(json!({
            "work_item_id": "work-item-1",
            "approval_instance_id": "instance-1",
            "approval_step_instance_id": "step-1",
            "expected_task_version": 1,
            "expected_instance_version": 2,
            "expected_step_version": 3,
            "expected_subject_version": "submission-1",
            "decision": "APPROVE",
            "reason": null,
            "actor_id": "forged-user",
            "idempotency_key": "request-1"
        }))
        .unwrap();

        assert_eq!(command.approval_step_instance_id, "step-1");
        assert_eq!(command.decision, ApprovalDecision::Approve);
        assert!(command.actor_id.is_empty());
        assert!(serde_json::to_value(command).unwrap().get("actor_id").is_none());
    }

    #[test]
    fn blocked_projection_contains_only_contract_safe_flat_fields() {
        let value = serde_json::to_value(BlockedApprovalView {
            approval_instance_id: "instance-1".to_string(),
            instance_version: "2".to_string(),
            current_step_instance_id: "step-1".to_string(),
            step_version: "3".to_string(),
            business_object_label: "销售单 SO-1".to_string(),
            blocker_code: "APPROVAL_OWNER_ROLE_UNAVAILABLE".to_string(),
            blocker_message: "审批责任角色当前不可用".to_string(),
            blocked_at: Instant::now(),
            work_item: None,
            allowed_actions: vec![],
        })
        .unwrap();
        let object = value.as_object().unwrap();

        assert_eq!(object.len(), 10);
        for key in [
            "approval_instance_id",
            "instance_version",
            "current_step_instance_id",
            "step_version",
            "business_object_label",
            "blocker_code",
            "blocker_message",
            "blocked_at",
            "work_item",
            "allowed_actions",
        ] {
            assert!(object.contains_key(key), "缺少合同字段 {key}");
        }
        assert!(!object.contains_key("business_object_id"));
        assert!(!object.contains_key("owner_organization_id"));
        assert_eq!(value["instance_version"], "2");
        assert_eq!(value["allowed_actions"], json!([]));
    }

    #[test]
    fn target_runtime_commands_omit_retry_current_step() {
        use super::{
            ApprovalCancelBlockedCommand, ApprovalCancelCommand, ApprovalDecisionCommand,
            ApprovalReassignCommand, ApprovalResumeCommand, ApprovalStartCommand,
        };
        use bpm::model::types::ApprovalDecision;

        let start = ApprovalStartCommand {
            subject_kind: "stock_adjustment".into(),
            subject_id: "adj-1".into(),
            subject_version: 1,
            actor_id: "u1".into(),
            idempotency_key: "k1".into(),
        };
        let decision = ApprovalDecisionCommand {
            work_item_id: "wi-1".into(),
            decision: ApprovalDecision::Approve,
            reason: None,
            expected_task_version: 1,
            idempotency_key: "k2".into(),
            actor_id: "u1".into(),
        };
        let _ = (
            start,
            decision,
            ApprovalCancelCommand {
                approval_process_instance_id: "inst".into(),
                expected_subject_version: 1,
                expected_instance_version: 1,
                expected_execution_version: 1,
                expected_task_version: Some(1),
                reason: "撤回".into(),
                idempotency_key: "k3".into(),
                actor_id: "u1".into(),
            },
            ApprovalResumeCommand {
                approval_process_instance_id: "inst".into(),
                expected_instance_version: 1,
                expected_execution_version: 1,
                expected_assignment_version: 1,
                expected_closed_task_version: None,
                idempotency_key: "k4".into(),
                actor_id: "admin".into(),
            },
            ApprovalReassignCommand {
                approval_process_instance_id: "inst".into(),
                target_user_id: "u9".into(),
                reason: "换人".into(),
                expected_instance_version: 1,
                expected_execution_version: 1,
                expected_assignment_version: 1,
                expected_task_version: None,
                idempotency_key: "k5".into(),
                actor_id: "admin".into(),
            },
            ApprovalCancelBlockedCommand {
                approval_process_instance_id: "inst".into(),
                blocker_code: "OPEN_TASK_CONFLICT".into(),
                expected_instance_version: 1,
                expected_execution_version: 1,
                expected_task_version: None,
                reason: "冻结退出".into(),
                idempotency_key: "k6".into(),
                actor_id: "admin".into(),
            },
        );
    }

    #[test]
    fn blocked_list_rejects_unregistered_query_fields() {
        assert!(serde_json::from_value::<BlockedApprovalListParams>(json!({
            "status": "BLOCKED",
            "page": 1,
            "page_size": 20,
            "decision": "APPROVE"
        }))
        .is_err());
    }
}
