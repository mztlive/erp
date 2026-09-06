//! 单据审批任务构造与审批运行时完成、关闭。

use bpm::ApprovalNodeExecutionId;
use serde::{Deserialize, Serialize};

use erp_core::common::time::Instant;
use erp_core::ids::WorkItemId;
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};

use super::validation::USER_ID_MAX_LEN;
use super::{
    AssignmentSource, WorkItem, WorkItemCloseData, WorkItemData, WorkItemPriority, WorkItemStatus,
    WorkItemType,
};

/// 单据审批任务进入决定编排前的纯领域校验失败原因。
#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecisionTaskError {
    /// 当前任务不是单据审批任务。
    #[error("不是单据审批任务")]
    NotDocumentApproval,
    /// 操作人不是当前个人责任人。
    #[error("无权执行该审批动作")]
    NotCurrentOwner,
    /// 任务已经完成或关闭。
    #[error("APPROVAL_TASK_NOT_OPEN")]
    NotOpen,
    /// 调用方持有的任务版本已经过期。
    #[error("任务版本已变化，请刷新后重试")]
    VersionConflict,
    /// 审批任务缺少节点执行引用。
    #[error("APPROVAL_TASK_NOT_OPEN")]
    MissingExecution,
}

/// 审批运行时对同一节点执行关联开放任务的终结方式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalRuntimeTaskEnding {
    /// 当前审批决定已经完成。
    Complete,
    /// 当前执行因受控原因关闭。
    Close {
        /// 审批运行时提供的稳定关闭原因。
        reason: String,
    },
}

/// 通用单据审批任务的创建数据。责任人、角色、组织和执行 ID 均必填。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentApprovalWorkItemData {
    /// 当前节点执行。
    pub approval_node_execution_id: ApprovalNodeExecutionId,
    /// 业务对象类型。
    pub business_object_type: String,
    /// 业务对象 ID。
    pub business_object_id: String,
    /// 被审批的冻结提交版本。
    pub subject_version: String,
    /// 合同签署的责任角色。
    pub owner_role: String,
    /// 责任组织。
    pub owner_organization_id: String,
    /// 当前实例审批人。
    pub owner_user_id: String,
    /// 优先级。
    pub priority: WorkItemPriority,
    /// 时限。
    pub due_at: Option<Instant>,
}

impl WorkItem {
    /// 创建指定到人的单据审批任务。
    ///
    /// 必须同时提供非空责任人、角色、组织、审批运行时来源和节点执行 ID。
    ///
    /// # 参数
    /// * `id` - 任务主键
    /// * `data` - 审批任务数据
    /// * `at` - 创建时间
    ///
    /// # 错误
    /// 任一必填责任字段为空或超长时返回错误。
    pub fn new_document_approval(
        id: WorkItemId,
        data: DocumentApprovalWorkItemData,
        at: Instant,
    ) -> Result<Self> {
        let owner_user_id = normalize_required_text(
            data.owner_user_id,
            "审批任务责任人不能为空",
            USER_ID_MAX_LEN,
            "审批任务责任人过长",
        )?;
        let generic = WorkItemData {
            work_item_type: WorkItemType::ImportBusinessConfirmation,
            business_object_type: data.business_object_type,
            business_object_id: data.business_object_id,
            subject_version: data.subject_version,
            owner_role: data.owner_role,
            owner_organization_id: data.owner_organization_id,
            owner_user_id: owner_user_id.clone(),
            assignment_source: AssignmentSource::ApprovalRuntime,
            priority: data.priority,
            due_at: data.due_at,
            reason_code: None,
            impact_summary: None,
        };
        let mut item = Self::new_at_with_optional_responsibility(id, generic, None, Vec::new(), at)?;
        item.work_item_type = WorkItemType::DocumentApproval;
        item.approval_node_execution_id = Some(data.approval_node_execution_id);
        item.assignment_source = AssignmentSource::ApprovalRuntime;
        Ok(item)
    }

