//! 审批连线定义。单条连线只校验自身形状，不识别入口或末节点。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::ids::{ApprovalProcessDefinitionId, ApprovalTransitionDefinitionId};
use crate::model::types::{
    base_model_at, normalize_required, ApprovalTerminalResult, ApprovalTransitionEvent, ModelError,
    ModelResult, NODE_KEY_MAX_LEN,
};
use crate::model::Timestamp;

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
    /// 指向终态时必填，且只能是通过。
    pub terminal_result: Option<ApprovalTerminalResult>,
}

impl ApprovalTransitionDefinition {
    /// 创建指向下一节点的连线。
    ///
    /// 驳回连线必须走本入口；通过连线也可以指向节点。
    ///
    /// # 参数
    /// * `id` - 连线主键
    /// * `process_definition_id` - 所属定义
    /// * `from_node_key` - 来源节点
    /// * `event` - 事件
    /// * `to_node_key` - 目标节点
    /// * `at` - 调用方时间
    ///
    /// # 错误
    /// 节点键非法时返回错误。
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
    /// 驳回不得携带终态。
    ///
    /// # 参数
    /// * `id` - 连线主键
    /// * `process_definition_id` - 所属定义
    /// * `from_node_key` - 来源节点
    /// * `event` - 必须是通过
    /// * `at` - 调用方时间
    ///
    /// # 错误
    /// 事件不是通过或来源键非法时返回错误。
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

    /// 校验单条连线形状：节点目标与终态恰有一个。
    ///
    /// 本方法不判断该节点是否入口或末节点。
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
            }
            (ApprovalTransitionEvent::Approve, None, Some(ApprovalTerminalResult::Approved)) => Ok(()),
            (ApprovalTransitionEvent::Reject, _, Some(_)) => {
                Err(ModelError::InvalidTransition("驳回连线必须指向节点"))
            }
            (ApprovalTransitionEvent::Approve, None, None) => {
                Err(ModelError::InvalidTransition("通过连线必须指向节点或终态"))
            }
            (_, Some(_), Some(_)) => Err(ModelError::InvalidTransition("节点目标与终态不能同时存在")),
            (_, None, None) => Err(ModelError::InvalidTransition("连线必须有且仅有一个目标")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ApprovalTransitionDefinition;
    use crate::ids::{ApprovalProcessDefinitionId, ApprovalTransitionDefinitionId};
    use crate::model::types::{ApprovalTerminalResult, ApprovalTransitionEvent, ModelError};
    use crate::model::Timestamp;

    fn def_id() -> ApprovalProcessDefinitionId {
        ApprovalProcessDefinitionId::new("def")
    }

    /// 通过可指向节点或终态；驳回只能指向节点。
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
        assert_eq!(
            approve_end.terminal_result,
            Some(ApprovalTerminalResult::Approved)
        );
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
    }
}
