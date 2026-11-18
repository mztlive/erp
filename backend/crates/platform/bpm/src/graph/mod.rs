//! BPM 定义图规则：节点规划、线性连线生成、发布图校验与运行期连线解析。

mod assignment;
pub mod linear;
pub mod validator;

use std::collections::{HashMap, HashSet};

pub use linear::{LinearTransitionDraft, build_linear_transitions, generate_linear_transitions};
pub use validator::{ordered_nodes, validate_entry_node, validate_linear_graph, validate_transition};

use crate::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId, ApprovalTransitionDefinitionId};
use crate::model::types::{
    ApprovalDecision, ApprovalTransitionEvent, ModelError, ModelResult, NODE_KEY_MAX_LEN, normalize_required,
};
use crate::model::{
    ApprovalNodeDefinition, ApprovalParticipant, ApprovalProcessDefinition, ApprovalTransitionDefinition,
    NewNodeDefinition, ParticipantId, ProcessKind, Timestamp,
};

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
    pub assignee_participant_id: Option<ParticipantId>,
    /// 不透明动态指派规则。
    pub assignee_rule: Option<String>,
    /// 同节点的其他或签参与人。
    pub additional_assignees: Vec<ApprovalParticipant>,
}

impl NodeReplacementDraft {
    /// 将整组替换命令中的单个节点整理为领域输入：规范化名称和新稳定键，检查非零顺序及调用方提供的身份。
    /// 即使保留已有节点，也校验提供的新主键和新键，但规划时不用它们替换旧身份。
    /// 是否引用了当前草稿的节点、是否重复使用旧 ID，由 [`DefinitionGraph::plan_replacement_nodes`] 统一判定。
    pub fn new(
        existing_node_id: Option<ApprovalNodeDefinitionId>,
        new_node_id: ApprovalNodeDefinitionId,
        new_node_key: impl Into<String>,
        node_name: impl Into<String>,
        display_order: u32,
        assignee_participant_id: impl Into<Option<ParticipantId>>,
    ) -> ModelResult<Self> {
        if display_order == 0 {
            return Err(ModelError::InvalidField("节点顺序必须从 1 开始"));
        }
        if existing_node_id.as_ref().is_some_and(|existing| existing.as_ref().trim().is_empty()) {
            return Err(ModelError::InvalidField("已有节点ID不能为空"));
        }
        if new_node_id.as_ref().trim().is_empty() {
            return Err(ModelError::InvalidField("新节点ID不能为空"));
        }
        let new_node_key =
            normalize_required(new_node_key, "节点键不能为空", NODE_KEY_MAX_LEN, "节点键过长")?;

        Ok(Self {
            existing_node_id,
            new_node_id,
            new_node_key,
            node_name: ApprovalNodeDefinition::normalize_name(node_name)?,
            display_order,
            assignee_participant_id: assignee_participant_id.into(),
            assignee_rule: None,
            additional_assignees: Vec::new(),
        })
    }
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
    /// 按定义内的稳定键定位入口、当前或后继节点；找不到时返回 `None`，由调用场景决定如何处理坏图。
    pub fn node(&self, node_key: &str) -> Option<&ApprovalNodeDefinition> {
        self.nodes.iter().find(|item| item.node_key == node_key)
    }

    /// 读取定义节点时以展示顺序为准，而不是存储顺序。空集合、超过 20 个节点或顺序不是从 1 连续递增时返回错误。
    pub fn ordered_nodes(&self) -> ModelResult<Vec<&ApprovalNodeDefinition>> {
        ordered_nodes(&self.nodes)
    }

    /// 供调用方批量读取账号并复验资格：收集指定及已解析的审批人和其他或签参与人，排序去重以消除存储顺序的影响。
    pub fn assignee_ids(&self) -> Vec<String> {
        assignee_ids(&self.nodes)
    }

    /// 把每个节点的可选用途键按现有节点顺序交给上层做绑定或解释；BPM 不解析这些不透明引用。
    pub fn purpose_refs(&self) -> Vec<Option<&str>> {
        self.nodes.iter().map(|node| node.node_purpose.as_deref()).collect()
    }