    /// 校验单据审批任务可由当前操作人按给定版本进入决定编排。
    ///
    /// # 参数
    /// * `actor_id` - 当前已认证操作人 ID
    /// * `expected_version` - 调用方期望的任务乐观锁版本
    ///
    /// # 返回
    /// 返回任务绑定的审批节点执行 ID。
    ///
    /// # 错误
    /// 任务类型、责任人、开放状态、版本或执行引用不满足时返回对应错误。
    ///
    /// # 关键业务约束
    /// 校验顺序固定为任务类型、当前责任人、开放状态、版本和执行引用，避免 Service 重复状态判断。
    pub fn approval_execution_for_decision(
        &self,
        actor_id: &str,
        expected_version: u64,
    ) -> std::result::Result<ApprovalNodeExecutionId, ApprovalDecisionTaskError> {
        if self.work_item_type != WorkItemType::DocumentApproval {
            return Err(ApprovalDecisionTaskError::NotDocumentApproval);
        }
        if self.owner_user_id.as_deref() != Some(actor_id) {
            return Err(ApprovalDecisionTaskError::NotCurrentOwner);
        }
        if self.status != WorkItemStatus::Open {
            return Err(ApprovalDecisionTaskError::NotOpen);
        }
        if self.base.version != expected_version {
            return Err(ApprovalDecisionTaskError::VersionConflict);
        }
        self.approval_node_execution_id
            .clone()
            .ok_or(ApprovalDecisionTaskError::MissingExecution)
    }

    /// 由审批运行时完成当前开放的单据审批任务。
    ///
    /// # 错误
    /// 不是单据审批任务、任务非开放或执行人不是当前责任人时返回错误。
    pub fn complete_by_approval_runtime(
        &mut self,
        completed_by: impl Into<String>,
        at: Instant,
    ) -> Result<()> {
        self.ensure_document_approval()?;
        self.complete_open(completed_by, at)
    }

    /// 由审批运行时关闭当前开放的单据审批任务。
    ///
    /// # 错误
    /// 不是单据审批任务、任务非开放或关闭数据非法时返回错误。
    pub fn close_by_approval_runtime(
        &mut self,
        closed_by: impl Into<String>,
        data: WorkItemCloseData,
        at: Instant,
    ) -> Result<()> {
        self.ensure_document_approval()?;
        self.close_open(closed_by, data, at)
    }

    /// 随审批实例取消关闭当前开放任务。
    ///
    /// 只有带节点执行引用的单据审批任务可以使用本入口；返回实体保持原持久化
    /// 版本，供仓储以 `OPEN + version + execution_id` 执行 CAS。
    ///
    /// # 参数
    /// * `closed_by` - 撤回审批的操作人
    /// * `reason` - 不可为空的撤回原因
    /// * `at` - 取消发生时间
    ///
    /// # 返回
    /// 返回已进入 `CLOSED` 的任务快照。
    ///
    /// # 错误
    /// 任务类型错误、缺少节点执行、任务非开放或关闭字段非法时返回错误。
    pub fn close_for_approval_cancellation(
        mut self,
        closed_by: impl Into<String>,
        reason: impl Into<String>,
        at: Instant,
    ) -> Result<Self> {
        self.ensure_document_approval()?;
        if self.approval_node_execution_id.is_none() {
            return Err(Error::from("单据审批任务缺少节点执行引用"));
        }
        self.close_open(
            closed_by,
            WorkItemCloseData {
                close_reason: reason.into(),
            },
            at,
        )?;
        Ok(self)
    }

    /// 批量形成审批取消所需的已关闭任务快照。
    ///
    /// 任一任务不满足关闭规则时整体返回错误，不产生部分持久化结果；空集合用于
    /// 已受阻且没有开放任务的合法取消路径。
    ///
    /// # 参数
    /// * `items` - 当前节点执行关联的开放审批任务
    /// * `closed_by` - 撤回审批的操作人
    /// * `reason` - 不可为空的撤回原因
    /// * `at` - 取消发生时间
    ///
    /// # 返回
    /// 返回与输入顺序一致的已关闭任务快照。
    ///
    /// # 错误
    /// 任一任务不满足 [`Self::close_for_approval_cancellation`] 规则时返回错误。
    pub fn close_all_for_approval_cancellation(
        items: Vec<Self>,
        closed_by: &str,
        reason: &str,
        at: Instant,
    ) -> Result<Vec<Self>> {
        items
            .into_iter()
            .map(|item| item.close_for_approval_cancellation(closed_by, reason, at))
            .collect()
    }

