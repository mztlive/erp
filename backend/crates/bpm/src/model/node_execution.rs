//! 节点执行。构造器只创建活动或受阻执行，结束后不得重开。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use crate::model::types::{
    base_model_at, normalize_optional, normalize_required, touch_base, ApprovalBlockerCode, ApprovalDecision,
    ApprovalExecutionAssignmentSource, ApprovalExecutionEndReason, ApprovalNodeExecutionStatus, ModelError,
    ModelResult, LABEL_MAX_LEN, NAME_MAX_LEN, NODE_KEY_MAX_LEN, REASON_MAX_LEN,
};
use crate::model::{ParticipantId, Timestamp};

/// 节点每次进入形成的执行记录。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ApprovalNodeExecution {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属实例。
    pub process_instance_id: ApprovalProcessInstanceId,
    /// 冻结节点键。
    pub node_key: String,
    /// 冻结节点名称。
    pub node_name: String,
    /// 所属轮次。
    pub round_no: u32,
    /// 实例内单调递增的执行序号。
    pub execution_no: u32,
    /// 执行状态。
    pub status: ApprovalNodeExecutionStatus,
    /// 本次进入的分派来源。
    pub assignment_source: ApprovalExecutionAssignmentSource,
    /// 被替换的旧执行。
    pub replaces_execution_id: Option<ApprovalNodeExecutionId>,
    /// 被替换时的固定结束原因。
    pub ended_reason: Option<ApprovalExecutionEndReason>,
    /// 本次冻结的审批人。
    pub assignee_participant_id: ParticipantId,
    /// 审批人显示名快照。
    pub assignee_name_snapshot: String,
    /// 正式决定。
    pub decision: Option<ApprovalDecision>,
    /// 决定原因。
    pub decision_reason: Option<String>,
    /// 决定人。
    pub decided_by: Option<ParticipantId>,
    /// 决定时间。
    pub decided_at: Option<Timestamp>,
    /// 进入时间。
    pub activated_at: Timestamp,
    /// 仅受阻时必填。
    pub blocker_code: Option<ApprovalBlockerCode>,
    /// 仅受阻时必填。
    pub blocked_at: Option<Timestamp>,
    /// 结束时间。
    pub ended_at: Option<Timestamp>,
}

/// 创建当前执行所需的身份与责任快照。
pub struct NewNodeExecution {
    /// 执行主键。
    pub id: ApprovalNodeExecutionId,
    /// 所属实例。
    pub process_instance_id: ApprovalProcessInstanceId,
    /// 节点键。
    pub node_key: String,
    /// 节点名称。
    pub node_name: String,
    /// 轮次。
    pub round_no: u32,
    /// 执行序号。
    pub execution_no: u32,
    /// 分派来源。
    pub assignment_source: ApprovalExecutionAssignmentSource,
    /// 替换的旧执行。
    pub replaces_execution_id: Option<ApprovalNodeExecutionId>,
    /// 审批人。
    pub assignee_participant_id: ParticipantId,
    /// 审批人显示名。
    pub assignee_name_snapshot: String,
    /// 进入时间。
    pub at: Timestamp,
}

impl ApprovalNodeExecution {
    /// 创建活动执行。
    ///
    /// # 错误
    /// 轮次、序号或快照非法时返回错误。
    pub fn new_active(input: NewNodeExecution) -> ModelResult<Self> {
        Self::new_current(input, ApprovalNodeExecutionStatus::Active, None)
    }

    /// 创建受阻执行。
    ///
    /// # 错误
    /// 轮次、序号或快照非法时返回错误。
    pub fn new_blocked(input: NewNodeExecution, blocker_code: ApprovalBlockerCode) -> ModelResult<Self> {
        Self::new_current(input, ApprovalNodeExecutionStatus::Blocked, Some(blocker_code))
    }

    /// 将活动执行标记为通过。
    ///
    /// # 错误
    /// 执行已结束时返回 [`ModelError::ExecutionAlreadyEnded`]。
    pub fn record_approve(
        &mut self,
        actor: ParticipantId,
        reason: Option<String>,
        at: Timestamp,
    ) -> ModelResult<()> {
        self.record_decision(ApprovalDecision::Approve, actor, reason, at)
    }

    /// 将活动执行标记为驳回。
    ///
    /// # 错误
    /// 执行为空原因或已结束时返回错误。
    pub fn record_reject(
        &mut self,
        actor: ParticipantId,
        reason: impl Into<String>,
        at: Timestamp,
    ) -> ModelResult<()> {
        let reason = normalize_required(reason, "驳回原因不能为空", REASON_MAX_LEN, "驳回原因过长")?;
        self.record_decision(ApprovalDecision::Reject, actor, Some(reason), at)
    }

    /// 将当前执行取消。
    ///
    /// # 错误
    /// 执行已结束时返回错误。
    pub fn cancel(&mut self, at: Timestamp) -> ModelResult<()> {
        self.ensure_current()?;
        self.status = ApprovalNodeExecutionStatus::Cancelled;
        self.ended_at = Some(at);
        touch_base(&mut self.base, at)
    }