    /// 发布、绑定或重建草稿时核对整张图：入口必须排首位，每个节点各有一条通过和驳回边，末节点通过即收结。
    /// 驳回应指向终态；历史冻结的驳回回入口连线只作终止解释。节点、指派或连线不合法时返回模型错误。
    pub fn validate_linear(&self) -> ModelResult<()> {
        validate_linear_graph(&self.definition, &self.nodes, &self.transitions)
    }

    /// 绑定定义前同时确认状态已发布且图完整；仅按 `PUBLISHED` 查询不能排除图损坏。
    /// 草稿或退役返回状态错误，发布图不满足线性规则也不得绑定。
    pub fn validate_published_linear(&self) -> ModelResult<()> {
        if !self.definition.is_published() {
            return Err(ModelError::InvalidStatus("只有已发布定义可以绑定"));
        }
        self.validate_linear()
    }

    /// 按当前节点及决定解析后继键，供调用方为后继节点准备资格。决定终结实例时返回 `None`，包括历史上指回入口的驳回边。
    /// 缺边、重复边、连线形状非法或历史驳回边不指向入口时返回模型错误，不从重复边中任取一条。
    pub fn decision_target_node_key(
        &self,
        current_node_key: &str,
        decision: ApprovalDecision,
    ) -> ModelResult<Option<String>> {
        let event = match decision {
            ApprovalDecision::Approve => ApprovalTransitionEvent::Approve,
            ApprovalDecision::Reject => ApprovalTransitionEvent::Reject,
        };
        let edge = self.unique_transition(current_node_key, event)?;
        edge.validate_shape()?;

        if decision == ApprovalDecision::Reject {
            if edge.to_node_key.as_deref().is_some_and(|key| key != self.definition.entry_node_key) {
                return Err(ModelError::InvalidTransition("历史驳回连线必须指向入口"));
            }
            return Ok(None);
        }
        Ok(edge.to_node_key.clone())
    }

    // 在解释决定前按来源节点和事件取唯一边；缺边与重复都返回错误，避免凭存储顺序选择路径。
    pub(crate) fn unique_transition(
        &self,
        from_node_key: &str,
        event: ApprovalTransitionEvent,
    ) -> ModelResult<&ApprovalTransitionDefinition> {
        let mut matches =
            self.transitions.iter().filter(|item| item.from_node_key == from_node_key && item.event == event);
        let edge = matches.next().ok_or(ModelError::InvalidTransition("审批定义缺少决定连线"))?;
        if matches.next().is_some() {
            return Err(ModelError::InvalidTransition("审批定义存在重复决定连线"));
        }
        Ok(edge)
    }

    /// 草稿整组编辑时按展示顺序构造完整节点集：保留项沿用原 ID 和稳定键，新节点使用调用方提供的新身份。
    /// 输出清除历史用途，显示名暂作占位，供调用方补齐快照后重建图。
    ///
    /// # 错误
    /// 非草稿、顺序或节点字段非法、重复身份、跨定义引用或引用已删除节点时返回错误。
    pub fn plan_replacement_nodes(
        &self,
        drafts: &[NodeReplacementDraft],
        at: Timestamp,
    ) -> ModelResult<Vec<ApprovalNodeDefinition>> {
        self.definition.ensure_mutable()?;
        let drafts = ordered_by_display_order(drafts, |draft| draft.display_order)?;

        let existing = self.nodes.iter().map(|node| (node.base.id.as_str(), node)).collect::<HashMap<_, _>>();
        let mut seen_existing = HashSet::new();
        let mut planned = Vec::with_capacity(drafts.len());
        for draft in drafts {
            let (node_id, node_key) =
                replacement_identity(&self.definition.base.id, &existing, &mut seen_existing, draft)?;
            planned.push(
                ApprovalNodeDefinition::new(NewNodeDefinition {
                    id: node_id,
                    process_definition_id: ApprovalProcessDefinitionId::new(self.definition.base.id.clone()),
                    node_key,
                    node_name: draft.node_name.clone(),
                    node_purpose: None,
                    display_order: draft.display_order,
                    assignee_participant_id: draft.assignee_participant_id.clone(),
                    assignee_label_snapshot: "pending".to_string(),
                    at,
                })?
                .with_additional_assignees(draft.additional_assignees.clone())?
                .with_rule(draft.assignee_rule.clone())?,
            );
        }

        ensure_unique_node_identities(&planned)?;
        Ok(planned)
    }

