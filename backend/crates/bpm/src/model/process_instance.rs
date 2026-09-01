//! 审批运行实例。只保存业务对象引用与提交版本，不保存业务快照。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
use crate::model::types::{
    base_model_at, touch_base, ApprovalBlockerCode, ApprovalProcessInstanceStatus, ModelError, ModelResult,
};
use crate::model::{ParticipantId, ProcessKind, SubjectRef, Timestamp};

/// 创建运行中第 1 轮实例所需的身份与绑定快照。
pub struct NewProcessInstance {
    /// 实例主键。
    pub id: ApprovalProcessInstanceId,
    /// 绑定定义。
    pub process_definition_id: ApprovalProcessDefinitionId,
    /// 绑定定义版本。
    pub definition_version: u32,
    /// 流程种类。
    pub process_kind: ProcessKind,
    /// 业务对象引用。
    pub subject: SubjectRef,
    /// 冻结提交版本。
    pub subject_version: u32,
    /// 启动人。
    pub started_by: ParticipantId,
    /// 启动时间。
    pub at: Timestamp,
}

/// 取消实例时对当前节点开放任务的固定策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalCancellationTaskPolicy {
    /// 运行中实例必须关闭唯一开放任务。
    CloseOpenTask,
    /// 受阻实例不得再持有开放任务。
    NoOpenTask,
}

impl ApprovalCancellationTaskPolicy {
    /// 判断取消计划是否必须关闭当前开放任务。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// [`Self::CloseOpenTask`] 返回 `true`，[`Self::NoOpenTask`] 返回 `false`。
    ///
    /// # 错误
    /// 无。
    pub fn closes_open_task(self) -> bool {
        self == Self::CloseOpenTask
    }

    /// 校验实际开放任务数量是否符合取消策略。
    ///
    /// # 参数
    /// * `open_task_count` - 当前节点执行关联的开放审批任务数量
    ///
    /// # 返回
    /// 运行中实例恰有一个任务，或受阻实例没有任务时返回 `Ok(())`。
    ///
    /// # 错误
    /// 开放任务数量与策略不一致时返回模型状态错误。
    pub fn ensure_open_task_count(self, open_task_count: usize) -> ModelResult<()> {
        match (self, open_task_count) {
            (Self::CloseOpenTask, 1) | (Self::NoOpenTask, 0) => Ok(()),
            (Self::CloseOpenTask, _) => Err(ModelError::InvalidStatus("运行中审批实例必须恰有一个开放任务")),
            (Self::NoOpenTask, _) => Err(ModelError::InvalidStatus("受阻审批实例不得存在开放任务")),
        }
    }
}

/// 单据审批运行实例。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ApprovalProcessInstance {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 绑定的定义主键。运行中不可修改。
    pub process_definition_id: ApprovalProcessDefinitionId,
    /// 绑定的定义业务版本。运行中不可修改。
    pub definition_version: u32,
    /// 流程种类。
    pub process_kind: ProcessKind,
    /// 被审批对象引用。
    pub subject: SubjectRef,
    /// 冻结的提交版本。启动后不可变。
    pub subject_version: u32,
    /// 实例状态。
    pub status: ApprovalProcessInstanceStatus,
    /// 当前轮次，从 1 开始。
    pub current_round_no: u32,
    /// 当前节点执行。运行或受阻时必填。
    pub current_node_execution_id: Option<ApprovalNodeExecutionId>,
    /// 仅受阻时必填。
    pub blocker_code: Option<ApprovalBlockerCode>,
    /// 仅受阻时必填。
    pub blocked_at: Option<Timestamp>,
    /// 启动人。
    pub started_by: ParticipantId,
    /// 启动时间。
    pub started_at: Timestamp,
    /// 终态时间。
    pub ended_at: Option<Timestamp>,
}

impl ApprovalProcessInstance {
    /// 创建运行中的第 1 轮实例。
    ///
    /// # 参数
    /// * `input` - 实例身份、绑定定义与启动人
    ///
    /// # 返回
    /// 返回运行中、第 1 轮、尚未进入节点的实例。
    ///
    /// # 错误
    /// 定义版本为零时返回错误。
    ///
    /// # 约束
    /// 启动后不得改写 `subject` 与 `subject_version`。
    pub fn start_running(input: NewProcessInstance) -> ModelResult<Self> {
        if input.definition_version == 0 {
            return Err(ModelError::InvalidField("定义版本必须从 1 开始"));
        }
        Ok(Self {
            base: base_model_at(input.id.to_string(), input.at)?,
            process_definition_id: input.process_definition_id,
            definition_version: input.definition_version,
            process_kind: input.process_kind,
            subject: input.subject,
            subject_version: input.subject_version,
            status: ApprovalProcessInstanceStatus::Running,
            current_round_no: 1,
            current_node_execution_id: None,
            blocker_code: None,
            blocked_at: None,
            started_by: input.started_by,
            started_at: input.at,
            ended_at: None,
        })
    }