    /// 批量终结同一节点执行关联的全部开放审批任务。
    ///
    /// 本方法只形成确定性的实体快照，不执行 I/O。任一任务类型、执行引用、
    /// 当前责任或关闭原因不满足时，整批返回错误，调用方不得持久化部分结果。
    ///
    /// # 参数
    /// * `items` - Repository 按 execution 读取的全部开放审批任务
    /// * `execution_id` - 本次审批运行时已经结束的节点执行
    /// * `actor_id` - 当前决定人或关闭操作人
    /// * `ending` - 完成或受控关闭方式
    /// * `at` - 终结时间
    ///
    /// # 返回
    /// 返回与输入顺序一致的终态任务快照。
    ///
    /// # 错误
    /// 任务不属于指定执行、不是开放单据审批任务、完成责任人不一致或关闭原因非法时返回错误。
    pub fn end_all_for_approval_execution(
        mut items: Vec<Self>,
        execution_id: &ApprovalNodeExecutionId,
        actor_id: &str,
        ending: &ApprovalRuntimeTaskEnding,
        at: Instant,
    ) -> Result<Vec<Self>> {
        for item in &mut items {
            if item.approval_node_execution_id.as_ref() != Some(execution_id) {
                return Err(Error::from("审批任务不属于当前节点执行"));
            }
            match ending {
                ApprovalRuntimeTaskEnding::Complete => {
                    item.complete_by_approval_runtime(actor_id, at)?;
                }
                ApprovalRuntimeTaskEnding::Close { reason } => {
                    item.close_by_approval_runtime(
                        actor_id,
                        WorkItemCloseData {
                            close_reason: reason.clone(),
                        },
                        at,
                    )?;
                }
            }
        }
        Ok(items)
    }

