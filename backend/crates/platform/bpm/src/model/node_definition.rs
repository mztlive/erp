//! 审批节点定义。不保存待办类型、处理器或责任池字段。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use super::approval_participant::validate_additional;
use crate::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId};
use crate::model::types::{
    ApprovalNodeType, LABEL_MAX_LEN, ModelError, ModelResult, NAME_MAX_LEN, NODE_KEY_MAX_LEN,
    PURPOSE_MAX_LEN, base_model_at, normalize_optional, normalize_required, trim_required,
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
    /// 草稿组装节点时设置动态指派规则；指定人员节点用 `None`。设置后立即核验指派形态，拒绝不完整或混用的规则与人员。
    pub fn with_rule(mut self, rule: Option<String>) -> ModelResult<Self> {
        self.assignee_rule = rule;
        self.validate_assignment()?;
        Ok(self)
    }

    /// 校验草稿定义或已解析运行图的指派：指定人员必须有身份且没有解析依据，动态规则则允许尚未解析。
    /// 动态节点不能混入或签人员；已解析事实须匹配规则和显示名，且申请人、来源与指派标识齐全。
    pub fn validate_assignment(&self) -> ModelResult<()> {
        trim_required(
            &self.assignee_label_snapshot,
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

    /// 引擎准备冻结绑定或创建执行时取已确定的首位审批人；未解析的动态节点无身份，不能进入运行路径。
    pub fn assignee(&self) -> ModelResult<&ParticipantId> {
        self.assignee_participant_id.as_ref().ok_or(ModelError::InvalidField("审批人来源尚未解析"))
    }

    /// 为指定人员节点附上其他或签人，供运行时按成员派任务；人数、身份重复或显示名不合法时返回错误。
    pub fn with_additional_assignees(mut self, assignees: Vec<ApprovalParticipant>) -> ModelResult<Self> {
        self.additional_assignees = assignees;
        self.validate_additional_assignees()?;
        Ok(self)
    }

    // 校验构图或回读所得的或签名单，不复制快照；禁止身份重复、超过 50 人，或未解析动态节点混入指定人员。
    pub(crate) fn validate_additional_assignees(&self) -> ModelResult<()> {
        if let Some(id) = &self.assignee_participant_id {
            validate_additional(id, &self.additional_assignees)?;
        } else if !self.additional_assignees.is_empty() {
            return Err(ModelError::InvalidField("动态节点不得混合指定人员"));
        }
        Ok(())
    }

    /// 为资格收敛或展示提取人员快照，首位在前、其他或签成员随后；未解析动态节点不会凭规则生成成员。
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

    /// 校验指定身份是否命中节点已知的主审批人或其他或签成员；不尝试解析动态规则。
    pub fn has_participant(&self, id: &str) -> bool {
        self.assignee_participant_id.as_ref().is_some_and(|value| value.as_str() == id)
            || self.additional_assignees.iter().any(|item| item.participant_id.as_str() == id)
    }

    /// 规范化节点显示名，供构造节点和计算命令摘要使用同一值。
    /// 裁剪后为空或超过 [`NAME_MAX_LEN`] 个 UTF-8 字节时返回 [`ModelError::InvalidField`]。
    pub fn normalize_name(name: impl Into<String>) -> ModelResult<String> {
        normalize_required(name, "节点名称不能为空", NAME_MAX_LEN, "节点名称过长")
    }

    /// 组装草稿时用调用方提供的人员快照创建人工审批节点，规范化节点键、名称和可选用途，不查询账号或解释用途。
    /// 初始不含规则、解析依据和其他或签人；组装后再校验完整指派。顺序为零、文本非法或时间不可持久化时返回错误。
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

    /// 调用方复核账号和权限后，用取得的显示名重建草稿节点快照，沿用节点身份、顺序、用途、规则与人员引用。
    /// 此操作不就地更新节点，也不查询账号；显示名或重建后的字段非法时返回错误。
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

    // 覆盖构建人工节点时的零顺序和空键，并验证刷新显示名不会更换稳定身份。
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

    // 检查公开名称规范化的裁剪和 UTF-8 字节边界，使命令摘要能与模型中落下的名称保持一致。
    #[test]
    fn node_name_normalization_is_public_and_matches_constructor() {
        assert_eq!(ApprovalNodeDefinition::normalize_name("  仓储复核  ").unwrap(), "仓储复核");
        assert!(ApprovalNodeDefinition::normalize_name("  ").is_err());
        assert!(ApprovalNodeDefinition::normalize_name("界".repeat(85)).is_ok());
        assert!(ApprovalNodeDefinition::normalize_name("界".repeat(86)).is_err());
    }
}
