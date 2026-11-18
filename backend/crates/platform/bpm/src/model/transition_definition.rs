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
    /// 创建指向下一节点的连线。
    ///
    /// 通过连线指向下一节点；历史驳回连线仅用于读取已冻结定义。
    ///
    /// # 参数
    /// * `id` - 连线主键
    /// * `process_definition_id` - 所属定义
    /// * `from_node_key` - 来源节点
    /// * `event` - 事件
    /// * `to_node_key` - 目标节点
    /// * `at` - 调用方时间
    ///
    /// # 返回
    /// 返回指向节点、终态为空的连线。
    ///
    /// # 错误
    /// 来源或目标节点键为空或超长，或调用方时间无法持久化时返回错误。
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

    /// 创建指向最终通过的连线。
    ///
    /// 通过事件只能指向通过终态。
    ///
    /// # 参数
    /// * `id` - 连线主键
    /// * `process_definition_id` - 所属定义
    /// * `from_node_key` - 来源节点
    /// * `event` - 必须是通过
    /// * `at` - 调用方时间
    ///
    /// # 返回
    /// 返回终态为通过、目标节点为空的连线。
    ///
    /// # 错误
    /// 事件不是通过、来源键为空或超长，或调用方时间无法持久化时返回错误。
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

    /// 创建指向本次审批已驳回终态的连线。
    ///
    /// # 参数
    /// * `id` - 连线主键。
    /// * `process_definition_id` - 所属定义。
    /// * `from_node_key` - 来源节点。
    /// * `at` - 调用方时间。
    /// # 返回
    /// 返回驳回事件对应的终态连线。
    /// # 错误
    /// 来源键非法或时间无法持久化时返回错误。
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

    /// 校验单条连线形状：节点目标与终态恰有一个。
    ///
    /// 本方法不判断该节点是否入口或末节点。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 形状合法时返回 `Ok(())`。
    ///
    /// # 错误
    /// 目标组合不合法时返回 [`ModelError::InvalidTransition`]。
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

    /// 通过与驳回终态匹配事件，并兼容历史驳回节点连线。
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
