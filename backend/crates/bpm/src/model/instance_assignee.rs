//! 实例审批人绑定。定义审批人永久不变。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::ids::{ApprovalInstanceAssigneeId, ApprovalProcessInstanceId};
use crate::model::types::{
    base_model_at, normalize_required, touch_base, ApprovalAssigneeBindingSource, ModelError, ModelResult,
    NODE_KEY_MAX_LEN, REASON_MAX_LEN,
};
use crate::model::{ParticipantId, Timestamp};

/// 实例内某个节点的当前有效审批人。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ApprovalInstanceAssignee {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属实例。
    pub process_instance_id: ApprovalProcessInstanceId,
    /// 实例内节点键。
    pub node_key: String,
    /// 从发布定义复制，永久保留。
    pub definition_assignee_participant_id: ParticipantId,
    /// 当前及后续轮次使用的责任人。
    pub current_assignee_participant_id: ParticipantId,
    /// 绑定来源，不能是人员恢复。
    pub assignment_source: ApprovalAssigneeBindingSource,
    /// 改派人。
    pub changed_by: Option<ParticipantId>,
    /// 改派时间。
    pub changed_at: Option<Timestamp>,
    /// 改派原因。
    pub change_reason: Option<String>,
}

impl ApprovalInstanceAssignee {
    /// 从定义审批人冻结实例绑定。
    ///
    /// # 参数
    /// * `id` - 绑定主键
    /// * `process_instance_id` - 所属实例
    /// * `node_key` - 节点键
    /// * `definition_assignee` - 定义审批人
    /// * `at` - 调用方时间
    ///
    /// # 错误
    /// 节点键非法时返回错误。
    pub fn from_definition(
        id: ApprovalInstanceAssigneeId,
        process_instance_id: ApprovalProcessInstanceId,
        node_key: impl Into<String>,
        definition_assignee: ParticipantId,
        at: Timestamp,
    ) -> ModelResult<Self> {
        Ok(Self {
            base: base_model_at(id.to_string(), at)?,
            process_instance_id,
            node_key: normalize_required(node_key, "节点键不能为空", NODE_KEY_MAX_LEN, "节点键过长")?,
            definition_assignee_participant_id: definition_assignee.clone(),
            current_assignee_participant_id: definition_assignee,
            assignment_source: ApprovalAssigneeBindingSource::Definition,
            changed_by: None,
            changed_at: None,
            change_reason: None,
        })
    }

    /// 返回改派乐观锁版本。
    ///
    /// # 返回
    /// 返回 `base.version`。
    pub fn assignment_version(&self) -> u64 {
        self.base.version
    }

    /// 原子更新当前审批人与改派审计，定义审批人不变。
    ///
    /// # 参数
    /// * `target` - 新的当前审批人
    /// * `actor` - 改派人
    /// * `reason` - 非空原因
    /// * `at` - 改派时间
    ///
    /// # 错误
    /// 原因为空或目标与当前相同时返回错误。
    pub fn reassign(
        &mut self,
        target: ParticipantId,
        actor: ParticipantId,
        reason: impl Into<String>,
        at: Timestamp,
    ) -> ModelResult<()> {
        let reason = normalize_required(reason, "改派原因不能为空", REASON_MAX_LEN, "改派原因过长")
            .map_err(|_| ModelError::EmptyReassignReason)?;
        if target == self.current_assignee_participant_id {
            return Err(ModelError::MeaninglessReassign);
        }
        let definition_assignee = self.definition_assignee_participant_id.clone();
        self.current_assignee_participant_id = target;
        self.assignment_source = ApprovalAssigneeBindingSource::AdminReassign;
        self.changed_by = Some(actor);
        self.changed_at = Some(at);
        self.change_reason = Some(reason);
        self.definition_assignee_participant_id = definition_assignee;
        touch_base(&mut self.base, at)
    }
}

#[cfg(test)]
mod tests {
    use super::ApprovalInstanceAssignee;
    use crate::ids::{ApprovalInstanceAssigneeId, ApprovalProcessInstanceId};
    use crate::model::types::{ApprovalAssigneeBindingSource, ModelError};
    use crate::model::{ParticipantId, Timestamp};

    fn binding() -> ApprovalInstanceAssignee {
        ApprovalInstanceAssignee::from_definition(
            ApprovalInstanceAssigneeId::new("a1"),
            ApprovalProcessInstanceId::new("inst"),
            "n1",
            ParticipantId::new("u1").unwrap(),
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap()
    }

    /// 定义审批人在改派后保持不变。
    #[test]
    fn reassign_keeps_definition_assignee() {
        let mut assignee = binding();
        let original = assignee.definition_assignee_participant_id.clone();
        assignee
            .reassign(
                ParticipantId::new("u2").unwrap(),
                ParticipantId::new("admin").unwrap(),
                "人员失效",
                Timestamp::from_unix_secs(2).unwrap(),
            )
            .unwrap();
        assert_eq!(assignee.definition_assignee_participant_id, original);
        assert_eq!(assignee.current_assignee_participant_id.as_str(), "u2");
        assert_eq!(
            assignee.assignment_source,
            ApprovalAssigneeBindingSource::AdminReassign
        );
    }

    /// 空原因与同人改派失败关闭。
    #[test]
    fn reassign_rejects_empty_reason_and_same_person() {
        let mut assignee = binding();
        assert_eq!(
            assignee.reassign(
                ParticipantId::new("u2").unwrap(),
                ParticipantId::new("admin").unwrap(),
                "  ",
                Timestamp::from_unix_secs(2).unwrap(),
            ),
            Err(ModelError::EmptyReassignReason)
        );
        assert_eq!(
            assignee.reassign(
                ParticipantId::new("u1").unwrap(),
                ParticipantId::new("admin").unwrap(),
                "相同的人",
                Timestamp::from_unix_secs(2).unwrap(),
            ),
            Err(ModelError::MeaninglessReassign)
        );
    }
}
