//! 审批节点定义。不保存待办类型、处理器或责任池字段。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use super::approval_participant::validate_additional;
use crate::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId};
use crate::model::types::{
    ApprovalNodeType, LABEL_MAX_LEN, ModelError, ModelResult, NAME_MAX_LEN, NODE_KEY_MAX_LEN,
    PURPOSE_MAX_LEN, base_model_at, normalize_optional, normalize_required,
};
use crate::model::{ApprovalParticipant, AssigneeResolution, ParticipantId, Timestamp};

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
    pub assignee_participant_id: Option<ParticipantId>,
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
    pub assignee_participant_id: Option<ParticipantId>,
    /// 发布时冻结的显示名。
    pub assignee_label_snapshot: String,
    /// 不透明的动态指派规则；解析由调用方完成。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee_rule: Option<String>,
    /// 动态规则的本次解析依据，仅运行内存图使用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<AssigneeResolution>,
    /// 同节点的其他或签审批人；缺失时兼容单人节点。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_assignees: Vec<ApprovalParticipant>,
}

impl ApprovalNodeDefinition {
    /// 配置动态指派规则，指定人员节点传空。
    /// # 参数
    /// `rule` 是调用方识别的规则键。
    /// # 返回
    /// 返回校验后的节点。
    /// # 错误
    /// 规则与人员混用或均缺失时拒绝。
    pub fn with_rule(mut self, rule: Option<String>) -> ModelResult<Self> {
        self.assignee_rule = rule;
        self.validate_assignment()?;
        Ok(self)
    }

    /// 校验定义规则或已解析运行图的指派完整性。
    /// # 参数
    /// 无。
    /// # 返回
    /// 合法指派返回成功。
    /// # 错误
    /// 缺少规则或人员、动态规则混合或签时拒绝。
    pub fn validate_assignment(&self) -> ModelResult<()> {
        normalize_required(
            self.assignee_label_snapshot.clone(),
            "审批人显示名不能为空",
            LABEL_MAX_LEN,
            "审批人显示名过长",
        )?;
        if let Some(rule) = &self.assignee_rule {
            if rule.trim().is_empty()
                || rule.len() > 64
                || !self.additional_assignees.is_empty()
                || self.assignee_participant_id.is_some() != self.resolution.is_some()
                || self.resolution.as_ref().is_some_and(|r| {
                    &r.rule != rule
                        || r.subject_id.is_empty()
                        || r.source_id.is_empty()
                        || r.assignment_id.is_empty()
                        || r.participant_name != self.assignee_label_snapshot
                })
            {
                return Err(ModelError::InvalidField("动态指派规则与解析结果不匹配"));
            }
        } else if self.assignee_participant_id.is_none() || self.resolution.is_some() {
            return Err(ModelError::InvalidField("指定审批人不能为空"));
        }
        Ok(())
    }

    /// 读取已确定的审批人；未解析规则不得进入引擎。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回首位审批人。
    /// # 错误
    /// 未解析时拒绝。
    pub fn assignee(&self) -> ModelResult<&ParticipantId> {
        self.assignee_participant_id.as_ref().ok_or(ModelError::InvalidField("审批人来源尚未解析"))
    }

    /// 设置同节点的其他或签审批人。
    /// # 参数
    /// `assignees` 为除首位之外的参与人快照。
    /// # 返回
    /// 返回包含完整或签集合的节点。
    /// # 错误
    /// 人员重复、超限或显示名非法时失败。
    pub fn with_additional_assignees(mut self, assignees: Vec<ApprovalParticipant>) -> ModelResult<Self> {
        if let Some(id) = &self.assignee_participant_id {
            validate_additional(id, &assignees)?;
        } else if !assignees.is_empty() {
            return Err(ModelError::InvalidField("动态节点不得混合指定人员"));
        }
        self.additional_assignees = assignees;
        Ok(self)
    }

    /// 返回冻结的全部参与人。
    /// # 参数
    /// 无。
    /// # 返回
    /// 首位与其他参与人按定义顺序返回。
    /// # 错误
    /// 无。
    pub fn participants(&self) -> Vec<ApprovalParticipant> {
        let mut result = self
            .assignee_participant_id
            .iter()
            .map(|id| ApprovalParticipant {
                participant_id: id.clone(),
                name: self.assignee_label_snapshot.clone(),
            })
            .collect::<Vec<_>>();
        result.extend(self.additional_assignees.clone());
        result
    }

