//! 审批运行时与业务领域强类型命令之间的事务内端口。

use std::{future::Future, pin::Pin};

use database::Executor;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use super::policy::ApprovalDomainAction;

/// 强类型领域动作执行所需的冻结审批上下文。
///
/// 字段为模块私有；生产路径必须经 [`Self::for_decision`] 或
/// [`Self::for_blocked_cancel`] 构造。跨模块只通过只读访问器读取。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalActionContext {
    approval_process_instance_id: String,
    approval_node_execution_id: Option<String>,
    work_item_id: Option<String>,
    business_object_type: String,
    business_object_id: String,
    subject_version: String,
    actor_id: String,
    reason: Option<String>,
    idempotency_key: String,
}

impl ApprovalActionContext {
    /// 构造正式决定使用的领域动作上下文。
    ///
    /// # 参数
    /// * `approval_process_instance_id` - 审批流程实例 ID
    /// * `approval_node_execution_id` - 当前节点执行 ID
    /// * `work_item_id` - 当前待办 ID
    /// * `business_object_type` - 业务对象类型稳定码
    /// * `business_object_id` - 业务对象主键
    /// * `subject_version` - 冻结业务版本十进制字符串
    /// * `actor_id` - 已认证操作人
    /// * `reason` - 可选决定原因
    /// * `idempotency_key` - 调用方幂等键
    ///
    /// # 返回
    /// 两个合法形态之一：决定上下文，执行与任务 ID 均非空。
    ///
    /// # 错误
    /// 缺/空实例、执行、任务、业务对象、操作人或幂等键，空原因，非法主题版本时返回校验错误。
    ///
    /// # 关键业务约束
    /// 跨聚合动作选择仍归 Service；本构造只约束决定形态的身份字段。
    #[allow(clippy::too_many_arguments)]
    pub fn for_decision(
        approval_process_instance_id: impl Into<String>,
        approval_node_execution_id: impl Into<String>,
        work_item_id: impl Into<String>,
        business_object_type: impl Into<String>,
        business_object_id: impl Into<String>,
        subject_version: impl Into<String>,
        actor_id: impl Into<String>,
        reason: Option<String>,
        idempotency_key: impl Into<String>,
    ) -> Result<Self> {
        let execution_id = required_id(approval_node_execution_id, "审批节点执行")?;
        let task_id = required_id(work_item_id, "审批任务")?;
        Ok(Self {
            approval_process_instance_id: required_id(approval_process_instance_id, "审批流程实例")?,
            approval_node_execution_id: Some(execution_id),
            work_item_id: Some(task_id),
            business_object_type: required_id(business_object_type, "业务对象类型")?,
            business_object_id: required_id(business_object_id, "业务对象")?,
            subject_version: parse_subject_version(subject_version)?,
            actor_id: required_id(actor_id, "操作人")?,
            reason: optional_reason(reason)?,
            idempotency_key: required_id(idempotency_key, "幂等键")?,
        })
    }