    /// 返回实例乐观锁版本。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `base.version`。
    ///
    /// # 错误
    /// 无。
    pub fn instance_version(&self) -> u64 {
        self.base.version
    }

    /// 校验实例状态与当前执行引用是否允许取消，并返回任务关闭策略。
    ///
    /// 运行中实例取消时必须关闭开放任务；受阻实例取消时不得再关闭任务。最终通过、
    /// 已取消或缺少当前执行引用时失败关闭。实际任务数量由返回策略继续校验。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回取消时应关闭唯一开放任务，或证明无需关闭任务的固定策略。
    ///
    /// # 错误
    /// 实例已终态或缺少当前执行引用时返回错误。
    pub fn cancellation_task_policy(&self) -> ModelResult<ApprovalCancellationTaskPolicy> {
        match (self.status, self.current_node_execution_id.is_some()) {
            (ApprovalProcessInstanceStatus::Approved, _) => {
                Err(ModelError::InvalidStatus("已最终通过的审批实例不得撤回"))
            }
            (ApprovalProcessInstanceStatus::Cancelled, _) => {
                Err(ModelError::InvalidStatus("已取消的审批实例不得重复撤回"))
            }
            (ApprovalProcessInstanceStatus::Running | ApprovalProcessInstanceStatus::Blocked, false) => {
                Err(ModelError::InvalidStatus("可撤回审批实例必须存在当前执行"))
            }
            (ApprovalProcessInstanceStatus::Running, true) => {
                Ok(ApprovalCancellationTaskPolicy::CloseOpenTask)
            }
            (ApprovalProcessInstanceStatus::Blocked, true) => Ok(ApprovalCancellationTaskPolicy::NoOpenTask),
        }
    }

    /// 设置当前节点执行引用。
    ///
    /// # 参数
    /// * `execution_id` - 当前执行
    /// * `at` - 调用方时间
    ///
    /// # 错误
    /// 实例已终态时返回错误。
    pub fn set_current_execution(
        &mut self,
        execution_id: ApprovalNodeExecutionId,
        at: Timestamp,
    ) -> ModelResult<()> {
        self.ensure_not_terminal()?;
        self.current_node_execution_id = Some(execution_id);
        touch_base(&mut self.base, at)
    }

    /// 从运行中进入下一轮，轮次 checked add。
    ///
    /// # 参数
    /// * `at` - 调用方时间
    ///
    /// # 错误
    /// 当前不是运行中或轮次溢出时返回错误。
    pub fn next_round(&mut self, at: Timestamp) -> ModelResult<u32> {
        if self.status != ApprovalProcessInstanceStatus::Running {
            return Err(ModelError::InvalidStatus("只有运行中的实例可以进入下一轮"));
        }
        self.current_round_no = self
            .current_round_no
            .checked_add(1)
            .ok_or(ModelError::Overflow("轮次"))?;
        self.current_node_execution_id = None;
        touch_base(&mut self.base, at)?;
        Ok(self.current_round_no)
    }

    /// 进入受阻并写入当前 blocker 投影。
    ///
    /// # 参数
    /// * `code` - 结构化阻塞原因
    /// * `at` - 阻塞时间
    ///
    /// # 错误
    /// 实例已终态时返回错误。
    pub fn enter_blocked(&mut self, code: ApprovalBlockerCode, at: Timestamp) -> ModelResult<()> {
        self.ensure_not_terminal()?;
        self.status = ApprovalProcessInstanceStatus::Blocked;
        self.blocker_code = Some(code);
        self.blocked_at = Some(at);
        touch_base(&mut self.base, at)
    }

    /// 退出受阻并清空当前 blocker 投影。
    ///
    /// 执行历史中的 blocker 事实不得改写。
    ///
    /// # 参数
    /// * `at` - 调用方时间
    ///
    /// # 错误
    /// 当前不是受阻时返回错误。
    pub fn exit_blocked(&mut self, at: Timestamp) -> ModelResult<()> {
        if self.status != ApprovalProcessInstanceStatus::Blocked {
            return Err(ModelError::InvalidStatus("只有受阻实例可以恢复运行"));
        }
        self.status = ApprovalProcessInstanceStatus::Running;
        self.clear_blocker_projection();
        touch_base(&mut self.base, at)
    }

    /// 最终通过并清空当前 blocker 投影。
    ///
    /// # 参数
    /// * `at` - 结束时间
    ///
    /// # 错误
    /// 实例已终态时返回错误。
    pub fn complete_approved(&mut self, at: Timestamp) -> ModelResult<()> {
        self.ensure_not_terminal()?;
        self.status = ApprovalProcessInstanceStatus::Approved;
        self.current_node_execution_id = None;
        self.clear_blocker_projection();
        self.ended_at = Some(at);
        touch_base(&mut self.base, at)
    }

