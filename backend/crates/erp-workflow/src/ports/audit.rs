//! Audit persistence consumed by workflow; adapters live at the composition root.

use std::num::NonZeroU32;

use application_core::AuditActor;
use async_trait::async_trait;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// Read-side audit fact consumed by workflow without depending on the audit domain.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowAuditFact {
    /// Actor account id.
    pub actor_id: String,
    /// Business action.
    pub action: String,
    /// Resource type.
    pub resource_type: String,
    /// Resource id when present.
    pub resource_id: Option<String>,
    /// Whether the recorded action succeeded.
    pub success: bool,
    /// Optional message, including command fingerprints.
    pub message: Option<String>,
}

#[cfg(test)]
impl WorkflowAuditFact {
    /// Construct a successful resource audit fact for tests and adapters.
    pub fn successful(
        actor_id: impl Into<String>,
        action: impl Into<String>,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
    ) -> Self {
        Self {
            actor_id: actor_id.into(),
            action: action.into(),
            resource_type: resource_type.into(),
            resource_id: Some(resource_id.into()),
            success: true,
            message: None,
        }
    }
}

/// 允许提交的工作项业务动作；内容只携带安全变更标记。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowAuditOperation {
    Reassigned,
    PurchaseOwnerReassigned,
    Closed,
}

/// 事务外尝试的最小结果，不包含错误正文。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowAuditAttemptResult {
    Failed,
    Rejected,
    Unknown,
}

/// Prepared success audit that workflow can persist through [`WorkflowAuditPort`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedWorkflowAudit {
    /// Stable audit id.
    pub id: String,
    /// Actor account id.
    pub actor_id: String,
    /// Actor login account.
    pub actor_account: String,
    /// Actor kind wire value.
    pub actor_type: String,
    /// 本次已鉴权操作人名称；历史缺失不补齐。
    pub actor_name_snapshot: Option<String>,
    /// 当前请求关联；内部命令明确保持缺失。
    pub request_id: Option<String>,
    /// 同一命令内从一开始的事件序号。
    pub event_sequence: NonZeroU32,
    /// Business action.
    pub action: String,
    /// Resource type.
    pub resource_type: String,
    /// Resource id.
    pub resource_id: String,
    /// 可读说明；不得携带命令摘要。
    pub message: Option<String>,
    /// 已登记的安全业务动作。
    pub operation: Option<WorkflowAuditOperation>,
    /// 独立命令收据关联。
    pub command_id: Option<String>,
}

impl PreparedWorkflowAudit {
    /// 保留同一命令内安全事件的非零顺序。
    /// # 参数
    /// * `event_sequence` - 原命令确定的事件序号。
    /// # 返回
    /// 返回保留操作人、请求及命令关联的事件输入。
    /// # 错误
    /// 无；参数类型保证序号非零。
    pub fn with_event_sequence(mut self, event_sequence: NonZeroU32) -> Self {
        self.event_sequence = event_sequence;
        self
    }

    /// 将动作绑定到独立回执，禁止错配动作与资源。
    /// # 参数
    /// * `command_id` - 原命令身份。
    /// * `operation` - 本域登记的安全动作。
    /// # 返回
    /// 返回安全投影完成的事件输入。
    /// # 错误
    /// 动作、资源或命令身份无效时拒绝。
    pub fn for_command(mut self, command_id: String, operation: WorkflowAuditOperation) -> Result<Self> {
        let (action, resource) = match operation {
            WorkflowAuditOperation::Reassigned => ("work_item.reassign", "work_item"),
            WorkflowAuditOperation::PurchaseOwnerReassigned => {
                ("purchase_order.owner_reassign", "purchase_order")
            },
            WorkflowAuditOperation::Closed => ("work_item.close", "work_item"),
        };
        if self.action != action || self.resource_type != resource || command_id.trim().is_empty() {
            return Err(Error::ValidationError("工作流审计动作或命令身份无效".to_string()));
        }
        self.operation = Some(operation);
        self.command_id = Some(command_id);
        Ok(self)
    }

    /// Build a success resource audit from an authenticated actor.
    ///
    /// # Errors
    /// Empty resource id.
    pub fn resource(
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<Self> {
        Self::resource_with_message(actor, action, resource_type, resource_id, None)
    }

    /// Build a success resource audit with an optional message.
    ///
    /// # Errors
    /// Empty resource id.
    pub fn resource_with_message(
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> Result<Self> {
        Self::resource_with_id(id_generator::next_id(), actor, action, resource_type, resource_id, message)
    }

    /// Build a success resource audit with an explicit id and optional message.
    ///
    /// # Errors
    /// Empty resource id.
    pub fn resource_with_id(
        id: String,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> Result<Self> {
        if resource_id.trim().is_empty() {
            return Err(Error::ValidationError("资源ID不能为空".to_string()));
        }
        let actor_name_snapshot = actor.actor_name_snapshot().map(str::to_string);
        let request_id = actor.request_id().map(str::to_string);
        let (actor_id, actor_account, actor_type) = actor.into_parts();
        Ok(Self {
            id,
            actor_id,
            actor_account,
            actor_type: actor_type.as_str().to_string(),
            actor_name_snapshot,
            request_id,
            event_sequence: NonZeroU32::MIN,
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id,
            message,
            operation: None,
            command_id: None,
        })
    }
}

/// Persist and query workflow audits without depending on the audit domain crate.
#[async_trait]
pub trait WorkflowAuditPort: Send + Sync {
    /// 在事务开始前校验静态动作和操作人。
    fn validate(&self, _audit: &PreparedWorkflowAudit) -> Result<()> {
        Ok(())
    }

    /// 在原事务已结束后保存独立尝试，禁止用于业务回放。
    async fn persist_attempt(
        &self,
        _audit: &PreparedWorkflowAudit,
        _result: WorkflowAuditAttemptResult,
    ) -> Result<()> {
        Err(Error::Internal("工作流尝试审计端口未接线".into()))
    }

    /// Persist a prepared success audit using the caller executor.
    async fn persist(&self, audit: &PreparedWorkflowAudit, executor: &mut dyn Executor) -> Result<()>;
}

/// Fail-closed audit port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedAuditPort;

#[async_trait]
impl WorkflowAuditPort for FailClosedAuditPort {
    async fn persist(&self, _audit: &PreparedWorkflowAudit, _executor: &mut dyn Executor) -> Result<()> {
        Err(Error::Internal("审计端口未接线".to_string()))
    }
}