    /// 构造受阻取消使用的领域动作上下文。
    ///
    /// # 参数
    /// * `approval_process_instance_id` - 审批流程实例 ID
    /// * `approval_node_execution_id` - 当前节点执行 ID
    /// * `work_item_id` - 若调用方误带任务 ID，错误必须携带该任务
    /// * `business_object_type` - 业务对象类型稳定码
    /// * `business_object_id` - 业务对象主键
    /// * `subject_version` - 冻结业务版本十进制字符串
    /// * `actor_id` - 已认证操作人
    /// * `reason` - 非空取消原因
    /// * `idempotency_key` - 调用方幂等键
    ///
    /// # 返回
    /// 两个合法形态之一：受阻取消上下文，任务 ID 固定为空。
    ///
    /// # 错误
    /// 缺/空实例、执行、业务对象、原因或幂等键，误带任务，非法主题版本时返回校验错误。
    ///
    /// # 关键业务约束
    /// 受阻取消不得写入待办 ID；若请求携带任务，错误文案必须包含该任务 ID。
    #[allow(clippy::too_many_arguments)]
    pub fn for_blocked_cancel(
        approval_process_instance_id: impl Into<String>,
        approval_node_execution_id: impl Into<String>,
        work_item_id: Option<String>,
        business_object_type: impl Into<String>,
        business_object_id: impl Into<String>,
        subject_version: impl Into<String>,
        actor_id: impl Into<String>,
        reason: impl Into<String>,
        idempotency_key: impl Into<String>,
    ) -> Result<Self> {
        if let Some(task_id) = work_item_id {
            let task_id = task_id.trim();
            if !task_id.is_empty() {
                return Err(Error::ValidationError(format!(
                    "受阻取消不得携带审批任务 {task_id}"
                )));
            }
            return Err(Error::ValidationError("受阻取消不得携带空审批任务".to_string()));
        }
        let reason = required_id(reason, "取消原因")?;
        Ok(Self {
            approval_process_instance_id: required_id(approval_process_instance_id, "审批流程实例")?,
            approval_node_execution_id: Some(required_id(approval_node_execution_id, "审批节点执行")?),
            work_item_id: None,
            business_object_type: required_id(business_object_type, "业务对象类型")?,
            business_object_id: required_id(business_object_id, "业务对象")?,
            subject_version: parse_subject_version(subject_version)?,
            actor_id: required_id(actor_id, "操作人")?,
            reason: Some(reason),
            idempotency_key: required_id(idempotency_key, "幂等键")?,
        })
    }

    /// 返回审批流程实例 ID。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 构造时写入的实例主键。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 只读；跨聚合动作不得改写此身份。
    pub fn approval_process_instance_id(&self) -> &str {
        &self.approval_process_instance_id
    }

    /// 返回当前节点执行 ID。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 决定与受阻取消均返回非空执行 ID。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 只读。
    pub fn approval_node_execution_id(&self) -> Option<&str> {
        self.approval_node_execution_id.as_deref()
    }

    /// 返回当前待办 ID。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 决定形态返回任务 ID；受阻取消返回 `None`。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 只读；不得在构造后再写入任务身份。
    pub fn work_item_id(&self) -> Option<&str> {
        self.work_item_id.as_deref()
    }

    /// 返回业务对象类型稳定码。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 构造时写入的单据类型代码。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 只读。
    pub fn business_object_type(&self) -> &str {
        &self.business_object_type
    }

    /// 返回业务对象主键。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 构造时写入的业务主键。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 只读。
    pub fn business_object_id(&self) -> &str {
        &self.business_object_id
    }

    /// 返回冻结业务版本。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 十进制主题版本字符串。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 只读。
    pub fn subject_version(&self) -> &str {
        &self.subject_version
    }

    /// 返回已认证操作人。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 构造时写入的操作人 ID。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 只读。
    pub fn actor_id(&self) -> &str {
        &self.actor_id
    }

    /// 返回正式决定或取消原因。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 决定可为空；受阻取消为非空原因。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 只读。
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    /// 返回调用方幂等键。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 构造时写入的规范幂等键文本。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 只读。
    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }
}

/// 事务内领域动作的异步返回类型。
pub type ApprovalActionFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

/// 审批运行时调用所属业务域的强类型命令端口。
///
/// 实现必须只通过领域 Repository 写正式事实，并复用运行时传入的执行器；
/// 禁止另开事务、调用 HTTP Handler 或根据字符串动态执行代码。
pub trait ApprovalDomainActionPort: Send + Sync {
    /// 在审批运行时的唯一外层事务内执行已注册动作。
    ///
    /// # 错误
    /// 业务版本、状态、权限或领域不变量不满足时返回稳定服务错误，外层事务整体回滚。
    fn execute<'a>(
        &'a self,
        action: ApprovalDomainAction,
        context: &'a ApprovalActionContext,
        actor: &'a AuditActor,
        executor: &'a mut dyn Executor,
    ) -> ApprovalActionFuture<'a>;
}

/// 未绑定领域命令时使用的失败关闭端口。
#[derive(Debug, Default)]
pub struct FailClosedApprovalActionPort;

