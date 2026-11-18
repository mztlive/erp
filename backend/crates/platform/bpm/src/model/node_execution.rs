//! 节点执行。构造器只创建活动或受阻执行，结束后不得重开。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use super::ApprovalCancellationTaskPolicy;
use super::approval_participant::validate_additional;
use crate::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use crate::model::types::{
    ApprovalBlockerCode, ApprovalDecision, ApprovalExecutionAssignmentSource, ApprovalExecutionEndReason,
    ApprovalNodeExecutionStatus, LABEL_MAX_LEN, ModelError, ModelResult, NAME_MAX_LEN, NODE_KEY_MAX_LEN,
    REASON_MAX_LEN, base_model_at, normalize_optional, normalize_required, touch_base,
};
use crate::model::{ApprovalParticipant, ParticipantId, Timestamp};

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
    /// 同节点的其他或签审批人；缺失时兼容单人节点。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_assignees: Vec<ApprovalParticipant>,
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
    /// 展示节点执行时优先显示已记录决定人的冻结名字；未决定或无对应快照时，显示冻结人员名单并标明或签。
    pub fn assignee_display_name(&self) -> String {
        let people = self.participants();
        if let Some(actor) = &self.decided_by
            && let Some(person) = people.iter().find(|person| &person.participant_id == actor)
        {
            return person.name.clone();
        }

        let names = people.into_iter().map(|person| person.name).collect::<Vec<_>>().join("、");
        if self.additional_assignees.is_empty() { names } else { format!("{names}（或签）") }
    }

    /// 撤回前用冻结人数解释执行关联的开放任务数：运行中单人须恰有一项，或签须至少一项且不超过人数；受阻必须为零。
    /// 数量偏离实例的取消策略时返回模型错误，不能继续关闭任务。
    pub fn ensure_cancellation_tasks(
        &self,
        policy: ApprovalCancellationTaskPolicy,
        count: usize,
    ) -> ModelResult<()> {
        let normalized = if count > 0 && count <= 1 + self.additional_assignees.len() { 1 } else { count };
        policy.ensure_open_task_count(normalized)
    }

    /// 进入节点时把其他或签成员写入执行，供任务分派与决定人展示沿用同一快照。人数、重复身份和名称受节点人数规则约束。
    pub fn with_additional_assignees(mut self, assignees: Vec<ApprovalParticipant>) -> ModelResult<Self> {
        validate_additional(&self.assignee_participant_id, &assignees)?;
        self.additional_assignees = assignees;
        Ok(self)
    }

    /// 从执行快照按首位、其他或签成员的顺序取回名单，供显示和决定人匹配，不重新查询定义或账号。
    pub fn participants(&self) -> Vec<ApprovalParticipant> {
        let mut result = vec![ApprovalParticipant {
            participant_id: self.assignee_participant_id.clone(),
            name: self.assignee_name_snapshot.clone(),
        }];
        result.extend(self.additional_assignees.clone());
        result
    }

    /// 接受决定前用执行快照判断身份是否属于首位或其他或签成员，不用当前账号配置替换冻结名单。
    pub fn has_participant(&self, id: &str) -> bool {
        self.assignee_participant_id.as_str() == id
            || self.additional_assignees.iter().any(|item| item.participant_id.as_str() == id)
    }

    /// 构造执行关联事件、任务或实例令牌时，取得与 `base.id` 同值的 [`ApprovalNodeExecutionId`]，避免调用方自行包装。
    pub fn typed_id(&self) -> ApprovalNodeExecutionId {
        ApprovalNodeExecutionId::new(self.base.id.clone())
    }

    /// 节点有合格办理人时创建活动执行，供引擎挂到实例并生成任务；原人恢复必须带旧执行引用，定义进入不能带。
    /// 轮次或序号为零、节点或快照文本非法、分派来源不合规则或时间不可持久化时返回错误。
    pub fn new_active(input: NewNodeExecution) -> ModelResult<Self> {
        Self::new_current(input, None)
    }

    /// 进入节点时无人能办理，用结构化阻塞码直接创建受阻执行，不先开任务再关闭。
    /// 原人恢复不能直接构造受阻执行；轮次、序号、来源、文本或时间不合法时返回错误。
    pub fn new_blocked(input: NewNodeExecution, blocker_code: ApprovalBlockerCode) -> ModelResult<Self> {
        Self::new_current(input, Some(blocker_code))
    }

    /// 引擎接受通过决定后结束活动执行，记录决定人、可选意见和决定时间，供后继节点或最终通过使用。
    ///
    /// # 错误
    /// 已结束返回 [`ModelError::ExecutionAlreadyEnded`]；非活动、意见超长、时间不可持久化或版本溢出也返回错误。
    pub fn record_approve(
        &mut self,
        actor: ParticipantId,
        reason: Option<String>,
        at: Timestamp,
    ) -> ModelResult<()> {
        self.record_decision(ApprovalDecision::Approve, actor, reason, at)
    }

    /// 引擎接受驳回决定后，结束活动执行并记录决定人与事由，保留本次实例被驳回的依据。
    ///
    /// # 错误
    /// 原因裁剪后为空或超长、非活动、时间不可持久化或版本溢出时返回错误；已结束执行返回 [`ModelError::ExecutionAlreadyEnded`]。
    pub fn record_reject(
        &mut self,
        actor: ParticipantId,
        reason: impl Into<String>,
        at: Timestamp,
    ) -> ModelResult<()> {
        let reason = normalize_required(reason, "驳回原因不能为空", REASON_MAX_LEN, "驳回原因过长")?;
        self.record_decision(ApprovalDecision::Reject, actor, Some(reason), at)
    }

    /// 撤回实例时一并结束其活动或受阻执行，防止撤回后还有可办理令牌。
    /// 执行已结束返回 [`ModelError::ExecutionAlreadyEnded`]；时间不可持久化或版本溢出时也返回错误。
    pub fn cancel(&mut self, at: Timestamp) -> ModelResult<()> {
        self.ensure_current()?;
        self.status = ApprovalNodeExecutionStatus::Cancelled;
        self.ended_at = Some(at);
        touch_base(&mut self.base, at)
    }

    /// 当前活动执行不能继续时写入结构化阻塞码和时间，使它保持为当前令牌但不再接受决定。
    /// 已结束返回 [`ModelError::ExecutionAlreadyEnded`]，已受阻返回 [`ModelError::InvalidStatus`]；时间或版本非法也返回模型错误。
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

    // 创建原人恢复的新执行前，把人员失效导致的旧受阻执行结束为 `SUPERSEDED`，固定原因为 `ASSIGNEE_RECOVERED`。
    // 非受阻、缺阻塞码、结构性阻塞或时间及版本非法时拒绝替换。
    pub(crate) fn supersede_for_assignee_recovery(&mut self, at: Timestamp) -> ModelResult<()> {
        if self.status != ApprovalNodeExecutionStatus::Blocked {
            return Err(ModelError::InvalidStatus("只有受阻执行可以替换"));
        }
        let Some(code) = self.blocker_code else {
            return Err(ModelError::InvalidStatus("受阻执行缺少 blocker"));
        };
        if !code.allows_assignee_recovery() {
            return Err(ModelError::InvalidStatus("结构性或一致性阻塞不得恢复原审批人"));
        }

        self.status = ApprovalNodeExecutionStatus::Superseded;
        self.ended_reason = Some(ApprovalExecutionEndReason::AssigneeRecovered);
        self.ended_at = Some(at);
        touch_base(&mut self.base, at)
    }

    // 活动和受阻构造共用：轮次与序号从 1 起；定义进入不带旧执行，只有原人恢复关联旧执行且必须创建活动令牌。
    fn new_current(input: NewNodeExecution, blocker_code: Option<ApprovalBlockerCode>) -> ModelResult<Self> {
        if input.round_no == 0 || input.execution_no == 0 {
            return Err(ModelError::InvalidField("轮次和执行序号必须从 1 开始"));
        }

        let status = if blocker_code.is_some() {
            ApprovalNodeExecutionStatus::Blocked
        } else {
            ApprovalNodeExecutionStatus::Active
        };
        match (input.assignment_source, input.replaces_execution_id.is_some(), status) {
            (ApprovalExecutionAssignmentSource::Definition, false, _)
            | (
                ApprovalExecutionAssignmentSource::AssigneeRecovery,
                true,
                ApprovalNodeExecutionStatus::Active,
            ) => {},
            _ => {
                return Err(ModelError::InvalidField(
                    "定义进入不得替换执行，原审批人恢复必须关联旧执行并创建活动执行",
                ));
            },
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
            additional_assignees: Vec::new(),
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

    // 通过和驳回复用同一落点，由活动执行记录决定、决定人和结束时间；已结束或受阻时不能覆盖既有事实。
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

    // 变更执行前拒绝已结束状态；受阻仍是当前令牌，能撤回或在专用恢复路径中被替换。
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

    // 验证构造入口分别生成活动或受阻状态，而不是未进入节点的等待记录。
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

    // 通过后再次驳回或试图替换旧执行均应失败，保护已记录决定。
    #[test]
    fn ended_execution_cannot_reopen() {
        let mut exec = active();
        exec.record_approve(ParticipantId::new("u1").unwrap(), None, Timestamp::from_unix_secs(3).unwrap())
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
            exec.supersede_for_assignee_recovery(Timestamp::from_unix_secs(5).unwrap()),
            Err(ModelError::InvalidStatus("只有受阻执行可以替换"))
        );
    }

    // 验证人员失效的旧执行结束时标记原人恢复，且保留当时的责任人快照。
    #[test]
    fn assignee_recovery_supersedes_blocked_and_keeps_snapshot() {
        let mut exec = active();
        exec.block(ApprovalBlockerCode::ApproverEmploymentInvalid, Timestamp::from_unix_secs(6).unwrap())
            .unwrap();
        let old_assignee = exec.assignee_participant_id.clone();
        exec.supersede_for_assignee_recovery(Timestamp::from_unix_secs(7).unwrap()).unwrap();
        assert_eq!(exec.status, ApprovalNodeExecutionStatus::Superseded);
        assert_eq!(exec.ended_reason, Some(ApprovalExecutionEndReason::AssigneeRecovered));
        assert_eq!(exec.assignee_participant_id, old_assignee);
    }

    // 验证直接调用执行模型也不能以原人恢复替换结构受阻执行；拒绝后仍保留原状态和未结束事实。
    #[test]
    fn structural_blocker_cannot_be_superseded_for_assignee_recovery() {
        let mut exec = active();
        exec.block(ApprovalBlockerCode::DefinitionGraphCorrupted, Timestamp::from_unix_secs(6).unwrap())
            .unwrap();
        assert_eq!(
            exec.supersede_for_assignee_recovery(Timestamp::from_unix_secs(7).unwrap()),
            Err(ModelError::InvalidStatus("结构性或一致性阻塞不得恢复原审批人"))
        );
        assert_eq!(exec.status, ApprovalNodeExecutionStatus::Blocked);
        assert!(exec.ended_reason.is_none());
        assert!(exec.ended_at.is_none());
    }

    // 覆盖定义进入携带旧引用、原人恢复缺旧引用以及受阻恢复，用于锁定来源、引用和活动态的配对约束。
    #[test]
    fn assignment_source_requires_matching_replacement_reference() {
        let invalid_definition = ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new("e3"),
            process_instance_id: ApprovalProcessInstanceId::new("inst"),
            node_key: "n1".into(),
            node_name: "仓储复核".into(),
            round_no: 1,
            execution_no: 2,
            assignment_source: ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: Some(ApprovalNodeExecutionId::new("e1")),
            assignee_participant_id: ParticipantId::new("u1").unwrap(),
            assignee_name_snapshot: "张三".into(),
            at: Timestamp::from_unix_secs(8).unwrap(),
        });
        assert_eq!(
            invalid_definition,
            Err(ModelError::InvalidField("定义进入不得替换执行，原审批人恢复必须关联旧执行并创建活动执行"))
        );

        let missing_replacement = ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new("e4"),
            process_instance_id: ApprovalProcessInstanceId::new("inst"),
            node_key: "n1".into(),
            node_name: "仓储复核".into(),
            round_no: 1,
            execution_no: 2,
            assignment_source: ApprovalExecutionAssignmentSource::AssigneeRecovery,
            replaces_execution_id: None,
            assignee_participant_id: ParticipantId::new("u1").unwrap(),
            assignee_name_snapshot: "张三".into(),
            at: Timestamp::from_unix_secs(8).unwrap(),
        });
        assert_eq!(
            missing_replacement,
            Err(ModelError::InvalidField("定义进入不得替换执行，原审批人恢复必须关联旧执行并创建活动执行"))
        );

        let blocked_recovery = ApprovalNodeExecution::new_blocked(
            NewNodeExecution {
                id: ApprovalNodeExecutionId::new("e5"),
                process_instance_id: ApprovalProcessInstanceId::new("inst"),
                node_key: "n1".into(),
                node_name: "仓储复核".into(),
                round_no: 1,
                execution_no: 2,
                assignment_source: ApprovalExecutionAssignmentSource::AssigneeRecovery,
                replaces_execution_id: Some(ApprovalNodeExecutionId::new("e1")),
                assignee_participant_id: ParticipantId::new("u1").unwrap(),
                assignee_name_snapshot: "张三".into(),
                at: Timestamp::from_unix_secs(8).unwrap(),
            },
            ApprovalBlockerCode::ApproverNotEligible,
        );
        assert_eq!(
            blocked_recovery,
            Err(ModelError::InvalidField("定义进入不得替换执行，原审批人恢复必须关联旧执行并创建活动执行"))
        );
    }
}
