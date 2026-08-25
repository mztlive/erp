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

/// 创建人工审批节点所需的身份与责任快照。
pub struct NewNodeDefinition {
    /// 节点主键。
    pub id: ApprovalNodeDefinitionId,
    /// 所属定义。
    pub process_definition_id: ApprovalProcessDefinitionId,
    /// 稳定节点键。
    pub node_key: String,
    /// 节点名称。
    pub node_name: String,
    /// 可选不透明用途。
    pub node_purpose: Option<String>,
    /// 从 1 开始的展示顺序。
    pub display_order: u32,
    /// 指定审批人。
    pub assignee_participant_id: ParticipantId,
    /// 发布时冻结的显示名。
    pub assignee_label_snapshot: String,
    /// 调用方时间。
    pub at: Timestamp,
}

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
    /// * `input` - 节点身份、顺序与审批人快照
    ///
    /// # 返回
    /// 返回人工审批节点。
    ///
    /// # 错误
    /// 键、名称、顺序或快照非法时返回错误。
    ///
    /// # 约束
    /// 节点类型固定为人工审批，BPM 不解释 `node_purpose`。
    pub fn new(input: NewNodeDefinition) -> ModelResult<Self> {
        if input.display_order == 0 {
            return Err(ModelError::InvalidField("节点顺序必须从 1 开始"));
        }
        Ok(Self {
            base: base_model_at(input.id.to_string(), input.at)?,
            process_definition_id: input.process_definition_id,
            node_key: normalize_required(input.node_key, "节点键不能为空", NODE_KEY_MAX_LEN, "节点键过长")?,
            node_name: normalize_required(input.node_name, "节点名称不能为空", NAME_MAX_LEN, "节点名称过长")?,
            node_type: ApprovalNodeType::UserApproval,
            node_purpose: normalize_optional(input.node_purpose, PURPOSE_MAX_LEN, "用途键过长")?,
            display_order: input.display_order,
            assignee_participant_id: input.assignee_participant_id,
            assignee_label_snapshot: normalize_required(
                input.assignee_label_snapshot,
                "审批人显示名不能为空",
                LABEL_MAX_LEN,
                "审批人显示名过长",
            )?,
        })
    }

    /// 以当前节点身份与流程字段刷新审批人显示名快照。
    ///
    /// # 参数
    /// * `assignee_label_snapshot` - 当前账号显示名
    /// * `at` - 调用方提供的快照时间
    ///
    /// # 返回
    /// 返回保留节点 ID、键、顺序、用途与审批人引用的新节点快照。
    ///
    /// # 错误
    /// 显示名为空、超长或当前节点字段已损坏时返回模型错误。
    ///
    /// # 关键业务约束
    /// BPM 不查询账号；调用方必须先完成账号与权限重验，再把显示名传入本方法。
    pub fn with_assignee_label_snapshot(
        &self,
        assignee_label_snapshot: impl Into<String>,
        at: Timestamp,
    ) -> ModelResult<Self> {
        Self::new(NewNodeDefinition {
            id: ApprovalNodeDefinitionId::new(self.base.id.clone()),
            process_definition_id: self.process_definition_id.clone(),
            node_key: self.node_key.clone(),
            node_name: self.node_name.clone(),
            node_purpose: self.node_purpose.clone(),
            display_order: self.display_order,
            assignee_participant_id: self.assignee_participant_id.clone(),
            assignee_label_snapshot: assignee_label_snapshot.into(),
            at,
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
        assert!(ApprovalNodeDefinition::new(super::NewNodeDefinition {
            id: ApprovalNodeDefinitionId::new("n-id"),
            process_definition_id: ApprovalProcessDefinitionId::new("def"),
            node_key: "n1".into(),
            node_name: "仓储复核".into(),
            node_purpose: None,
            display_order: 0,
            assignee_participant_id: assignee.clone(),
            assignee_label_snapshot: "张三".into(),
            at,
        })
        .is_err());
        assert!(ApprovalNodeDefinition::new(super::NewNodeDefinition {
            id: ApprovalNodeDefinitionId::new("n-id"),
            process_definition_id: ApprovalProcessDefinitionId::new("def"),
            node_key: "  ".into(),
            node_name: "仓储复核".into(),
            node_purpose: None,
            display_order: 1,
            assignee_participant_id: assignee.clone(),
            assignee_label_snapshot: "张三".into(),
            at,
        })
        .is_err());
        let node = ApprovalNodeDefinition::new(super::NewNodeDefinition {
            id: ApprovalNodeDefinitionId::new("n-id"),
            process_definition_id: ApprovalProcessDefinitionId::new("def"),
            node_key: "n1".into(),
            node_name: "仓储复核".into(),
            node_purpose: None,
            display_order: 1,
            assignee_participant_id: assignee,
            assignee_label_snapshot: "张三".into(),
            at,
        })
        .unwrap();
        assert_eq!(node.node_type, ApprovalNodeType::UserApproval);
        assert!(node.node_purpose.is_none());

        let refreshed = node
            .with_assignee_label_snapshot("李四", Timestamp::from_unix_secs(2).unwrap())
            .unwrap();
        assert_eq!(refreshed.base.id, node.base.id);
        assert_eq!(refreshed.node_key, node.node_key);
        assert_eq!(refreshed.assignee_label_snapshot, "李四");
        assert!(node
            .with_assignee_label_snapshot("   ", Timestamp::from_unix_secs(2).unwrap())
            .is_err());
    }
}
