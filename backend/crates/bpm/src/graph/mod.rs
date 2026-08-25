//! BPM 定义图规则：节点规划、线性连线生成、发布图校验与运行期连线解析。

pub mod linear;
pub mod validator;

use std::collections::{HashMap, HashSet};

use crate::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId, ApprovalTransitionDefinitionId};
use crate::model::types::{ApprovalDecision, ApprovalTransitionEvent, ModelError, ModelResult};
use crate::model::{
    ApprovalNodeDefinition, ApprovalProcessDefinition, ApprovalTransitionDefinition, NewNodeDefinition,
    ParticipantId, ProcessKind, Timestamp,
};

pub use linear::{build_linear_transitions, generate_linear_transitions, LinearTransitionDraft};
pub use validator::{ordered_nodes, validate_entry_node, validate_linear_graph, validate_transition};

/// 单个审批定义允许的最大节点数。
pub const MAX_DEFINITION_NODES: usize = 20;

/// 引擎与定义管理共用的纯 BPM 定义图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionGraph {
    /// 流程定义。
    pub definition: ApprovalProcessDefinition,
    /// 定义节点。
    pub nodes: Vec<ApprovalNodeDefinition>,
    /// 定义连线。
    pub transitions: Vec<ApprovalTransitionDefinition>,
}

/// 节点整组替换的 BPM 输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeReplacementDraft {
    /// 被保留的已有节点 ID；为空表示创建新节点。
    pub existing_node_id: Option<ApprovalNodeDefinitionId>,
    /// 新节点使用的调用方生成 ID；保留已有节点时忽略。
    pub new_node_id: ApprovalNodeDefinitionId,
    /// 新节点使用的调用方生成稳定键；保留已有节点时忽略。
    pub new_node_key: String,
    /// 节点显示名称。
    pub node_name: String,
    /// 从 1 开始的展示顺序。
    pub display_order: u32,
    /// 指定审批人。
    pub assignee_participant_id: ParticipantId,
}

/// 复制节点时由调用方提供的新身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopiedNodeIdentity {
    /// 新节点 ID。
    pub node_id: ApprovalNodeDefinitionId,
    /// 新节点稳定键。
    pub node_key: String,
}

impl DefinitionGraph {
    /// 按节点键查找节点。
    ///
    /// # 参数
    /// * `node_key` - 定义内稳定节点键
    ///
    /// # 返回
    /// 命中时返回节点引用，否则返回 `None`。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 本方法不为缺失节点补默认值。
    pub fn node(&self, node_key: &str) -> Option<&ApprovalNodeDefinition> {
        self.nodes.iter().find(|item| item.node_key == node_key)
    }

    /// 返回按展示顺序排列且顺序连续的节点。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回从 `display_order = 1` 开始的节点引用。
    ///
    /// # 错误
    /// 节点数量越界或展示顺序不连续时返回模型错误。
    ///
    /// # 关键业务约束
    /// 节点数量固定在 `1..=20`，同一顺序不得重复。
    pub fn ordered_nodes(&self) -> ModelResult<Vec<&ApprovalNodeDefinition>> {
        ordered_nodes(&self.nodes)
    }

    /// 提取按展示顺序排列的节点键。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回从入口开始的节点键。
    ///
    /// # 错误
    /// 节点数量或顺序非法时返回模型错误。
    ///
    /// # 关键业务约束
    /// 返回顺序可直接交给线性连线生成器。
    pub fn ordered_node_keys(&self) -> ModelResult<Vec<String>> {
        Ok(self
            .ordered_nodes()?
            .into_iter()
            .map(|node| node.node_key.clone())
            .collect())
    }

    /// 提取定义内确定性的审批人 ID 集合。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回按字符串排序并去重的审批人 ID。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 输出不得依赖节点存储顺序，供批量账号查询与资格重验复用。
    pub fn assignee_ids(&self) -> Vec<String> {
        assignee_ids(&self.nodes)
    }

