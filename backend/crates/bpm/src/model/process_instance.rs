//! 审批运行实例。只保存业务对象引用与提交版本，不保存业务快照。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
use crate::model::types::{
    base_model_at, persistence_stamp, touch_base, ApprovalBlockerCode, ApprovalProcessInstanceStatus,
    ModelError, ModelResult,
};
use crate::model::{ParticipantId, ProcessKind, SubjectRef, Timestamp};

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
    /// * `id` - 实例主键
    /// * `process_definition_id` - 绑定定义
    /// * `definition_version` - 绑定定义版本
    /// * `process_kind` - 流程种类
    /// * `subject` - 业务对象引用
    /// * `subject_version` - 冻结提交版本
    /// * `started_by` - 启动人
    /// * `at` - 启动时间
    ///
    /// # 错误
    /// 定义版本为零时返回错误。
    #[allow(clippy::too_many_arguments)]
    pub fn start_running(
        id: ApprovalProcessInstanceId,
        process_definition_id: ApprovalProcessDefinitionId,
        definition_version: u32,
        process_kind: ProcessKind,
        subject: SubjectRef,
        subject_version: u32,
        started_by: ParticipantId,
        at: Timestamp,
    ) -> ModelResult<Self> {
        if definition_version == 0 {
            return Err(ModelError::InvalidField("定义版本必须从 1 开始"));
        }
        Ok(Self {
            base: base_model_at(id.to_string(), at)?,
            process_definition_id,
            definition_version,
            process_kind,
            subject,
            subject_version,
            status: ApprovalProcessInstanceStatus::Running,
            current_round_no: 1,
            current_node_execution_id: None,
            blocker_code: None,
            blocked_at: None,
            started_by,
            started_at: at,
            ended_at: None,
        })
    }

    /// 返回实例乐观锁版本。
    ///
    /// # 返回
    /// 返回 `base.version`。
    pub fn instance_version(&self) -> u64 {
        self.base.version
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

    /// 校验当前 blocker 是否允许人员改派。
    ///
    /// # 错误
    /// 终态、非受阻或结构性阻塞时返回错误。
    pub fn ensure_personnel_reassign_allowed(&self) -> ModelResult<()> {
        self.ensure_not_terminal()
            .map_err(|_| ModelError::TerminalInstanceCannotReassign)?;
        let Some(code) = self.blocker_code else {
            return Err(ModelError::InvalidStatus("只有受阻实例可以改派"));
        };
        if code.allows_personnel_reassign() {
            return Ok(());
        }
        Err(ModelError::StructuralBlockerCannotReassign)
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

    /// 返回启动时间对应的持久化秒，供测试断言。
    ///
    /// # 错误
    /// 时间为负时返回错误。
    pub fn started_at_stamp(&self) -> ModelResult<u64> {
        persistence_stamp(self.started_at)
    }
}

#[cfg(test)]
mod tests {
    use super::ApprovalProcessInstance;
    use crate::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use crate::model::types::{ApprovalBlockerCode, ApprovalProcessInstanceStatus, ModelError};
    use crate::model::{ParticipantId, ProcessKind, SubjectRef, Timestamp};

    fn instance() -> ApprovalProcessInstance {
        ApprovalProcessInstance::start_running(
            ApprovalProcessInstanceId::new("inst"),
            ApprovalProcessDefinitionId::new("def"),
            1,
            ProcessKind::StockAdjustment,
            SubjectRef::new("stock_adjustment", "adj-1").unwrap(),
            1,
            ParticipantId::new("user").unwrap(),
            Timestamp::from_unix_secs(10).unwrap(),
        )
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

    /// 结构性阻塞不得走领域改派。
    #[test]
    fn structural_blocker_cannot_reassign() {
        let mut inst = instance();
        inst.enter_blocked(
            ApprovalBlockerCode::DefinitionGraphCorrupted,
            Timestamp::from_unix_secs(30).unwrap(),
        )
        .unwrap();
        assert_eq!(
            inst.ensure_personnel_reassign_allowed(),
            Err(ModelError::StructuralBlockerCannotReassign)
        );
        inst.blocker_code = Some(ApprovalBlockerCode::ApproverAccountInactive);
        assert!(inst.ensure_personnel_reassign_allowed().is_ok());
        inst.complete_approved(Timestamp::from_unix_secs(31).unwrap())
            .unwrap();
        assert_eq!(
            inst.ensure_personnel_reassign_allowed(),
            Err(ModelError::TerminalInstanceCannotReassign)
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