    /// 节点整组替换后保留草稿 ID 和业务版本，把首位节点设为入口，并按节点顺序与调用方提供的连线 ID 重建完整图。
    /// 非草稿、节点为空或归属及顺序非法、连线 ID 数量不符、最终图校验失败时返回模型错误。
    pub fn rebuild_draft(
        definition: &ApprovalProcessDefinition,
        nodes: Vec<ApprovalNodeDefinition>,
        transition_ids: Vec<ApprovalTransitionDefinitionId>,
        at: Timestamp,
    ) -> ModelResult<Self> {
        let ordered = ordered_nodes_for_definition(&nodes, &definition.base.id)?;
        let entry = ordered[0].node_key.clone();
        let mut definition = definition.clone();
        definition.set_entry_node_draft(entry, at)?;

        let transitions = build_linear_transitions(
            &ApprovalProcessDefinitionId::new(definition.base.id.clone()),
            &nodes,
            transition_ids,
            at,
        )?;

        let graph = Self { definition, nodes, transitions };
        graph.validate_linear()?;
        Ok(graph)
    }

    /// 复制已发布定义时，把带新身份的节点组装成有入口和完整连线的新草稿。要创建不带节点的草稿，直接构造定义实体。
    /// 定义字段、节点归属或顺序不合法，以及连线 ID 数量不符时返回模型错误。
    pub fn new_populated_draft(params: NewPopulatedDraftParams) -> ModelResult<Self> {
        let NewPopulatedDraftParams {
            definition_id,
            process_kind,
            definition_version,
            name,
            created_by,
            nodes,
            transition_ids,
            at,
        } = params;
        let ordered = ordered_nodes_for_definition(&nodes, definition_id.as_ref())?;
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

/// 构造非空草稿定义图的输入参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPopulatedDraftParams {
    /// 新定义 ID。
    pub definition_id: ApprovalProcessDefinitionId,
    /// 流程种类。
    pub process_kind: ProcessKind,
    /// 同流程种类内的新业务版本。
    pub definition_version: u32,
    /// 草稿名称。
    pub name: String,
    /// 草稿创建人。
    pub created_by: ParticipantId,
    /// 已归属新定义的节点。
    pub nodes: Vec<ApprovalNodeDefinition>,
    /// 调用方为每条线性连线提供的 ID。
    pub transition_ids: Vec<ApprovalTransitionDefinitionId>,
    /// 调用方提供的创建时间。
    pub at: Timestamp,
}

/// 批量校验资格前汇总节点已知的首位审批人和其他或签成员；按身份字符串排序去重，避免同一人被重复查询。
pub fn assignee_ids(nodes: &[ApprovalNodeDefinition]) -> Vec<String> {
    let mut ids = nodes
        .iter()
        .flat_map(|node| {
            node.assignee_participant_id
                .iter()
                .chain(node.additional_assignees.iter().map(|person| &person.participant_id))
        })
        .map(|id| id.as_str().to_owned())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

/// 以旧定义创建新草稿时，按展示顺序使用调用方提供的新 ID 和稳定键复制节点。
/// 保留名称、顺序、人员与显示名快照及指派规则，不沿用历史用途和原身份。
///
/// # 错误
/// 源节点顺序非法、身份数量不符、新身份重复或节点字段不合法时返回模型错误。
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
        copied.push(
            ApprovalNodeDefinition::new(NewNodeDefinition {
                id: identity.node_id.clone(),
                process_definition_id: definition_id.clone(),
                node_key: identity.node_key.clone(),
                node_name: node.node_name.clone(),
                node_purpose: None,
                display_order: node.display_order,
                assignee_participant_id: node.assignee_participant_id.clone(),
                assignee_label_snapshot: node.assignee_label_snapshot.clone(),
                at,
            })?
            .with_additional_assignees(node.additional_assignees.clone())?
            .with_rule(node.assignee_rule.clone())?,
        );
    }