    /// 提取节点用途引用。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回每个节点当前保存的可选用途键。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// BPM 只保持用途为不透明字符串，不解释 ERP 含义。
    pub fn purpose_refs(&self) -> Vec<Option<&str>> {
        self.nodes
            .iter()
            .map(|node| node.node_purpose.as_deref())
            .collect()
    }

    /// 校验定义图符合完整线性审批模型。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 入口、节点顺序和全部连线与线性模型一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 节点数量、顺序、入口或连线不一致时返回模型错误。
    ///
    /// # 关键业务约束
    /// 每个节点必须恰有通过与驳回连线，驳回回入口，末节点通过进入已批准终态。
    pub fn validate_linear(&self) -> ModelResult<()> {
        validate_linear_graph(&self.definition, &self.nodes, &self.transitions)
    }

    /// 解析当前节点决定连线的目标节点。
    ///
    /// # 参数
    /// * `current_node_key` - 当前节点键
    /// * `decision` - 通过或驳回决定
    ///
    /// # 返回
    /// 连线指向节点时返回目标键，指向终态时返回 `None`。
    ///
    /// # 错误
    /// 决定连线缺失、重复或形状损坏时返回模型错误。
    ///
    /// # 关键业务约束
    /// 同一来源节点与事件必须恰有一条连线，不得取第一条掩盖重复定义。
    pub fn decision_target_node_key(
        &self,
        current_node_key: &str,
        decision: ApprovalDecision,
    ) -> ModelResult<Option<String>> {
        let event = match decision {
            ApprovalDecision::Approve => ApprovalTransitionEvent::Approve,
            ApprovalDecision::Reject => ApprovalTransitionEvent::Reject,
        };
        let mut matches = self
            .transitions
            .iter()
            .filter(|item| item.from_node_key == current_node_key && item.event == event);
        let edge = matches
            .next()
            .ok_or(ModelError::InvalidTransition("审批定义缺少决定连线"))?;
        if matches.next().is_some() {
            return Err(ModelError::InvalidTransition("审批定义存在重复决定连线"));
        }
        edge.validate_shape()?;
        Ok(edge.to_node_key.clone())
    }

    /// 按整组替换输入规划草稿节点。
    ///
    /// # 参数
    /// * `drafts` - 调用方提供的已有节点引用、新身份、名称、顺序与审批人
    /// * `at` - 调用方提供的规划时间
    ///
    /// # 返回
    /// 返回按展示顺序排列、用途已清空的完整节点集合。
    ///
    /// # 错误
    /// 节点数量、顺序、名称、身份、跨定义引用或重复引用非法时返回模型错误。
    ///
    /// # 关键业务约束
    /// 已有节点保留 ID 与 `node_key`；新节点只使用调用方提供的新身份；历史用途不得复制。
    pub fn plan_replacement_nodes(
        &self,
        drafts: &[NodeReplacementDraft],
        at: Timestamp,
    ) -> ModelResult<Vec<ApprovalNodeDefinition>> {
        self.definition.ensure_mutable()?;
        let drafts = ordered_replacement_drafts(drafts)?;
        let existing = self
            .nodes
            .iter()
            .map(|node| (node.base.id.as_str(), node))
            .collect::<HashMap<_, _>>();
        let mut seen_existing = HashSet::new();
        let mut planned = Vec::with_capacity(drafts.len());
        for draft in drafts {
            let (node_id, node_key) =
                replacement_identity(&self.definition.base.id, &existing, &mut seen_existing, draft)?;
            planned.push(ApprovalNodeDefinition::new(NewNodeDefinition {
                id: node_id,
                process_definition_id: ApprovalProcessDefinitionId::new(self.definition.base.id.clone()),
                node_key,
                node_name: draft.node_name.clone(),
                node_purpose: None,
                display_order: draft.display_order,
                assignee_participant_id: draft.assignee_participant_id.clone(),
                assignee_label_snapshot: "pending".to_string(),
                at,
            })?);
        }
        ensure_unique_node_identities(&planned)?;
        Ok(planned)
    }

