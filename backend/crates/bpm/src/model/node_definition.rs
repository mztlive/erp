//! 审批节点定义。不保存待办类型、处理器或责任池字段。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId};
use crate::model::types::{
    base_model_at, normalize_optional, normalize_required, ApprovalNodeType, ModelError, ModelResult,
    LABEL_MAX_LEN, NAME_MAX_LEN, NODE_KEY_MAX_LEN, PURPOSE_MAX_LEN,
};
use crate::model::{ParticipantId, Timestamp};

/// 定义版本内的人工审批节点。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ApprovalNodeDefinition {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属定义。
    pub process_definition_id: ApprovalProcessDefinitionId,
    /// 定义内稳定节点键。
    pub node_key: String,
    /// 面向用户的节点名称。
    pub node_name: String,
    /// 节点类型。第一阶段仅人工审批。
    pub node_type: ApprovalNodeType,
    /// 不透明用途键；BPM 不解释其业务含义。
    pub node_purpose: Option<String>,
    /// 从 1 开始的展示顺序。
    pub display_order: u32,
    /// 指定审批人。
    pub assignee_participant_id: ParticipantId,
    /// 发布时冻结的显示名。
    pub assignee_label_snapshot: String,
}

impl ApprovalNodeDefinition {
    /// 创建人工审批节点。
    ///
    /// 处理人与显示名由调用方提供；本模块不查询账号或组织。
    ///
    /// # 参数
    /// * `id` - 节点主键
    /// * `process_definition_id` - 所属定义
    /// * `node_key` - 稳定节点键
    /// * `node_name` - 节点名称
    /// * `node_purpose` - 可选不透明用途
    /// * `display_order` - 从 1 开始的顺序
    /// * `assignee_participant_id` - 指定审批人
    /// * `assignee_label_snapshot` - 显示名快照
    /// * `at` - 调用方时间
    ///
    /// # 错误
    /// 键、名称、顺序或快照非法时返回错误。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: ApprovalNodeDefinitionId,
        process_definition_id: ApprovalProcessDefinitionId,
        node_key: impl Into<String>,
        node_name: impl Into<String>,
        node_purpose: Option<String>,
        display_order: u32,
        assignee_participant_id: ParticipantId,
        assignee_label_snapshot: impl Into<String>,
        at: Timestamp,
    ) -> ModelResult<Self> {
        if display_order == 0 {
            return Err(ModelError::InvalidField("节点顺序必须从 1 开始"));
        }
        Ok(Self {
            base: base_model_at(id.to_string(), at)?,
            process_definition_id,
            node_key: normalize_required(node_key, "节点键不能为空", NODE_KEY_MAX_LEN, "节点键过长")?,
            node_name: normalize_required(node_name, "节点名称不能为空", NAME_MAX_LEN, "节点名称过长")?,
            node_type: ApprovalNodeType::UserApproval,
            node_purpose: normalize_optional(node_purpose, PURPOSE_MAX_LEN, "用途键过长")?,
            display_order,
            assignee_participant_id,
            assignee_label_snapshot: normalize_required(
                assignee_label_snapshot,
                "审批人显示名不能为空",
                LABEL_MAX_LEN,
                "审批人显示名过长",
            )?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ApprovalNodeDefinition;
    use crate::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId};
    use crate::model::types::ApprovalNodeType;
    use crate::model::{ParticipantId, Timestamp};

    /// 节点只允许人工审批类型，顺序 0 被拒。
    #[test]
    fn node_rejects_zero_order_and_empty_key() {
        let at = Timestamp::from_unix_secs(1).unwrap();
        let assignee = ParticipantId::new("u1").unwrap();
        assert!(ApprovalNodeDefinition::new(
            ApprovalNodeDefinitionId::new("n-id"),
            ApprovalProcessDefinitionId::new("def"),
            "n1",
            "仓储复核",
            None,
            0,
            assignee.clone(),
            "张三",
            at,
        )
        .is_err());
        assert!(ApprovalNodeDefinition::new(
            ApprovalNodeDefinitionId::new("n-id"),
            ApprovalProcessDefinitionId::new("def"),
            "  ",
            "仓储复核",
            None,
            1,
            assignee.clone(),
            "张三",
            at,
        )
        .is_err());
        let node = ApprovalNodeDefinition::new(
            ApprovalNodeDefinitionId::new("n-id"),
            ApprovalProcessDefinitionId::new("def"),
            "n1",
            "仓储复核",
            None,
            1,
            assignee,
            "张三",
            at,
        )
        .unwrap();
        assert_eq!(node.node_type, ApprovalNodeType::UserApproval);
        assert!(node.node_purpose.is_none());
    }
}