    ensure_unique_node_identities(&copied)?;
    Ok(copied)
}

// 定义节点和整组替换输入共用同一展示顺序规则：先限制数量为 1 至 `MAX_DEFINITION_NODES`，再按从 1 连续且不重复的顺序返回引用，否则报 `ModelError::InvalidField`。
pub(crate) fn ordered_by_display_order<T>(
    items: &[T],
    display_order: impl Fn(&T) -> u32,
) -> ModelResult<Vec<&T>> {
    if !(1..=MAX_DEFINITION_NODES).contains(&items.len()) {
        return Err(ModelError::InvalidField("审批节点数量必须在 1 到 20 之间"));
    }
    let mut ordered = items.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|item| display_order(item));
    if ordered.iter().zip(1_u32..).any(|(item, expected)| display_order(item) != expected) {
        return Err(ModelError::InvalidField("节点顺序必须从 1 连续且无重复"));
    }
    Ok(ordered)
}

// 整组替换逐项决定是沿用旧节点的主键与稳定键，还是接收调用方提供的新身份；拦截重复引用、跨定义引用和已删除节点。
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
        .ok_or(ModelError::InvalidField("节点不属于当前草稿，不能跨定义引用或改写已删除节点"))?;
    if node.process_definition_id.as_ref() != definition_id {
        return Err(ModelError::InvalidField("节点归属定义不一致"));
    }
    Ok((ApprovalNodeDefinitionId::new(node.base.id.clone()), node.node_key.clone()))
}

// 在替换或复制的完整节点组中检查主键和稳定键各自唯一，防止不同节点被合并到同一记录或路径。
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

// 新建非空草稿和重建草稿共用：选入口前核对节点数量、连续顺序以及归属，避免从别的定义取首节点。
fn ordered_nodes_for_definition<'a>(
    nodes: &'a [ApprovalNodeDefinition],
    definition_id: &str,
) -> ModelResult<Vec<&'a ApprovalNodeDefinition>> {
    let ordered = ordered_nodes(nodes)?;
    ensure_nodes_belong_to_definition(nodes, definition_id)?;
    Ok(ordered)
}

// 构图时逐节点核对定义 ID，不让完整的展示顺序掩盖跨定义混入的节点。
fn ensure_nodes_belong_to_definition(
    nodes: &[ApprovalNodeDefinition],
    definition_id: &str,
) -> ModelResult<()> {
    if nodes.iter().all(|node| node.process_definition_id.as_ref() == definition_id) {
        return Ok(());
    }
    Err(ModelError::InvalidField("节点归属定义不一致"))
}