    /// 以新节点集合重建现有草稿的入口与线性连线。
    ///
    /// # 参数
    /// * `definition` - 需要保持身份与业务版本的草稿定义
    /// * `nodes` - 完整替换后的节点集合
    /// * `transition_ids` - 调用方为每条线性连线提供的 ID
    /// * `at` - 调用方提供的重建时间
    ///
    /// # 返回
    /// 返回保持定义身份、重写入口与连线后的完整图。
    ///
    /// # 错误
    /// 定义不可修改、节点非法或连线 ID 数量不匹配时返回模型错误。
    ///
    /// # 关键业务约束
    /// 入口固定为 `display_order = 1` 的节点键，连线完全由线性生成器产生。
    pub fn rebuild_draft(
        definition: &ApprovalProcessDefinition,
        nodes: Vec<ApprovalNodeDefinition>,
        transition_ids: Vec<ApprovalTransitionDefinitionId>,
        at: Timestamp,
    ) -> ModelResult<Self> {
        let ordered = ordered_nodes(&nodes)?;
        ensure_nodes_belong_to_definition(&nodes, &definition.base.id)?;
        let entry = ordered[0].node_key.clone();
        let mut definition = definition.clone();
        definition.set_entry_node_draft(entry, at)?;
        let transitions = build_linear_transitions(
            &ApprovalProcessDefinitionId::new(definition.base.id.clone()),
            &nodes,
            transition_ids,
            at,
        )?;
        let graph = Self {
            definition,
            nodes,
            transitions,
        };
        graph.validate_linear()?;
        Ok(graph)
    }

    /// 由已复制节点构造新的非空草稿图。
    ///
    /// # 参数
    /// * `definition_id` - 新定义 ID
    /// * `process_kind` - 流程种类
    /// * `definition_version` - 同流程种类内的新业务版本
    /// * `name` - 草稿名称
    /// * `created_by` - 草稿创建人
    /// * `nodes` - 已归属新定义的节点
    /// * `transition_ids` - 调用方为每条线性连线提供的 ID
    /// * `at` - 调用方提供的创建时间
    ///
    /// # 返回
    /// 返回入口和线性连线完整的新草稿图。
    ///
    /// # 错误
    /// 定义字段、节点归属、顺序或连线 ID 数量非法时返回模型错误。
    ///
    /// # 关键业务约束
    /// 本入口只用于至少一个节点的复制草稿；空草稿由定义实体单独创建。
    #[allow(clippy::too_many_arguments)]
    pub fn new_populated_draft(
        definition_id: ApprovalProcessDefinitionId,
        process_kind: ProcessKind,
        definition_version: u32,
        name: impl Into<String>,
        created_by: ParticipantId,
        nodes: Vec<ApprovalNodeDefinition>,
        transition_ids: Vec<ApprovalTransitionDefinitionId>,
        at: Timestamp,
    ) -> ModelResult<Self> {
        let ordered = ordered_nodes(&nodes)?;
        ensure_nodes_belong_to_definition(&nodes, definition_id.as_ref())?;
        let definition = ApprovalProcessDefinition::new_draft(
            definition_id,
            process_kind,
            definition_version,
            name,
            ordered[0].node_key.clone(),
            created_by,
            at,
        )?;
        Self::rebuild_draft(&definition, nodes, transition_ids, at)
    }
}