    /// 判断身份是否属于冻结参与人。
    /// # 参数
    /// `id` 为待校验身份。
    /// # 返回
    /// 集合内身份返回 true。
    /// # 错误
    /// 无。
    pub fn has_participant(&self, id: &str) -> bool {
        self.assignee_participant_id.as_ref().is_some_and(|value| value.as_str() == id)
            || self.additional_assignees.iter().any(|item| item.participant_id.as_str() == id)
    }

    /// 规范化节点显示名称，供命令摘要与实体构造共享同一规则源。
    ///
    /// # 参数
    /// * `name` - 调用方提供的节点名称
    ///
    /// # 返回
    /// 返回去除首尾空白后的名称。
    ///
    /// # 错误
    /// trim 后为空或超过 [`NAME_MAX_LEN`] 个 UTF-8 字节时返回 [`ModelError::InvalidField`]。
    pub fn normalize_name(name: impl Into<String>) -> ModelResult<String> {
        normalize_required(name, "节点名称不能为空", NAME_MAX_LEN, "节点名称过长")
    }

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
            node_name: Self::normalize_name(input.node_name)?,
            node_type: ApprovalNodeType::UserApproval,
            node_purpose: normalize_optional(input.node_purpose, PURPOSE_MAX_LEN, "用途键过长")?,
            display_order: input.display_order,
            additional_assignees: Vec::new(),
            assignee_rule: None,
            resolution: None,
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
        })?
        .with_additional_assignees(self.additional_assignees.clone())?
        .with_rule(self.assignee_rule.clone())
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
        assert!(
            ApprovalNodeDefinition::new(super::NewNodeDefinition {
                id: ApprovalNodeDefinitionId::new("n-id"),
                process_definition_id: ApprovalProcessDefinitionId::new("def"),
                node_key: "n1".into(),
                node_name: "仓储复核".into(),
                node_purpose: None,
                display_order: 0,
                assignee_participant_id: Some(assignee.clone()),
                assignee_label_snapshot: "张三".into(),
                at,
            })
            .is_err()
        );
        assert!(
            ApprovalNodeDefinition::new(super::NewNodeDefinition {
                id: ApprovalNodeDefinitionId::new("n-id"),
                process_definition_id: ApprovalProcessDefinitionId::new("def"),
                node_key: "  ".into(),
                node_name: "仓储复核".into(),
                node_purpose: None,
                display_order: 1,
                assignee_participant_id: Some(assignee.clone()),
                assignee_label_snapshot: "张三".into(),
                at,
            })
            .is_err()
        );
        let node = ApprovalNodeDefinition::new(super::NewNodeDefinition {
            id: ApprovalNodeDefinitionId::new("n-id"),
            process_definition_id: ApprovalProcessDefinitionId::new("def"),
            node_key: "n1".into(),
            node_name: "仓储复核".into(),
            node_purpose: None,
            display_order: 1,
            assignee_participant_id: Some(assignee),
            assignee_label_snapshot: "张三".into(),
            at,
        })
        .unwrap();
        assert_eq!(node.node_type, ApprovalNodeType::UserApproval);
        assert!(node.node_purpose.is_none());

        let refreshed =
            node.with_assignee_label_snapshot("李四", Timestamp::from_unix_secs(2).unwrap()).unwrap();
        assert_eq!(refreshed.base.id, node.base.id);
        assert_eq!(refreshed.node_key, node.node_key);
        assert_eq!(refreshed.assignee_label_snapshot, "李四");
        assert!(node.with_assignee_label_snapshot("   ", Timestamp::from_unix_secs(2).unwrap()).is_err());
    }

    /// 摘要准备与实体构造必须共享 trim 和 UTF-8 字节边界。
    #[test]
    fn node_name_normalization_is_public_and_matches_constructor() {
        assert_eq!(ApprovalNodeDefinition::normalize_name("  仓储复核  ").unwrap(), "仓储复核");
        assert!(ApprovalNodeDefinition::normalize_name("  ").is_err());
        assert!(ApprovalNodeDefinition::normalize_name("界".repeat(85)).is_ok());
        assert!(ApprovalNodeDefinition::normalize_name("界".repeat(86)).is_err());
    }
}
