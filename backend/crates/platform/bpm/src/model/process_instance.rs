//! 审批运行实例。只保存业务对象引用与提交版本，不保存业务快照。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
use crate::model::types::{
    ApprovalBlockerCode, ApprovalProcessInstanceStatus, ModelError, ModelResult, base_model_at, touch_base,
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
    /// 构造撤回命令时据此决定是否发出关闭任务意图；[`Self::CloseOpenTask`] 需要关闭，[`Self::NoOpenTask`] 不需要。
    pub fn closes_open_task(self) -> bool {
        self == Self::CloseOpenTask
    }

    /// 撤回时校验传入数量符合策略：运行中要求一项，受阻要求零项；或签实际数量先由节点执行按冻结人数校验并折算。
    /// 不符时返回状态错误，防止按错误任务事实撤回。
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
    /// 启动审批先创建第 1 轮运行实例，冻结定义版本与业务对象提交版本；当前执行由引擎进入入口时再挂上。
    /// 后续状态推进不得改写业务对象引用及提交版本。定义版本为零或启动时间不可持久化时返回错误。
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

    /// 构造执行、事件或任务关联时取得与 `base.id` 同值的 [`ApprovalProcessInstanceId`]，无需调用方自行转换。
    pub fn typed_id(&self) -> ApprovalProcessInstanceId {
        ApprovalProcessInstanceId::new(self.base.id.clone())
    }

    /// 提交迁移时读取实例的乐观锁版本，用于 CAS 区分并发变更，而不是比较已冻结的单据提交版本。
    pub fn instance_version(&self) -> u64 {
        self.base.version
    }

    /// 撤回前用实例状态和当前令牌引用选择任务策略：运行中需要关闭任务，受阻不得仍有开放任务。
    /// 终态或缺当前执行引用时拒绝；实际任务数量由执行与任务事实校验。
    pub fn cancellation_task_policy(&self) -> ModelResult<ApprovalCancellationTaskPolicy> {
        match (self.status, self.current_node_execution_id.is_some()) {
            (ApprovalProcessInstanceStatus::Approved, _) => {
                Err(ModelError::InvalidStatus("已最终通过的审批实例不得撤回"))
            },
            (ApprovalProcessInstanceStatus::Rejected, _) => {
                Err(ModelError::InvalidStatus("已最终驳回的审批实例不得撤回"))
            },
            (ApprovalProcessInstanceStatus::Cancelled, _) => {
                Err(ModelError::InvalidStatus("已取消的审批实例不得重复撤回"))
            },
            (ApprovalProcessInstanceStatus::Running | ApprovalProcessInstanceStatus::Blocked, false) => {
                Err(ModelError::InvalidStatus("可撤回审批实例必须存在当前执行"))
            },
            (ApprovalProcessInstanceStatus::Running, true) => {
                Ok(ApprovalCancellationTaskPolicy::CloseOpenTask)
            },
            (ApprovalProcessInstanceStatus::Blocked, true) => Ok(ApprovalCancellationTaskPolicy::NoOpenTask),
        }
    }

    /// 启动、节点交接或恢复进入新执行时更新当前令牌引用并推进锁版本；不允许为终态实例再挂执行。
    pub fn set_current_execution(
        &mut self,
        execution_id: ApprovalNodeExecutionId,
        at: Timestamp,
    ) -> ModelResult<()> {
        self.ensure_not_terminal()?;
        self.current_node_execution_id = Some(execution_id);
        touch_base(&mut self.base, at)
    }

    /// 显式进入下一轮时，只允许运行中实例递增轮次并清除旧执行引用；当前驳回路径不调用它。
    /// 轮次或版本溢出、时间不可持久化时返回错误。
    pub fn next_round(&mut self, at: Timestamp) -> ModelResult<u32> {
        if self.status != ApprovalProcessInstanceStatus::Running {
            return Err(ModelError::InvalidStatus("只有运行中的实例可以进入下一轮"));
        }

        self.current_round_no = self.current_round_no.checked_add(1).ok_or(ModelError::Overflow("轮次"))?;
        self.current_node_execution_id = None;
        touch_base(&mut self.base, at)?;
        Ok(self.current_round_no)
    }

    /// 引擎记录当前无法推进时，把阻塞码和时间投影到未终结的实例，供调用方识别受阻状态。
    /// 执行历史和任务变更由迁移计划携带；终态、时间非法或版本溢出时不允许提交。
    pub fn enter_blocked(&mut self, code: ApprovalBlockerCode, at: Timestamp) -> ModelResult<()> {
        self.ensure_not_terminal()?;
        self.status = ApprovalProcessInstanceStatus::Blocked;
        self.blocker_code = Some(code);
        self.blocked_at = Some(at);
        touch_base(&mut self.base, at)
    }

    /// 原人恢复重新进入活动执行时，将受阻实例恢复为运行并清除实例上的阻塞码和时间；旧执行中的历史不动。
    /// 非受阻、时间非法或版本溢出时返回错误。
    pub fn exit_blocked(&mut self, at: Timestamp) -> ModelResult<()> {
        if self.status != ApprovalProcessInstanceStatus::Blocked {
            return Err(ModelError::InvalidStatus("只有受阻实例可以恢复运行"));
        }

        self.status = ApprovalProcessInstanceStatus::Running;
        self.clear_blocker_projection();
        touch_base(&mut self.base, at)
    }

    /// 末节点通过时将实例收结为已批准，记录结束时间并清掉当前令牌与阻塞投影。
    /// 实例已终结、时间非法或版本溢出时返回错误。
    pub fn complete_approved(&mut self, at: Timestamp) -> ModelResult<()> {
        self.ensure_not_terminal()?;
        self.status = ApprovalProcessInstanceStatus::Approved;
        self.current_node_execution_id = None;
        self.clear_blocker_projection();
        self.ended_at = Some(at);
        touch_base(&mut self.base, at)
    }

    /// 接受运行中节点的驳回决定后终结本实例，清除当前令牌和阻塞投影并记录决定时间；不会创建新轮。
    /// 非运行中、时间非法或版本溢出时返回错误。
    pub fn complete_rejected(&mut self, at: Timestamp) -> ModelResult<()> {
        if self.status != ApprovalProcessInstanceStatus::Running {
            return Err(ModelError::InvalidStatus("只有运行中的实例可以最终驳回"));
        }

        self.status = ApprovalProcessInstanceStatus::Rejected;
        self.current_node_execution_id = None;
        self.clear_blocker_projection();
        self.ended_at = Some(at);
        touch_base(&mut self.base, at)
    }

    /// 撤回计划结束当前执行时一并终结实例，清除令牌和阻塞投影并记录结束时间，避免继续办理。
    /// 终态、时间非法或版本溢出时返回错误；权限、任务与取消端口由上层计划校验。
    pub fn cancel(&mut self, at: Timestamp) -> ModelResult<()> {
        self.ensure_not_terminal()?;
        self.status = ApprovalProcessInstanceStatus::Cancelled;
        self.current_node_execution_id = None;
        self.clear_blocker_projection();
        self.ended_at = Some(at);
        touch_base(&mut self.base, at)
    }

    /// 原人恢复前要求实例确实因人员失效受阻；不把图损坏或一致性异常当成可重派任务。
    /// 终态、非受阻、缺阻塞码或不支持恢复时返回 [`ModelError::InvalidStatus`]。
    pub fn ensure_assignee_recovery_allowed(&self) -> ModelResult<()> {
        self.ensure_not_terminal().map_err(|_| ModelError::InvalidStatus("终态实例不得恢复原审批人"))?;
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

    // 写实例运行字段前拦截已批准、已驳回和已取消状态，防止终态重新获得令牌或阻塞信息。
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

    // 验证启动模型先创建未挂执行、未受阻的第 1 轮实例，入口执行随后由引擎写入。
    #[test]
    fn start_running_round_one() {
        let inst = instance();
        assert_eq!(inst.status, ApprovalProcessInstanceStatus::Running);
        assert_eq!(inst.current_round_no, 1);
        assert!(inst.current_node_execution_id.is_none());
        assert!(inst.blocker_code.is_none());
    }

    // 验证运行中可以递增轮次并清掉旧令牌，受阻后不能借换轮跳过恢复规则。
    #[test]
    fn next_round_requires_running() {
        let mut inst = instance();
        inst.set_current_execution(
            ApprovalNodeExecutionId::new("e1"),
            Timestamp::from_unix_secs(11).unwrap(),
        )
        .unwrap();
        assert_eq!(inst.next_round(Timestamp::from_unix_secs(12).unwrap()).unwrap(), 2);
        assert!(inst.current_node_execution_id.is_none());

        inst.enter_blocked(
            ApprovalBlockerCode::ApproverAccountInactive,
            Timestamp::from_unix_secs(13).unwrap(),
        )
        .unwrap();
        assert!(inst.next_round(Timestamp::from_unix_secs(14).unwrap()).is_err());
    }

    // 验证恢复清除实例阻塞投影，最终通过清除令牌，且终结后不允许再取消。
    #[test]
    fn blocker_projection_clears_on_exit_or_terminal() {
        let mut inst = instance();
        let at = Timestamp::from_unix_secs(20).unwrap();
        inst.enter_blocked(ApprovalBlockerCode::ApproverNotEligible, at).unwrap();
        assert_eq!(inst.status, ApprovalProcessInstanceStatus::Blocked);
        assert_eq!(inst.blocker_code, Some(ApprovalBlockerCode::ApproverNotEligible));

        inst.exit_blocked(Timestamp::from_unix_secs(21).unwrap()).unwrap();
        assert_eq!(inst.status, ApprovalProcessInstanceStatus::Running);
        assert!(inst.blocker_code.is_none());

        inst.complete_approved(Timestamp::from_unix_secs(22).unwrap()).unwrap();
        assert_eq!(inst.status, ApprovalProcessInstanceStatus::Approved);
        assert!(inst.current_node_execution_id.is_none());
        assert!(inst.cancel(Timestamp::from_unix_secs(23).unwrap()).is_err());
    }

    // 用带当前令牌的运行实例检查基础取消策略，确认零项或两项开放任务都不能按单任务处理。
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

    // 验证受阻执行必须无开放任务才能撤回，且终态实例或没有当前令牌的运行实例拿不到取消策略。
    #[test]
    fn blocked_and_terminal_cancellation_facts_fail_closed() {
        let mut blocked = instance();
        blocked
            .set_current_execution(ApprovalNodeExecutionId::new("e1"), Timestamp::from_unix_secs(26).unwrap())
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
        approved.complete_approved(Timestamp::from_unix_secs(28).unwrap()).unwrap();
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

    // 验证人员失效受阻可走原人恢复，结构损坏和终态则拒绝。
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

        inst.complete_approved(Timestamp::from_unix_secs(31).unwrap()).unwrap();
        assert_eq!(
            inst.ensure_assignee_recovery_allowed(),
            Err(ModelError::InvalidStatus("终态实例不得恢复原审批人"))
        );
    }

    // 验证受阻不等于流程收结，使恢复和取消仍可按各自的前置条件处理。
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