/// 提取节点集合中确定性的审批人 ID。
///
/// # 参数
/// * `nodes` - 任意存储顺序的节点集合
///
/// # 返回
/// 返回按字符串排序并去重的审批人 ID。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 输出不得依赖节点存储顺序，供批量账号查询与资格重验复用。
pub fn assignee_ids(nodes: &[ApprovalNodeDefinition]) -> Vec<String> {
    let mut ids = nodes
        .iter()
        .map(|node| node.assignee_participant_id.as_str().to_string())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

/// 把已发布节点复制为新定义节点，清除历史用途并替换全部身份。
///
/// # 参数
/// * `source` - 被复制定义的节点集合
/// * `definition_id` - 新草稿定义 ID
/// * `identities` - 与有序源节点一一对应的新节点身份
/// * `at` - 调用方提供的复制时间
///
/// # 返回
/// 返回按原展示顺序复制、用途为空的新节点集合。
///
/// # 错误
/// 源节点顺序非法、身份数量不匹配或新节点字段非法时返回模型错误。
///
/// # 关键业务约束
/// 不复制节点 ID、`node_key` 或历史用途，但保留名称、顺序、审批人与显示名快照。
pub fn copy_nodes_for_definition(
    source: &[ApprovalNodeDefinition],
    definition_id: ApprovalProcessDefinitionId,
    identities: &[CopiedNodeIdentity],
    at: Timestamp,
) -> ModelResult<Vec<ApprovalNodeDefinition>> {
    let ordered = ordered_nodes(source)?;
    if ordered.len() != identities.len() {
        return Err(ModelError::InvalidField("复制节点身份数量不匹配"));
    }
    let mut copied = Vec::with_capacity(ordered.len());
    for (node, identity) in ordered.into_iter().zip(identities) {
        copied.push(ApprovalNodeDefinition::new(NewNodeDefinition {
            id: identity.node_id.clone(),
            process_definition_id: definition_id.clone(),
            node_key: identity.node_key.clone(),
            node_name: node.node_name.clone(),
            node_purpose: None,
            display_order: node.display_order,
            assignee_participant_id: node.assignee_participant_id.clone(),
            assignee_label_snapshot: node.assignee_label_snapshot.clone(),
            at,
        })?);
    }
    ensure_unique_node_identities(&copied)?;
    Ok(copied)
}

/// 按展示顺序排列节点替换输入并校验连续顺序。
///
/// # 参数
/// * `drafts` - 未排序的整组替换输入
///
/// # 返回
/// 返回从 1 开始连续排列的输入引用。
///
/// # 错误
/// 节点数量不在 `1..=20` 或顺序不连续时返回模型错误。
fn ordered_replacement_drafts(drafts: &[NodeReplacementDraft]) -> ModelResult<Vec<&NodeReplacementDraft>> {
    if !(1..=MAX_DEFINITION_NODES).contains(&drafts.len()) {
        return Err(ModelError::InvalidField("审批节点数量必须在 1 到 20 之间"));
    }
    let mut ordered = drafts.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|draft| draft.display_order);
    for (index, draft) in ordered.iter().enumerate() {
        let expected = u32::try_from(index + 1).map_err(|_| ModelError::InvalidField("节点顺序溢出"))?;
        if draft.display_order != expected {
            return Err(ModelError::InvalidField("节点顺序必须从 1 连续且无重复"));
        }
    }
    Ok(ordered)
}

/// 解析替换输入应使用的节点身份。
///
/// # 参数
/// * `definition_id` - 当前草稿定义 ID
/// * `existing` - 当前定义内按节点 ID 建立的索引
/// * `seen_existing` - 本次已引用的已有节点 ID
/// * `draft` - 当前替换输入
///
/// # 返回
/// 已有节点返回原 ID 与键，新节点返回调用方提供的新身份。
///
/// # 错误
/// 已有节点重复、跨定义引用或节点归属损坏时返回模型错误。
fn replacement_identity(
    definition_id: &str,
    existing: &HashMap<&str, &ApprovalNodeDefinition>,
    seen_existing: &mut HashSet<String>,
    draft: &NodeReplacementDraft,
) -> ModelResult<(ApprovalNodeDefinitionId, String)> {
    let Some(existing_id) = draft.existing_node_id.as_ref() else {
        return Ok((draft.new_node_id.clone(), draft.new_node_key.clone()));
    };
    if !seen_existing.insert(existing_id.as_ref().to_string()) {
        return Err(ModelError::InvalidField("节点ID不能重复"));
    }
    let node = existing
        .get(existing_id.as_ref())
        .ok_or(ModelError::InvalidField(
            "节点不属于当前草稿，不能跨定义引用或改写已删除节点",
        ))?;
    if node.process_definition_id.as_ref() != definition_id {
        return Err(ModelError::InvalidField("节点归属定义不一致"));
    }
    Ok((
        ApprovalNodeDefinitionId::new(node.base.id.clone()),
        node.node_key.clone(),
    ))
}