impl ApprovalDomainActionPort for FailClosedApprovalActionPort {
    fn execute<'a>(
        &'a self,
        action: ApprovalDomainAction,
        _context: &'a ApprovalActionContext,
        _actor: &'a AuditActor,
        _executor: &'a mut dyn Executor,
    ) -> ApprovalActionFuture<'a> {
        Box::pin(async move {
            Err(Error::BusinessLogicError(format!(
                "审批领域动作 {} 尚未绑定，已按安全策略拒绝推进",
                action.as_str()
            )))
        })
    }
}

/// 清洗必填身份字段。
///
/// # 参数
/// * `value` - 原始文本
/// * `label` - 字段中文名，用于错误说明
///
/// # 返回
/// 去空白后的非空字符串。
///
/// # 错误
/// 空值或仅空白时返回校验错误。
///
/// # 关键业务约束
/// 本函数不生成默认身份。
fn required_id(value: impl Into<String>, label: &str) -> Result<String> {
    let value = value.into();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Error::ValidationError(format!("{label}不能为空")));
    }
    Ok(trimmed.to_string())
}

/// 清洗可选原因：缺省允许，空白等价于缺省。
///
/// # 参数
/// * `reason` - 可选原因
///
/// # 返回
/// 非空原因或 `None`。
///
/// # 错误
/// 提交了仅空白的原因时返回校验错误。
///
/// # 关键业务约束
/// 决定原因可缺省；一旦提交必须有实质文本。
fn optional_reason(reason: Option<String>) -> Result<Option<String>> {
    match reason {
        None => Ok(None),
        Some(reason) => {
            let trimmed = reason.trim();
            if trimmed.is_empty() {
                return Err(Error::ValidationError("决定原因不能为空".to_string()));
            }
            Ok(Some(trimmed.to_string()))
        }
    }
}

/// 校验冻结主题版本为非空十进制数字字符串。
///
/// # 参数
/// * `subject_version` - 调用方提供的版本文本
///
/// # 返回
/// 去空白后的十进制版本字符串。
///
/// # 错误
/// 空值或含非数字字符时返回校验错误。
///
/// # 关键业务约束
/// 不得把非法版本改写为默认 `0`。
fn parse_subject_version(subject_version: impl Into<String>) -> Result<String> {
    let value = required_id(subject_version, "业务版本")?;
    if value.chars().all(|ch| ch.is_ascii_digit()) {
        return Ok(value);
    }
    Err(Error::ValidationError("业务版本必须是十进制数字".to_string()))
}

#[cfg(test)]
mod tests {
    use database::NoTransaction;
    use entities::AccountKind;

    use super::*;

    #[tokio::test]
    async fn unbound_domain_action_fails_closed() {
        let context = ApprovalActionContext::for_decision(
            "instance-1",
            "exec-1",
            "wi-1",
            "stock_adjustment",
            "adj-1",
            "1",
            "user-1",
            None,
            "request-1",
        )
        .expect("合法决定上下文");
        let error = FailClosedApprovalActionPort
            .execute(
                ApprovalDomainAction::StockAdjustmentSubmit,
                &context,
                &AuditActor::new("user-1".to_string(), "tester".to_string(), AccountKind::Admin),
                &mut NoTransaction,
            )
            .await
            .unwrap_err();
        assert!(matches!(error, Error::BusinessLogicError(_)));
    }

    /// 决定与受阻取消是仅有的两种合法动作上下文形态。
    #[test]
    fn action_context_has_two_legal_shapes() {
        let decision = ApprovalActionContext::for_decision(
            "instance-1",
            "exec-1",
            "wi-1",
            "stock_adjustment",
            "adj-1",
            "1",
            "user-1",
            Some("同意".to_string()),
            "key-1",
        )
        .unwrap();
        assert_eq!(decision.work_item_id(), Some("wi-1"));
        assert_eq!(decision.approval_node_execution_id(), Some("exec-1"));
        assert_eq!(decision.reason(), Some("同意"));

        let blocked = ApprovalActionContext::for_blocked_cancel(
            "instance-1",
            "exec-1",
            None,
            "stock_adjustment",
            "adj-1",
            "1",
            "admin-1",
            "结构损坏",
            "key-2",
        )
        .unwrap();
        assert!(blocked.work_item_id().is_none());
        assert_eq!(blocked.reason(), Some("结构损坏"));
    }

