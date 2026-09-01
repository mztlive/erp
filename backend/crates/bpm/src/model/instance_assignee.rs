//! 实例审批人绑定。定义审批人永久不变。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::ids::{ApprovalInstanceAssigneeId, ApprovalProcessInstanceId};
use crate::model::types::{
    base_model_at, normalize_required, ApprovalAssigneeBindingSource, ModelError, ModelResult,
    NODE_KEY_MAX_LEN,
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
    /// 当前及后续轮次使用的责任人，永久等于定义审批人。
    pub current_assignee_participant_id: ParticipantId,
    /// 绑定来源，永久为定义冻结。
    pub assignment_source: ApprovalAssigneeBindingSource,
    /// 保留的变更审计字段，运行时永久为空。
    pub changed_by: Option<ParticipantId>,
    /// 保留的变更时间字段，运行时永久为空。
    pub changed_at: Option<Timestamp>,
    /// 保留的变更原因字段，运行时永久为空。
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

    /// 校验实例绑定仍是启动时从定义冻结的原始事实。
    ///
    /// # 错误
    /// 来源、当前责任人或保留审计字段发生变化时返回错误。
    pub fn ensure_unchanged_from_definition(&self) -> ModelResult<()> {
        if self.assignment_source != ApprovalAssigneeBindingSource::Definition
            || self.current_assignee_participant_id != self.definition_assignee_participant_id
            || self.changed_by.is_some()
            || self.changed_at.is_some()
            || self.change_reason.is_some()
        {
            return Err(ModelError::InvalidStatus("实例审批人绑定必须保持定义快照不变"));
        }
        Ok(())
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

    /// 定义冻结绑定的责任人与保留审计字段必须保持不变。
    #[test]
    fn definition_binding_is_unchanged() {
        let assignee = binding();
        assert_eq!(
            assignee.assignment_source,
            ApprovalAssigneeBindingSource::Definition
        );
        assert_eq!(
            assignee.current_assignee_participant_id,
            assignee.definition_assignee_participant_id
        );
        assert!(assignee.changed_by.is_none());
        assert!(assignee.changed_at.is_none());
        assert!(assignee.change_reason.is_none());
        assert!(assignee.ensure_unchanged_from_definition().is_ok());
    }

    /// 当前责任或任一保留审计字段漂移时失败关闭。
    #[test]
    fn changed_definition_binding_is_rejected() {
        let expected = Err(ModelError::InvalidStatus("实例审批人绑定必须保持定义快照不变"));

        let mut changed_current = binding();
        changed_current.current_assignee_participant_id = ParticipantId::new("u2").unwrap();
        assert_eq!(changed_current.ensure_unchanged_from_definition(), expected);

        let mut changed_by = binding();
        changed_by.changed_by = Some(ParticipantId::new("operator").unwrap());
        assert_eq!(changed_by.ensure_unchanged_from_definition(), expected);

        let mut changed_at = binding();
        changed_at.changed_at = Some(Timestamp::from_unix_secs(2).unwrap());
        assert_eq!(changed_at.ensure_unchanged_from_definition(), expected);

        let mut changed_reason = binding();
        changed_reason.change_reason = Some("unexpected change".into());
        assert_eq!(changed_reason.ensure_unchanged_from_definition(), expected);
    }
}