/// 校验节点 ID 与稳定键在定义内均唯一。
///
/// # 参数
/// * `nodes` - 待持久化的完整节点集合
///
/// # 返回
/// 身份均唯一时返回 `Ok(())`。
///
/// # 错误
/// 节点 ID 或稳定键重复时返回模型错误。
fn ensure_unique_node_identities(nodes: &[ApprovalNodeDefinition]) -> ModelResult<()> {
    let mut ids = HashSet::new();
    let mut keys = HashSet::new();
    for node in nodes {
        if !ids.insert(node.base.id.as_str()) {
            return Err(ModelError::InvalidField("节点ID不能重复"));
        }
        if !keys.insert(node.node_key.as_str()) {
            return Err(ModelError::InvalidField("节点键不能重复"));
        }
    }
    Ok(())
}

/// 校验全部节点归属于给定定义。
///
/// # 参数
/// * `nodes` - 待构图节点
/// * `definition_id` - 目标定义 ID
///
/// # 返回
/// 全部节点归属一致时返回 `Ok(())`。
///
/// # 错误
/// 任一节点指向其它定义时返回模型错误。
fn ensure_nodes_belong_to_definition(
    nodes: &[ApprovalNodeDefinition],
    definition_id: &str,
) -> ModelResult<()> {
    if nodes
        .iter()
        .all(|node| node.process_definition_id.as_ref() == definition_id)
    {
        return Ok(());
    }
    Err(ModelError::InvalidField("节点归属定义不一致"))
}