    /// 将活动执行置为受阻。
    ///
    /// # 错误
    /// 执行已结束时返回错误。
    pub fn block(&mut self, code: ApprovalBlockerCode, at: Timestamp) -> ModelResult<()> {
        self.ensure_current()?;
        if self.status != ApprovalNodeExecutionStatus::Active {
            return Err(ModelError::InvalidStatus("只有活动执行可以进入受阻"));
        }
        self.status = ApprovalNodeExecutionStatus::Blocked;
        self.blocker_code = Some(code);
        self.blocked_at = Some(at);
        touch_base(&mut self.base, at)
    }

    /// 仅把受阻执行转为已替换，并写入固定结束原因。
    ///
    /// # 参数
    /// * `reason` - `ADMIN_REASSIGNED` 或 `ASSIGNEE_RECOVERED`
    /// * `at` - 结束时间
    ///
    /// # 错误
    /// 当前不是受阻时返回错误。
    pub fn supersede(&mut self, reason: ApprovalExecutionEndReason, at: Timestamp) -> ModelResult<()> {
        if self.status != ApprovalNodeExecutionStatus::Blocked {
            return Err(ModelError::InvalidStatus("只有受阻执行可以替换"));
        }
        self.status = ApprovalNodeExecutionStatus::Superseded;
        self.ended_reason = Some(reason);
        self.ended_at = Some(at);
        touch_base(&mut self.base, at)
    }

    /// 创建同轮次、同节点、更大序号的替换执行。
    ///
    /// 旧执行审批人快照保持不变。
    ///
    /// # 错误
    /// 新序号未递增或快照非法时返回错误。
    pub fn spawn_replacement(
        &self,
        id: ApprovalNodeExecutionId,
        execution_no: u32,
        source: ApprovalExecutionAssignmentSource,
        assignee: ParticipantId,
        assignee_name: impl Into<String>,
        at: Timestamp,
    ) -> ModelResult<Self> {
        if execution_no <= self.execution_no {
            return Err(ModelError::InvalidField("替换执行序号必须递增"));
        }
        if self.node_key.is_empty() {
            return Err(ModelError::InvalidField("节点键不能为空"));
        }
        Self::new_active(NewNodeExecution {
            id,
            process_instance_id: self.process_instance_id.clone(),
            node_key: self.node_key.clone(),
            node_name: self.node_name.clone(),
            round_no: self.round_no,
            execution_no,
            assignment_source: source,
            replaces_execution_id: Some(ApprovalNodeExecutionId::new(self.base.id.clone())),
            assignee_participant_id: assignee,
            assignee_name_snapshot: assignee_name.into(),
            at,
        })
    }

    fn new_current(
        input: NewNodeExecution,
        status: ApprovalNodeExecutionStatus,
        blocker_code: Option<ApprovalBlockerCode>,
    ) -> ModelResult<Self> {
        if input.round_no == 0 || input.execution_no == 0 {
            return Err(ModelError::InvalidField("轮次和执行序号必须从 1 开始"));
        }
        if !status.is_current() {
            return Err(ModelError::InvalidStatus("构造器不得创建已结束执行"));
        }
        let blocked_at = blocker_code.map(|_| input.at);
        Ok(Self {
            base: base_model_at(input.id.to_string(), input.at)?,
            process_instance_id: input.process_instance_id,
            node_key: normalize_required(input.node_key, "节点键不能为空", NODE_KEY_MAX_LEN, "节点键过长")?,
            node_name: normalize_required(input.node_name, "节点名称不能为空", NAME_MAX_LEN, "节点名称过长")?,
            round_no: input.round_no,
            execution_no: input.execution_no,
            status,
            assignment_source: input.assignment_source,
            replaces_execution_id: input.replaces_execution_id,
            ended_reason: None,
            assignee_participant_id: input.assignee_participant_id,
            assignee_name_snapshot: normalize_required(
                input.assignee_name_snapshot,
                "审批人显示名不能为空",
                LABEL_MAX_LEN,
                "审批人显示名过长",
            )?,
            decision: None,
            decision_reason: None,
            decided_by: None,
            decided_at: None,
            activated_at: input.at,
            blocker_code,
            blocked_at,
            ended_at: None,
        })
    }

    fn record_decision(
        &mut self,
        decision: ApprovalDecision,
        actor: ParticipantId,
        reason: Option<String>,
        at: Timestamp,
    ) -> ModelResult<()> {
        self.ensure_current()?;
        if self.status != ApprovalNodeExecutionStatus::Active {
            return Err(ModelError::InvalidStatus("只有活动执行可以记录决定"));
        }
        self.decision_reason = normalize_optional(reason, REASON_MAX_LEN, "决定原因过长")?;
        self.status = match decision {
            ApprovalDecision::Approve => ApprovalNodeExecutionStatus::Approved,
            ApprovalDecision::Reject => ApprovalNodeExecutionStatus::Rejected,
        };
        self.decision = Some(decision);
        self.decided_by = Some(actor);
        self.decided_at = Some(at);
        self.ended_at = Some(at);
        touch_base(&mut self.base, at)
    }