    /// 字段为模块私有；其它模块只能通过构造器与只读访问器使用。
    #[test]
    fn action_context_fields_are_private_outside_this_module() {
        let source = include_str!("action.rs");
        for field in [
            "approval_process_instance_id",
            "approval_node_execution_id",
            "work_item_id",
            "business_object_type",
            "business_object_id",
            "subject_version",
            "actor_id",
            "reason",
            "idempotency_key",
        ] {
            assert!(!source.contains(&format!("pub {field}")), "{field} 不得公开");
        }
        assert!(!include_str!("execution/runtime_service.rs").contains("ApprovalActionContext {"));
        assert!(!include_str!("binding.rs").contains("ApprovalActionContext {"));
        assert!(!include_str!("definition.rs").contains("ApprovalActionContext {"));
    }

    /// 缺/空身份、空原因/幂等键、非法版本必须失败关闭；受阻取消误带任务时错误携带任务。
    #[test]
    fn action_context_constructors_reject_illegal_identity() {
        assert!(ApprovalActionContext::for_decision(
            " ",
            "exec-1",
            "wi-1",
            "stock_adjustment",
            "adj-1",
            "1",
            "user-1",
            None,
            "key-1",
        )
        .is_err());
        assert!(ApprovalActionContext::for_decision(
            "instance-1",
            "",
            "wi-1",
            "stock_adjustment",
            "adj-1",
            "1",
            "user-1",
            None,
            "key-1",
        )
        .is_err());
        assert!(ApprovalActionContext::for_decision(
            "instance-1",
            "exec-1",
            "  ",
            "stock_adjustment",
            "adj-1",
            "1",
            "user-1",
            None,
            "key-1",
        )
        .is_err());
        assert!(ApprovalActionContext::for_decision(
            "instance-1",
            "exec-1",
            "wi-1",
            "stock_adjustment",
            "",
            "1",
            "user-1",
            None,
            "key-1",
        )
        .is_err());
        assert!(ApprovalActionContext::for_decision(
            "instance-1",
            "exec-1",
            "wi-1",
            "stock_adjustment",
            "adj-1",
            "v1",
            "user-1",
            None,
            "key-1",
        )
        .is_err());
        assert!(ApprovalActionContext::for_decision(
            "instance-1",
            "exec-1",
            "wi-1",
            "stock_adjustment",
            "adj-1",
            "1",
            "user-1",
            Some("  ".to_string()),
            "key-1",
        )
        .is_err());
        assert!(ApprovalActionContext::for_decision(
            "instance-1",
            "exec-1",
            "wi-1",
            "stock_adjustment",
            "adj-1",
            "1",
            "user-1",
            None,
            " ",
        )
        .is_err());

        let carried = ApprovalActionContext::for_blocked_cancel(
            "instance-1",
            "exec-1",
            Some("wi-9".to_string()),
            "stock_adjustment",
            "adj-1",
            "1",
            "admin-1",
            "结构损坏",
            "key-2",
        )
        .unwrap_err();
        assert!(carried.to_string().contains("wi-9"));

        let empty_task = ApprovalActionContext::for_blocked_cancel(
            "instance-1",
            "exec-1",
            Some("  ".to_string()),
            "stock_adjustment",
            "adj-1",
            "1",
            "admin-1",
            "结构损坏",
            "key-2",
        )
        .unwrap_err();
        assert!(empty_task.to_string().contains("任务"));

        assert!(ApprovalActionContext::for_blocked_cancel(
            "instance-1",
            "exec-1",
            None,
            "stock_adjustment",
            "adj-1",
            "1",
            "admin-1",
            "  ",
            "key-2",
        )
        .is_err());
    }
}