#[cfg(test)]
mod tests {
    use super::{copy_nodes_for_definition, CopiedNodeIdentity, DefinitionGraph, NodeReplacementDraft};
    use crate::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId, ApprovalTransitionDefinitionId};
    use crate::model::types::ApprovalDecision;
    use crate::model::{
        ApprovalNodeDefinition, ApprovalProcessDefinition, NewNodeDefinition, ParticipantId, ProcessKind,
        Timestamp,
    };

    /// 构造最小草稿定义。
    fn definition(entry: &str) -> ApprovalProcessDefinition {
        ApprovalProcessDefinition::new_draft(
            ApprovalProcessDefinitionId::new("def"),
            ProcessKind::StockAdjustment,
            1,
            "库存调整",
            entry,
            ParticipantId::new("admin").unwrap(),
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap()
    }

    /// 构造带可选历史用途的节点。
    fn node(id: &str, key: &str, order: u32, purpose: Option<&str>) -> ApprovalNodeDefinition {
        ApprovalNodeDefinition::new(NewNodeDefinition {
            id: ApprovalNodeDefinitionId::new(id),
            process_definition_id: ApprovalProcessDefinitionId::new("def"),
            node_key: key.to_string(),
            node_name: format!("节点{order}"),
            node_purpose: purpose.map(ToOwned::to_owned),
            display_order: order,
            assignee_participant_id: ParticipantId::new(format!("u{order}")).unwrap(),
            assignee_label_snapshot: format!("人员{order}"),
            at: Timestamp::from_unix_secs(1).unwrap(),
        })
        .unwrap()
    }

    /// 节点替换保留已有身份、使用调用方新身份，并拒绝重复与跨定义引用。
    #[test]
    fn replacement_planning_owns_node_identity_rules() {
        let graph = DefinitionGraph {
            definition: definition("n1"),
            nodes: vec![node("id1", "n1", 1, Some("legacy"))],
            transitions: Vec::new(),
        };
        let planned = graph
            .plan_replacement_nodes(
                &[
                    NodeReplacementDraft {
                        existing_node_id: None,
                        new_node_id: ApprovalNodeDefinitionId::new("id2"),
                        new_node_key: "n2".to_string(),
                        node_name: "财务".to_string(),
                        display_order: 2,
                        assignee_participant_id: ParticipantId::new("u2").unwrap(),
                    },
                    NodeReplacementDraft {
                        existing_node_id: Some(ApprovalNodeDefinitionId::new("id1")),
                        new_node_id: ApprovalNodeDefinitionId::new("ignored"),
                        new_node_key: "ignored".to_string(),
                        node_name: "仓储".to_string(),
                        display_order: 1,
                        assignee_participant_id: ParticipantId::new("u1").unwrap(),
                    },
                ],
                Timestamp::from_unix_secs(2).unwrap(),
            )
            .unwrap();
        assert_eq!(planned[0].base.id, "id1");
        assert_eq!(planned[0].node_key, "n1");
        assert!(planned[0].node_purpose.is_none());
        assert_eq!(planned[1].base.id, "id2");
        assert_eq!(planned[1].node_key, "n2");

        let duplicate = NodeReplacementDraft {
            existing_node_id: Some(ApprovalNodeDefinitionId::new("id1")),
            new_node_id: ApprovalNodeDefinitionId::new("ignored"),
            new_node_key: "ignored".to_string(),
            node_name: "仓储".to_string(),
            display_order: 1,
            assignee_participant_id: ParticipantId::new("u1").unwrap(),
        };
        let mut second = duplicate.clone();
        second.display_order = 2;
        assert!(graph
            .plan_replacement_nodes(&[duplicate, second], Timestamp::from_unix_secs(2).unwrap(),)
            .is_err());
        assert!(graph
            .plan_replacement_nodes(
                &[NodeReplacementDraft {
                    existing_node_id: Some(ApprovalNodeDefinitionId::new("foreign")),
                    new_node_id: ApprovalNodeDefinitionId::new("ignored"),
                    new_node_key: "ignored".to_string(),
                    node_name: "仓储".to_string(),
                    display_order: 1,
                    assignee_participant_id: ParticipantId::new("u1").unwrap(),
                }],
                Timestamp::from_unix_secs(2).unwrap(),
            )
            .is_err());
        assert!(graph
            .plan_replacement_nodes(&[], Timestamp::from_unix_secs(2).unwrap())
            .is_err());
    }

    /// 节点复制替换全部身份并清除用途，结果可直接重建为合法线性图。
    #[test]
    fn copied_nodes_and_rebuilt_graph_are_deterministic() {
        let source = vec![
            node("old2", "old-n2", 2, None),
            node("old1", "old-n1", 1, Some("legacy")),
        ];
        let new_definition_id = ApprovalProcessDefinitionId::new("new-def");
        let copied = copy_nodes_for_definition(
            &source,
            new_definition_id.clone(),
            &[
                CopiedNodeIdentity {
                    node_id: ApprovalNodeDefinitionId::new("new1"),
                    node_key: "new-n1".to_string(),
                },
                CopiedNodeIdentity {
                    node_id: ApprovalNodeDefinitionId::new("new2"),
                    node_key: "new-n2".to_string(),
                },
            ],
            Timestamp::from_unix_secs(2).unwrap(),
        )
        .unwrap();
        assert_eq!(copied[0].base.id, "new1");
        assert_eq!(copied[0].node_name, "节点1");
        assert!(copied.iter().all(|item| item.node_purpose.is_none()));

        let graph = DefinitionGraph::new_populated_draft(
            new_definition_id,
            ProcessKind::StockAdjustment,
            2,
            "复制草稿",
            ParticipantId::new("admin").unwrap(),
            copied,
            (1..=4)
                .map(|index| ApprovalTransitionDefinitionId::new(format!("t{index}")))
                .collect(),
            Timestamp::from_unix_secs(2).unwrap(),
        )
        .unwrap();
        graph.validate_linear().unwrap();
        assert_eq!(graph.definition.entry_node_key, "new-n1");
        assert_eq!(
            graph
                .decision_target_node_key("new-n1", ApprovalDecision::Approve)
                .unwrap()
                .as_deref(),
            Some("new-n2")
        );
        assert_eq!(
            graph
                .decision_target_node_key("new-n2", ApprovalDecision::Approve)
                .unwrap(),
            None
        );
        assert!(graph
            .decision_target_node_key("missing", ApprovalDecision::Approve)
            .is_err());
    }
}