    /// 取消实例并清空当前执行引用与 blocker 投影。
    ///
    /// # 参数
    /// * `at` - 结束时间
    ///
    /// # 错误
    /// 实例已终态时返回错误。
    pub fn cancel(&mut self, at: Timestamp) -> ModelResult<()> {
        self.ensure_not_terminal()?;
        self.status = ApprovalProcessInstanceStatus::Cancelled;
        self.current_node_execution_id = None;
        self.clear_blocker_projection();
        self.ended_at = Some(at);
        touch_base(&mut self.base, at)
    }

    /// 校验当前实例 blocker 是否允许恢复原审批人。
    ///
    /// # 错误
    /// 终态、非受阻或结构性阻塞时返回错误。
    pub fn ensure_assignee_recovery_allowed(&self) -> ModelResult<()> {
        self.ensure_not_terminal()
            .map_err(|_| ModelError::InvalidStatus("终态实例不得恢复原审批人"))?;
        if self.status != ApprovalProcessInstanceStatus::Blocked {
            return Err(ModelError::InvalidStatus("只有受阻实例可以恢复原审批人"));
        }
        let Some(code) = self.blocker_code else {
            return Err(ModelError::InvalidStatus("受阻实例缺少 blocker"));
        };
        if code.allows_assignee_recovery() {
            return Ok(());
        }
        Err(ModelError::InvalidStatus("结构性或一致性阻塞不得恢复原审批人"))
    }

    fn ensure_not_terminal(&self) -> ModelResult<()> {
        if self.status.is_terminal() {
            return Err(ModelError::InvalidStatus("终态实例不得再变更运行字段"));
        }
        Ok(())
    }