    fn ensure_document_approval(&self) -> Result<()> {
        if self.work_item_type != WorkItemType::DocumentApproval {
            return Err(Error::from("只有单据审批任务可以由审批运行时完成或关闭"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use bpm::ApprovalNodeExecutionId;

    use super::super::{
        approval_item, direct_data, ApprovalDecisionTaskError, ApprovalRuntimeTaskEnding, WorkItem,
        WorkItemStatus, WorkItemType,
    };
    use erp_core::common::state::ensure_transition;
    use erp_core::common::time::Instant;
    use erp_core::ids::WorkItemId;

    /// 单据审批任务固定带个人责任与节点执行，且禁止通用改派。
    ///
    /// 审批运行时完成后任务进入不可逆终态。
    #[test]
    fn document_approval_requires_owner_and_execution() {
        let mut item = approval_item("wi-approval");
        assert_eq!(item.work_item_type, WorkItemType::DocumentApproval);
        assert!(item.reassign("bob", Instant::from_unix_secs(110)).is_err());
        item.complete_by_approval_runtime("alice", Instant::from_unix_secs(110))
            .unwrap();
        assert_eq!(item.status, WorkItemStatus::Completed);
        assert!(ensure_transition(WorkItemStatus::Open, WorkItemStatus::Completed).is_ok());
    }

    /// 决定前置规则直接返回执行引用，并区分责任、状态、版本与引用失败。
    #[test]
    fn approval_decision_preconditions_are_owned_by_work_item() {
        let item = approval_item("wi-decision");
        let version = item.base.version;
        assert_eq!(
            item.approval_execution_for_decision("alice", version)
                .unwrap()
                .as_ref(),
            "exec-1"
        );
        assert_eq!(
            item.approval_execution_for_decision("bob", version),
            Err(ApprovalDecisionTaskError::NotCurrentOwner)
        );
        assert_eq!(
            item.approval_execution_for_decision("alice", version + 1),
            Err(ApprovalDecisionTaskError::VersionConflict)
        );

        let mut closed = item.clone();
        closed
            .close_by_approval_runtime(
                "alice",
                super::WorkItemCloseData {
                    close_reason: "运行时关闭".to_string(),
                },
                Instant::from_unix_secs(110),
            )
            .unwrap();
        assert_eq!(
            closed.approval_execution_for_decision("alice", version),
            Err(ApprovalDecisionTaskError::NotOpen)
        );

        let mut missing = item;
        missing.approval_node_execution_id = None;
        assert_eq!(
            missing.approval_execution_for_decision("alice", version),
            Err(ApprovalDecisionTaskError::MissingExecution)
        );
    }

    /// 审批取消关闭任务时保留 CAS 版本并写入受控关闭事实。
    ///
    /// 批量入口保持输入顺序，受阻取消的空任务集合也是合法结果。
    #[test]
    fn approval_cancellation_closes_open_tasks_without_advancing_version() {
        let item = approval_item("wi-cancel");
        let expected_version = item.base.version;
        let closed = WorkItem::close_all_for_approval_cancellation(
            vec![item],
            "submitter",
            "撤回重改",
            Instant::from_unix_secs(120),
        )
        .unwrap();

        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].base.version, expected_version);
        assert_eq!(closed[0].status, WorkItemStatus::Closed);
        assert_eq!(closed[0].closed_by.as_deref(), Some("submitter"));
        assert_eq!(closed[0].close_reason.as_deref(), Some("撤回重改"));
        assert_eq!(
            closed[0].approval_node_execution_id.as_ref().map(AsRef::as_ref),
            Some("exec-1")
        );
        assert!(WorkItem::close_all_for_approval_cancellation(
            Vec::new(),
            "submitter",
            "受阻取消",
            Instant::from_unix_secs(121),
        )
        .unwrap()
        .is_empty());
    }

    /// 审批取消不得关闭独立任务、缺失执行引用或已终态任务。
    ///
    /// 任一非法任务都会使批量规则整体失败关闭。
    #[test]
    fn approval_cancellation_rejects_invalid_task_facts() {
        let generic = WorkItem::new_at(
            WorkItemId::new("wi-generic"),
            direct_data(),
            Instant::from_unix_secs(100),
        )
        .unwrap();
        assert!(generic
            .close_for_approval_cancellation("submitter", "撤回", Instant::from_unix_secs(120))
            .is_err());

        let mut missing_execution = approval_item("wi-missing-execution");
        missing_execution.approval_node_execution_id = None;
        assert!(missing_execution
            .close_for_approval_cancellation("submitter", "撤回", Instant::from_unix_secs(120))
            .is_err());

        let completed = approval_item("wi-completed");
        let mut completed = completed;
        completed
            .complete_by_approval_runtime("alice", Instant::from_unix_secs(110))
            .unwrap();
        assert!(completed
            .close_for_approval_cancellation("submitter", "撤回", Instant::from_unix_secs(120))
            .is_err());

        assert!(WorkItem::close_all_for_approval_cancellation(
            vec![approval_item("wi-valid"), approval_item("wi-invalid")],
            "",
            "撤回",
            Instant::from_unix_secs(120),
        )
        .is_err());
    }

    /// 同一执行的遗留重复开放任务必须按确定性顺序全部完成或关闭。
    #[test]
    fn approval_runtime_ends_every_open_task_for_execution() {
        let execution_id = ApprovalNodeExecutionId::new("exec-1");
        let completed = WorkItem::end_all_for_approval_execution(
            vec![approval_item("wi-1"), approval_item("wi-2")],
            &execution_id,
            "alice",
            &ApprovalRuntimeTaskEnding::Complete,
            Instant::from_unix_secs(120),
        )
        .unwrap();
        assert_eq!(completed.len(), 2);
        assert!(completed
            .iter()
            .all(|item| item.status == WorkItemStatus::Completed));

        let closed = WorkItem::end_all_for_approval_execution(
            vec![approval_item("wi-3"), approval_item("wi-4")],
            &execution_id,
            "runtime",
            &ApprovalRuntimeTaskEnding::Close {
                reason: "APPROVAL_RUNTIME_BLOCKED".to_string(),
            },
            Instant::from_unix_secs(121),
        )
        .unwrap();
        assert!(closed.iter().all(|item| {
            item.status == WorkItemStatus::Closed
                && item.close_reason.as_deref() == Some("APPROVAL_RUNTIME_BLOCKED")
        }));
    }

    /// 批量终结必须拒绝外来执行和完成责任人漂移。
    #[test]
    fn approval_runtime_batch_rejects_foreign_execution_or_owner() {
        assert!(WorkItem::end_all_for_approval_execution(
            vec![approval_item("wi-foreign")],
            &ApprovalNodeExecutionId::new("exec-other"),
            "alice",
            &ApprovalRuntimeTaskEnding::Complete,
            Instant::from_unix_secs(120),
        )
        .is_err());
        assert!(WorkItem::end_all_for_approval_execution(
            vec![approval_item("wi-owner")],
            &ApprovalNodeExecutionId::new("exec-1"),
            "bob",
            &ApprovalRuntimeTaskEnding::Complete,
            Instant::from_unix_secs(120),
        )
        .is_err());
    }
}
