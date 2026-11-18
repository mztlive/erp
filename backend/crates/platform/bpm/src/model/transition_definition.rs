//! 审批连线定义。单条连线只校验自身形状，不识别入口或末节点。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::ids::{ApprovalProcessDefinitionId, ApprovalTransitionDefinitionId};
use crate::model::Timestamp;
use crate::model::types::{
    ApprovalTerminalResult, ApprovalTransitionEvent, ModelError, ModelResult, NODE_KEY_MAX_LEN,
    base_model_at, normalize_required,
};

/// 节点在事件发生后的唯一流向。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ApprovalTransitionDefinition {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属定义。
    pub process_definition_id: ApprovalProcessDefinitionId,
    /// 事件来源节点。
    pub from_node_key: String,
    /// 通过或驳回。
    pub event: ApprovalTransitionEvent,
    /// 指向下一节点时必填。
    pub to_node_key: Option<String>,
    /// 指向终态时必填，且必须与事件一致。
    pub terminal_result: Option<ApprovalTerminalResult>,
}

impl ApprovalTransitionDefinition {
    /// 为草稿线性图创建从当前节点到下一节点的通过边，不写终态。旧驳回节点边仅用于读取冻结图，新定义应使用 [`Self::to_rejected`]。
    /// 来源键或目标键非法、时间不可持久化时返回错误；是否为相邻节点由整图校验。
    pub fn to_node(
        id: ApprovalTransitionDefinitionId,
        process_definition_id: ApprovalProcessDefinitionId,
        from_node_key: impl Into<String>,
        event: ApprovalTransitionEvent,
        to_node_key: impl Into<String>,
        at: Timestamp,
    ) -> ModelResult<Self> {
        Ok(Self {
            base: base_model_at(id.to_string(), at)?,
            process_definition_id,
            from_node_key: normalize_required(
                from_node_key,
                "来源节点键不能为空",
                NODE_KEY_MAX_LEN,
                "来源节点键过长",
            )?,
            event,
            to_node_key: Some(normalize_required(
                to_node_key,
                "目标节点键不能为空",
                NODE_KEY_MAX_LEN,
                "目标节点键过长",
            )?),
            terminal_result: None,
        })
    }

    /// 为线性图末节点创建指向最终通过的边，不再指定后继节点。
    /// 只接受通过事件；来源键非法或时间不可持久化时返回错误。
    pub fn to_approved(
        id: ApprovalTransitionDefinitionId,
        process_definition_id: ApprovalProcessDefinitionId,
        from_node_key: impl Into<String>,
        event: ApprovalTransitionEvent,
        at: Timestamp,
    ) -> ModelResult<Self> {
        if event != ApprovalTransitionEvent::Approve {
            return Err(ModelError::InvalidTransition("驳回连线不得携带终态"));
        }
        Ok(Self {
            base: base_model_at(id.to_string(), at)?,
            process_definition_id,
            from_node_key: normalize_required(
                from_node_key,
                "来源节点键不能为空",
                NODE_KEY_MAX_LEN,
                "来源节点键过长",
            )?,
            event,
            to_node_key: None,
            terminal_result: Some(ApprovalTerminalResult::Approved),
        })
    }

    /// 为新线性图的每个节点创建驳回终态边，让驳回收结当前审批而不回到入口；来源键非法或时间不可持久化时返回错误。
    pub fn to_rejected(
        id: ApprovalTransitionDefinitionId,
        process_definition_id: ApprovalProcessDefinitionId,
        from_node_key: impl Into<String>,
        at: Timestamp,
    ) -> ModelResult<Self> {
        let mut transition = Self::to_approved(
            id,
            process_definition_id,
            from_node_key,
            ApprovalTransitionEvent::Approve,
            at,
        )?;
        transition.event = ApprovalTransitionEvent::Reject;
        transition.terminal_result = Some(ApprovalTerminalResult::Rejected);
        Ok(transition)
    }