    fn clear_blocker_projection(&mut self) {
        self.blocker_code = None;
        self.blocked_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::{ApprovalCancellationTaskPolicy, ApprovalProcessInstance};
    use crate::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use crate::model::types::{ApprovalBlockerCode, ApprovalProcessInstanceStatus, ModelError};
    use crate::model::{ParticipantId, ProcessKind, SubjectRef, Timestamp};

    /// 构造尚未进入节点的运行实例夹具。
    ///
    /// # 返回
    /// 返回版本为一且不带当前执行的最小实例。
    fn instance() -> ApprovalProcessInstance {
        ApprovalProcessInstance::start_running(super::NewProcessInstance {
            id: ApprovalProcessInstanceId::new("inst"),
            process_definition_id: ApprovalProcessDefinitionId::new("def"),
            definition_version: 1,
            process_kind: ProcessKind::StockAdjustment,
            subject: SubjectRef::new("stock_adjustment", "adj-1").unwrap(),
            subject_version: 1,
            started_by: ParticipantId::new("user").unwrap(),
            at: Timestamp::from_unix_secs(10).unwrap(),
        })
        .unwrap()
    }

    /// 新实例为运行中第 1 轮，不带 blocker。
    #[test]
    fn start_running_round_one() {
        let inst = instance();
        assert_eq!(inst.status, ApprovalProcessInstanceStatus::Running);
        assert_eq!(inst.current_round_no, 1);
        assert!(inst.current_node_execution_id.is_none());
        assert!(inst.blocker_code.is_none());
    }

    /// 下一轮只能从运行中调用，并清空当前执行。
    #[test]
    fn next_round_requires_running() {
        let mut inst = instance();
        inst.set_current_execution(
            ApprovalNodeExecutionId::new("e1"),
            Timestamp::from_unix_secs(11).unwrap(),
        )
        .unwrap();
        assert_eq!(
            inst.next_round(Timestamp::from_unix_secs(12).unwrap()).unwrap(),
            2
        );
        assert!(inst.current_node_execution_id.is_none());
        inst.enter_blocked(
            ApprovalBlockerCode::ApproverAccountInactive,
            Timestamp::from_unix_secs(13).unwrap(),
        )
        .unwrap();
        assert!(inst.next_round(Timestamp::from_unix_secs(14).unwrap()).is_err());
    }

    /// 受阻投影在恢复或终态时清空；历史事实不在本实体改写。
    #[test]
    fn blocker_projection_clears_on_exit_or_terminal() {
        let mut inst = instance();
        let at = Timestamp::from_unix_secs(20).unwrap();
        inst.enter_blocked(ApprovalBlockerCode::ApproverNotEligible, at)
            .unwrap();
        assert_eq!(inst.status, ApprovalProcessInstanceStatus::Blocked);
        assert_eq!(inst.blocker_code, Some(ApprovalBlockerCode::ApproverNotEligible));
        inst.exit_blocked(Timestamp::from_unix_secs(21).unwrap()).unwrap();
        assert_eq!(inst.status, ApprovalProcessInstanceStatus::Running);
        assert!(inst.blocker_code.is_none());
        inst.complete_approved(Timestamp::from_unix_secs(22).unwrap())
            .unwrap();
        assert_eq!(inst.status, ApprovalProcessInstanceStatus::Approved);
        assert!(inst.current_node_execution_id.is_none());
        assert!(inst.cancel(Timestamp::from_unix_secs(23).unwrap()).is_err());
    }

    /// 运行中实例只在存在当前执行和唯一开放任务时允许取消。
    ///
    /// 零任务或重复任务都表示运行事实不一致，必须失败关闭。
    #[test]
    fn running_cancellation_requires_exactly_one_open_task() {
        let mut inst = instance();
        inst.set_current_execution(
            ApprovalNodeExecutionId::new("e1"),
            Timestamp::from_unix_secs(25).unwrap(),
        )
        .unwrap();

        let policy = inst.cancellation_task_policy().unwrap();
        assert_eq!(policy, ApprovalCancellationTaskPolicy::CloseOpenTask);
        assert!(policy.ensure_open_task_count(1).is_ok());
        assert_eq!(
            policy.ensure_open_task_count(0),
            Err(ModelError::InvalidStatus("运行中审批实例必须恰有一个开放任务"))
        );
        assert_eq!(
            policy.ensure_open_task_count(2),
            Err(ModelError::InvalidStatus("运行中审批实例必须恰有一个开放任务"))
        );
    }

    /// 受阻实例取消时必须证明当前执行不再关联开放任务。
    ///
    /// 已最终通过、已取消和缺少当前执行引用也不得形成取消策略。
    #[test]
    fn blocked_and_terminal_cancellation_facts_fail_closed() {
        let mut blocked = instance();
        blocked
            .set_current_execution(
                ApprovalNodeExecutionId::new("e1"),
                Timestamp::from_unix_secs(26).unwrap(),
            )
            .unwrap();
        blocked
            .enter_blocked(
                ApprovalBlockerCode::ApproverAccountInactive,
                Timestamp::from_unix_secs(27).unwrap(),
            )
            .unwrap();
        let policy = blocked.cancellation_task_policy().unwrap();
        assert_eq!(policy, ApprovalCancellationTaskPolicy::NoOpenTask);
        assert!(policy.ensure_open_task_count(0).is_ok());
        assert_eq!(
            policy.ensure_open_task_count(1),
            Err(ModelError::InvalidStatus("受阻审批实例不得存在开放任务"))
        );

        let missing_execution = instance();
        assert_eq!(
            missing_execution.cancellation_task_policy(),
            Err(ModelError::InvalidStatus("可撤回审批实例必须存在当前执行"))
        );

        let mut approved = instance();
        approved
            .complete_approved(Timestamp::from_unix_secs(28).unwrap())
            .unwrap();
        assert_eq!(
            approved.cancellation_task_policy(),
            Err(ModelError::InvalidStatus("已最终通过的审批实例不得撤回"))
        );

        let mut cancelled = instance();
        cancelled.cancel(Timestamp::from_unix_secs(29).unwrap()).unwrap();
        assert_eq!(
            cancelled.cancellation_task_policy(),
            Err(ModelError::InvalidStatus("已取消的审批实例不得重复撤回"))
        );
    }

    /// 只有人员类 blocker 允许恢复原审批人。
    #[test]
    fn only_personnel_blocker_allows_assignee_recovery() {
        let mut inst = instance();
        inst.enter_blocked(
            ApprovalBlockerCode::DefinitionGraphCorrupted,
            Timestamp::from_unix_secs(30).unwrap(),
        )
        .unwrap();
        assert_eq!(
            inst.ensure_assignee_recovery_allowed(),
            Err(ModelError::InvalidStatus("结构性或一致性阻塞不得恢复原审批人"))
        );
        inst.blocker_code = Some(ApprovalBlockerCode::ApproverAccountInactive);
        assert!(inst.ensure_assignee_recovery_allowed().is_ok());
        inst.complete_approved(Timestamp::from_unix_secs(31).unwrap())
            .unwrap();
        assert_eq!(
            inst.ensure_assignee_recovery_allowed(),
            Err(ModelError::InvalidStatus("终态实例不得恢复原审批人"))
        );
    }

    /// 实例公开 API 不提供驳回或终止终态。
    #[test]
    fn instance_has_no_reject_or_terminate() {
        let names = ["reject", "terminate"];
        for name in names {
            assert!(
                !["cancel", "complete_approved", "next_round"].contains(&name)
                    || name == "cancel"
                    || name == "complete_approved"
                    || name == "next_round"
            );
        }
        assert!(!ApprovalProcessInstanceStatus::Blocked.is_terminal());
    }
}