#[cfg(test)]
mod tests {
    use super::{
        CopiedNodeIdentity, DefinitionGraph, NewPopulatedDraftParams, NodeReplacementDraft,
        copy_nodes_for_definition,
    };
    use crate::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId, ApprovalTransitionDefinitionId};
    use crate::model::types::{ApprovalDecision, ApprovalTransitionEvent, ModelError};
    use crate::model::{
        ApprovalNodeDefinition, ApprovalParticipant, ApprovalProcessDefinition, ApprovalTransitionDefinition,
        NewNodeDefinition, ParticipantId, ProcessKind, Timestamp,
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
            assignee_participant_id: Some(ParticipantId::new(format!("u{order}")).unwrap()),
            assignee_label_snapshot: format!("人员{order}"),
            at: Timestamp::from_unix_secs(1).unwrap(),
        })
        .unwrap()
    }

    #[test]
    fn assignee_ids_include_all_present_participants_sorted_and_deduplicated() {
        let first = node("id1", "n1", 2, None)
            .with_additional_assignees(vec![
                ApprovalParticipant::new(ParticipantId::new("u3").unwrap(), "人员3").unwrap(),
                ApprovalParticipant::new(ParticipantId::new("u1").unwrap(), "人员1").unwrap(),
            ])
            .unwrap();
        let mut unresolved = node("id2", "n2", 3, None);
        unresolved.assignee_participant_id = None;
        let unresolved = unresolved.with_rule(Some("manager".into())).unwrap();
        let graph = DefinitionGraph {
            definition: definition("n3"),
            nodes: vec![first, unresolved, node("id3", "n3", 1, None)],
            transitions: Vec::new(),
        };
        assert_eq!(graph.assignee_ids(), ["u1", "u2", "u3"]);
    }

    // 验证整组替换既能沿用当前节点身份、接收新身份并清除用途，也拒绝空集合、重复引用和跨定义引用。
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
                    NodeReplacementDraft::new(
                        None,
                        ApprovalNodeDefinitionId::new("id2"),
                        "n2",
                        "财务",
                        2,
                        ParticipantId::new("u2").unwrap(),
                    )
                    .unwrap(),
                    NodeReplacementDraft::new(
                        Some(ApprovalNodeDefinitionId::new("id1")),
                        ApprovalNodeDefinitionId::new("ignored"),
                        "ignored",
                        "仓储",
                        1,
                        ParticipantId::new("u1").unwrap(),
                    )
                    .unwrap(),
                ],
                Timestamp::from_unix_secs(2).unwrap(),
            )
            .unwrap();
        assert_eq!(planned[0].base.id, "id1");
        assert_eq!(planned[0].node_key, "n1");
        assert!(planned[0].node_purpose.is_none());
        assert_eq!(planned[1].base.id, "id2");
        assert_eq!(planned[1].node_key, "n2");

        let duplicate = NodeReplacementDraft::new(
            Some(ApprovalNodeDefinitionId::new("id1")),
            ApprovalNodeDefinitionId::new("ignored"),
            "ignored",
            "仓储",
            1,
            ParticipantId::new("u1").unwrap(),
        )
        .unwrap();
        let mut second = duplicate.clone();
        second.display_order = 2;
        assert!(
            graph
                .plan_replacement_nodes(&[duplicate, second], Timestamp::from_unix_secs(2).unwrap(),)
                .is_err()
        );

        assert!(
            graph
                .plan_replacement_nodes(
                    &[NodeReplacementDraft::new(
                        Some(ApprovalNodeDefinitionId::new("foreign")),
                        ApprovalNodeDefinitionId::new("ignored"),
                        "ignored",
                        "仓储",
                        1,
                        ParticipantId::new("u1").unwrap(),
                    )
                    .unwrap()],
                    Timestamp::from_unix_secs(2).unwrap(),
                )
                .is_err()
        );
        assert!(graph.plan_replacement_nodes(&[], Timestamp::from_unix_secs(2).unwrap()).is_err());
    }

    // 覆盖空白身份、空名称和零顺序，并验证合法输入规范化后仍使用调用方给定的主键。
    #[test]
    fn replacement_draft_constructor_owns_node_invariants() {
        let assignee = ParticipantId::new("u1").unwrap();
        assert!(
            NodeReplacementDraft::new(
                None,
                ApprovalNodeDefinitionId::new("id2"),
                "n2",
                "  ",
                1,
                assignee.clone(),
            )
            .is_err()
        );
        assert!(
            NodeReplacementDraft::new(
                None,
                ApprovalNodeDefinitionId::new("id2"),
                "n2",
                "仓储",
                0,
                assignee.clone(),
            )
            .is_err()
        );
        assert!(
            NodeReplacementDraft::new(
                Some(ApprovalNodeDefinitionId::new("   ")),
                ApprovalNodeDefinitionId::new("id2"),
                "n2",
                "仓储",
                1,
                assignee.clone(),
            )
            .is_err()
        );
        assert!(
            NodeReplacementDraft::new(
                None,
                ApprovalNodeDefinitionId::new(""),
                "n2",
                "仓储",
                1,
                assignee.clone(),
            )
            .is_err()
        );
        assert!(
            NodeReplacementDraft::new(
                None,
                ApprovalNodeDefinitionId::new("id2"),
                "  ",
                "仓储",
                1,
                assignee.clone(),
            )
            .is_err()
        );

        let draft = NodeReplacementDraft::new(
            None,
            ApprovalNodeDefinitionId::new("id2"),
            " n2 ",
            " 仓储 ",
            1,
            assignee,
        )
        .unwrap();
        assert_eq!(draft.new_node_key, "n2");
        assert_eq!(draft.node_name, "仓储");
        assert_eq!(draft.new_node_id.as_ref(), "id2");
    }

    // 复制时置换身份、清除用途，再重建草稿并按新键解析路径，验证新图不依赖旧节点身份。
    #[test]
    fn copied_nodes_and_rebuilt_graph_are_deterministic() {
        let source = vec![node("old2", "old-n2", 2, None), node("old1", "old-n1", 1, Some("legacy"))];
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

        let graph = DefinitionGraph::new_populated_draft(NewPopulatedDraftParams {
            definition_id: new_definition_id,
            process_kind: ProcessKind::StockAdjustment,
            definition_version: 2,
            name: "复制草稿".into(),
            created_by: ParticipantId::new("admin").unwrap(),
            nodes: copied,
            transition_ids: (1..=4)
                .map(|index| ApprovalTransitionDefinitionId::new(format!("t{index}")))
                .collect(),
            at: Timestamp::from_unix_secs(2).unwrap(),
        })
        .unwrap();
        graph.validate_linear().unwrap();
        assert_eq!(graph.definition.entry_node_key, "new-n1");
        assert_eq!(
            graph.decision_target_node_key("new-n1", ApprovalDecision::Approve).unwrap().as_deref(),
            Some("new-n2")
        );
        assert_eq!(graph.decision_target_node_key("new-n2", ApprovalDecision::Approve).unwrap(), None);
        assert!(graph.decision_target_node_key("missing", ApprovalDecision::Approve).is_err());
    }

    // 为同一来源和事件放入重复边，验证解析决定时不会取第一条继续推进。
    #[test]
    fn duplicate_decision_edges_fail_closed() {
        let at = Timestamp::from_unix_secs(1).unwrap();
        let transition = |id: &str| {
            ApprovalTransitionDefinition::to_node(
                ApprovalTransitionDefinitionId::new(id),
                ApprovalProcessDefinitionId::new("def"),
                "n1",
                ApprovalTransitionEvent::Approve,
                "n2",
                at,
            )
            .unwrap()
        };
        let graph = DefinitionGraph {
            definition: definition("n1"),
            nodes: vec![node("id1", "n1", 1, None), node("id2", "n2", 2, None)],
            transitions: vec![transition("t1"), transition("t-dup")],
        };
        assert_eq!(
            graph.decision_target_node_key("n1", ApprovalDecision::Approve),
            Err(ModelError::InvalidTransition("审批定义存在重复决定连线"))
        );
    }

    // 分别检查未发布、已退役和结构损坏的已发布图，验证绑定不能只凭状态或图形状放行。
    #[test]
    fn published_linear_validation_rejects_draft_retired_and_corrupt_graphs() {
        let mut graph = DefinitionGraph::new_populated_draft(NewPopulatedDraftParams {
            definition_id: ApprovalProcessDefinitionId::new("def"),
            process_kind: ProcessKind::StockAdjustment,
            definition_version: 1,
            name: "库存调整".into(),
            created_by: ParticipantId::new("admin").unwrap(),
            nodes: vec![node("id1", "n1", 1, None)],
            transition_ids: (1..=2)
                .map(|index| ApprovalTransitionDefinitionId::new(format!("t{index}")))
                .collect(),
            at: Timestamp::from_unix_secs(1).unwrap(),
        })
        .unwrap();
        graph.validate_linear().unwrap();
        assert!(graph.validate_published_linear().is_err());

        graph
            .definition
            .publish(ParticipantId::new("admin").unwrap(), Timestamp::from_unix_secs(2).unwrap())
            .unwrap();
        graph.validate_published_linear().unwrap();

        let mut corrupt = graph.clone();
        corrupt.transitions.clear();
        assert!(corrupt.validate_published_linear().is_err());

        graph
            .definition
            .retire(ParticipantId::new("admin").unwrap(), Timestamp::from_unix_secs(3).unwrap())
            .unwrap();
        assert!(graph.validate_published_linear().is_err());
    }
}