    fn ensure_current(&self) -> ModelResult<()> {
        if self.status.is_ended() {
            return Err(ModelError::ExecutionAlreadyEnded);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ApprovalNodeExecution, NewNodeExecution};
    use crate::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
    use crate::model::types::{
        ApprovalBlockerCode, ApprovalDecision, ApprovalExecutionAssignmentSource, ApprovalExecutionEndReason,
        ApprovalNodeExecutionStatus, ModelError,
    };
    use crate::model::{ParticipantId, Timestamp};

    fn active() -> ApprovalNodeExecution {
        ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new("e1"),
            process_instance_id: ApprovalProcessInstanceId::new("inst"),
            node_key: "n1".into(),
            node_name: "仓储复核".into(),
            round_no: 1,
            execution_no: 1,
            assignment_source: ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: ParticipantId::new("u1").unwrap(),
            assignee_name_snapshot: "张三".into(),
            at: Timestamp::from_unix_secs(1).unwrap(),
        })
        .unwrap()
    }

    /// 构造器不得创建等待态；活动执行可记录决定。
    #[test]
    fn constructor_only_creates_current_execution() {
        let exec = active();
        assert_eq!(exec.status, ApprovalNodeExecutionStatus::Active);
        assert!(exec.decision.is_none());
        let blocked = ApprovalNodeExecution::new_blocked(
            NewNodeExecution {
                id: ApprovalNodeExecutionId::new("e2"),
                process_instance_id: ApprovalProcessInstanceId::new("inst"),
                node_key: "n1".into(),
                node_name: "仓储复核".into(),
                round_no: 1,
                execution_no: 2,
                assignment_source: ApprovalExecutionAssignmentSource::Definition,
                replaces_execution_id: None,
                assignee_participant_id: ParticipantId::new("u1").unwrap(),
                assignee_name_snapshot: "张三".into(),
                at: Timestamp::from_unix_secs(2).unwrap(),
            },
            ApprovalBlockerCode::ApproverAccountInactive,
        )
        .unwrap();
        assert_eq!(blocked.status, ApprovalNodeExecutionStatus::Blocked);
    }

    /// 已结束执行不能重开或覆盖决定。
    #[test]
    fn ended_execution_cannot_reopen() {
        let mut exec = active();
        exec.record_approve(
            ParticipantId::new("u1").unwrap(),
            None,
            Timestamp::from_unix_secs(3).unwrap(),
        )
        .unwrap();
        assert_eq!(exec.status, ApprovalNodeExecutionStatus::Approved);
        assert_eq!(exec.decision, Some(ApprovalDecision::Approve));
        assert_eq!(
            exec.record_reject(
                ParticipantId::new("u1").unwrap(),
                "不行",
                Timestamp::from_unix_secs(4).unwrap()
            ),
            Err(ModelError::ExecutionAlreadyEnded)
        );
        assert_eq!(
            exec.supersede(
                ApprovalExecutionEndReason::AdminReassigned,
                Timestamp::from_unix_secs(5).unwrap()
            ),
            Err(ModelError::InvalidStatus("只有受阻执行可以替换"))
        );
    }

    /// 受阻到已替换必须写入固定结束原因，并生成同轮递增执行。
    #[test]
    fn supersede_only_from_blocked_and_keeps_snapshot() {
        let mut exec = active();
        exec.block(
            ApprovalBlockerCode::ApproverEmploymentInvalid,
            Timestamp::from_unix_secs(6).unwrap(),
        )
        .unwrap();
        let old_assignee = exec.assignee_participant_id.clone();
        exec.supersede(
            ApprovalExecutionEndReason::AdminReassigned,
            Timestamp::from_unix_secs(7).unwrap(),
        )
        .unwrap();
        assert_eq!(exec.status, ApprovalNodeExecutionStatus::Superseded);
        assert_eq!(
            exec.ended_reason,
            Some(ApprovalExecutionEndReason::AdminReassigned)
        );
        assert_eq!(exec.assignee_participant_id, old_assignee);

        let replacement = exec
            .spawn_replacement(
                ApprovalNodeExecutionId::new("e3"),
                2,
                ApprovalExecutionAssignmentSource::AdminReassign,
                ParticipantId::new("u2").unwrap(),
                "李四",
                Timestamp::from_unix_secs(8).unwrap(),
            )
            .unwrap();
        assert_eq!(replacement.round_no, exec.round_no);
        assert_eq!(replacement.node_key, exec.node_key);
        assert_eq!(replacement.execution_no, 2);
        assert_eq!(replacement.assignee_name_snapshot, "李四");
        assert_eq!(
            replacement.assignment_source,
            ApprovalExecutionAssignmentSource::AdminReassign
        );
        assert!(exec
            .spawn_replacement(
                ApprovalNodeExecutionId::new("e4"),
                1,
                ApprovalExecutionAssignmentSource::AssigneeRecovery,
                ParticipantId::new("u1").unwrap(),
                "张三",
                Timestamp::from_unix_secs(9).unwrap(),
            )
            .is_err());
    }
}