    /// 构图或解释决定时确认连线有且仅有一个目标：非空节点键，或与事件匹配的终态。
    /// 非法组合返回 [`ModelError::InvalidTransition`]；是否为入口、末节点或兼容的历史驳回目标须由整图或运行规则判断。
    pub fn validate_shape(&self) -> ModelResult<()> {
        match (self.event, self.to_node_key.as_deref(), self.terminal_result) {
            (_, Some(to), None) => {
                if to.is_empty() {
                    return Err(ModelError::InvalidTransition("目标节点键不能为空"));
                }
                Ok(())
            },
            (ApprovalTransitionEvent::Approve, None, Some(ApprovalTerminalResult::Approved)) => Ok(()),
            (ApprovalTransitionEvent::Reject, None, Some(ApprovalTerminalResult::Rejected)) => Ok(()),
            (_, None, Some(_)) => Err(ModelError::InvalidTransition("连线事件与终态不一致")),
            (ApprovalTransitionEvent::Approve, None, None) => {
                Err(ModelError::InvalidTransition("通过连线必须指向节点或终态"))
            },
            (_, Some(_), Some(_)) => Err(ModelError::InvalidTransition("节点目标与终态不能同时存在")),
            (_, None, None) => Err(ModelError::InvalidTransition("连线必须有且仅有一个目标")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ApprovalTransitionDefinition;
    use crate::ids::{ApprovalProcessDefinitionId, ApprovalTransitionDefinitionId};
    use crate::model::Timestamp;
    use crate::model::types::{ApprovalTerminalResult, ApprovalTransitionEvent, ModelError};

    fn def_id() -> ApprovalProcessDefinitionId {
        ApprovalProcessDefinitionId::new("def")
    }

    // 验证事件与终态必须配对，同时保留单边层面对历史驳回节点目标的读取兼容。
    #[test]
    fn transition_shape_rules() {
        let at = Timestamp::from_unix_secs(1).unwrap();
        let approve_node = ApprovalTransitionDefinition::to_node(
            ApprovalTransitionDefinitionId::new("t1"),
            def_id(),
            "n1",
            ApprovalTransitionEvent::Approve,
            "n2",
            at,
        )
        .unwrap();
        assert!(approve_node.validate_shape().is_ok());

        let approve_end = ApprovalTransitionDefinition::to_approved(
            ApprovalTransitionDefinitionId::new("t2"),
            def_id(),
            "n2",
            ApprovalTransitionEvent::Approve,
            at,
        )
        .unwrap();
        assert_eq!(approve_end.terminal_result, Some(ApprovalTerminalResult::Approved));
        assert!(approve_end.to_node_key.is_none());

        assert!(matches!(
            ApprovalTransitionDefinition::to_approved(
                ApprovalTransitionDefinitionId::new("t3"),
                def_id(),
                "n1",
                ApprovalTransitionEvent::Reject,
                at,
            ),
            Err(ModelError::InvalidTransition(_))
        ));

        let reject = ApprovalTransitionDefinition::to_node(
            ApprovalTransitionDefinitionId::new("t4"),
            def_id(),
            "n2",
            ApprovalTransitionEvent::Reject,
            "n1",
            at,
        )
        .unwrap();
        assert!(reject.validate_shape().is_ok());

        let mut rejected = ApprovalTransitionDefinition::to_rejected(
            ApprovalTransitionDefinitionId::new("t5"),
            def_id(),
            "n2",
            at,
        )
        .unwrap();
        assert!(rejected.validate_shape().is_ok());
        assert_eq!(rejected.terminal_result, Some(ApprovalTerminalResult::Rejected));

        rejected.event = ApprovalTransitionEvent::Approve;
        assert!(rejected.validate_shape().is_err());
        rejected.event = ApprovalTransitionEvent::Reject;
        rejected.to_node_key = Some("n1".into());
        assert!(rejected.validate_shape().is_err());
    }
}
