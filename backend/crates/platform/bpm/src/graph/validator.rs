//! 审批定义图的节点顺序、入口、连线形状与完整线性模型校验。

use super::{LinearTransitionDraft, generate_linear_transitions};
use crate::model::types::{ApprovalTransitionEvent, ModelError, ModelResult, NODE_KEY_MAX_LEN};
use crate::model::{ApprovalNodeDefinition, ApprovalProcessDefinition, ApprovalTransitionDefinition};

/// 单独读取或校验连线时检查自身目标形状，不判断来源是否为入口或末节点。
/// 不合法的形状返回 [`ModelError::InvalidTransition`]；节点在图中的位置由整图校验负责。
pub fn validate_transition(transition: &ApprovalTransitionDefinition) -> ModelResult<()> {
    transition.validate_shape()
}

/// 整图校验前确认入口键裁剪后非空、未超字节上限，并能在提供的节点键集合中找到。
/// 这里只检查键可用且存在，是否排第一由线性图校验决定。
pub fn validate_entry_node(entry_node_key: &str, node_keys: &[String]) -> ModelResult<()> {
    let entry = entry_node_key.trim();
    if entry.is_empty() {
        return Err(ModelError::InvalidField("入口节点键不能为空"));
    }
    if entry.len() > NODE_KEY_MAX_LEN {
        return Err(ModelError::InvalidField("入口节点键过长"));
    }

    if node_keys.iter().any(|key| key.trim() == entry) {
        return Ok(());
    }
    Err(ModelError::InvalidField("入口节点必须存在于节点集合"))
}

/// 构图和读取定义时按展示顺序统一排列节点；数量须为 1 至 [`super::MAX_DEFINITION_NODES`]，顺序从 1 连续且无重复，不能依赖持久化顺序。
pub fn ordered_nodes(nodes: &[ApprovalNodeDefinition]) -> ModelResult<Vec<&ApprovalNodeDefinition>> {
    super::ordered_by_display_order(nodes, |node| node.display_order)
}

/// 发布、绑定或读取冻结图时校验逐节点指派、入口和连线。入口必须排第一，通过按顺序推进，末节点通过和任一驳回都收结实例。
/// 允许读取指回入口的历史冻结驳回边，但只按终态理解；其它边、目标或数量与生成结果不一致时返回错误。
pub fn validate_linear_graph(
    definition: &ApprovalProcessDefinition,
    nodes: &[ApprovalNodeDefinition],
    transitions: &[ApprovalTransitionDefinition],
) -> ModelResult<()> {
    for node in nodes {
        node.validate_assignment()?;
        node.validate_additional_assignees()?;
    }

    let ordered = ordered_nodes(nodes)?;
    let keys = ordered.into_iter().map(|node| node.node_key.clone()).collect::<Vec<_>>();
    validate_entry_node(&definition.entry_node_key, &keys)?;
    if definition.entry_node_key.trim() != keys[0] {
        return Err(ModelError::InvalidField("入口必须是顺序第一节点"));
    }

    let expected = generate_linear_transitions(&keys)?;
    ensure_transitions_match(transitions, &expected, &definition.entry_node_key)?;
    for transition in transitions {
        validate_transition(transition)?;
    }
    Ok(())
}

// 整图校验时忽略存储顺序，对比实际和生成的来源、事件、节点目标与终态；数量或内容不同即拒绝。
fn ensure_transitions_match(
    actual: &[ApprovalTransitionDefinition],
    expected: &[LinearTransitionDraft],
    entry: &str,
) -> ModelResult<()> {
    if actual.len() != expected.len() {
        return Err(ModelError::InvalidTransition("连线与线性生成器结果不一致"));
    }

    let mut actual_keys =
        actual.iter().map(|transition| transition_key(transition, entry)).collect::<Vec<_>>();
    let mut expected_keys = expected
        .iter()
        .map(|draft| {
            (
                draft.from_node_key.as_str(),
                draft.event.as_str(),
                draft.to_node_key.as_deref(),
                draft.terminal_result.map(|result| result.as_str()),
            )
        })
        .collect::<Vec<_>>();

    actual_keys.sort();
    expected_keys.sort();
    if actual_keys == expected_keys {
        return Ok(());
    }
    Err(ModelError::InvalidTransition("连线与线性生成器结果不一致"))
}

// 为连线对比构造稳定元组；历史驳回回入口边映射为驳回终态，只用于比较，不改写冻结记录。
fn transition_key<'a>(
    transition: &'a ApprovalTransitionDefinition,
    entry: &str,
) -> (&'a str, &'static str, Option<&'a str>, Option<&'static str>) {
    if transition.event == ApprovalTransitionEvent::Reject
        && transition.to_node_key.as_deref() == Some(entry)
        && transition.terminal_result.is_none()
    {
        return (transition.from_node_key.as_str(), "REJECT", None, Some("REJECTED"));
    }
    (
        transition.from_node_key.as_str(),
        transition.event.as_str(),
        transition.to_node_key.as_deref(),
        transition.terminal_result.map(|result| result.as_str()),
    )
}

#[cfg(test)]
mod tests {
    use super::{ordered_nodes, validate_entry_node, validate_linear_graph, validate_transition};
    use crate::graph::{DefinitionGraph, build_linear_transitions};
    use crate::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId, ApprovalTransitionDefinitionId};
    use crate::model::types::{ApprovalTransitionEvent, ModelError};
    use crate::model::{
        ApprovalNodeDefinition, ApprovalParticipant, ApprovalProcessDefinition, ApprovalTransitionDefinition,
        NewNodeDefinition, ParticipantId, ProcessKind, Timestamp,
    };

    /// 构造最小合法节点。
    fn node(id: &str, key: &str, order: u32) -> ApprovalNodeDefinition {
        ApprovalNodeDefinition::new(NewNodeDefinition {
            id: ApprovalNodeDefinitionId::new(id),
            process_definition_id: ApprovalProcessDefinitionId::new("def"),
            node_key: key.to_string(),
            node_name: format!("节点{order}"),
            node_purpose: None,
            display_order: order,
            assignee_participant_id: Some(ParticipantId::new(format!("u{order}")).unwrap()),
            assignee_label_snapshot: format!("人员{order}"),
            at: Timestamp::from_unix_secs(1).unwrap(),
        })
        .unwrap()
    }

    // 覆盖合法单边及入口键存在、缺失和空白的情况，区分连线形状和入口校验。
    #[test]
    fn validates_transition_and_entry() {
        let transition = ApprovalTransitionDefinition::to_node(
            ApprovalTransitionDefinitionId::new("t1"),
            ApprovalProcessDefinitionId::new("def"),
            "n1",
            ApprovalTransitionEvent::Approve,
            "n2",
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap();
        assert!(validate_transition(&transition).is_ok());
        assert!(validate_entry_node("n1", &["n1".into(), "n2".into()]).is_ok());
        assert!(validate_entry_node("n3", &["n1".into(), "n2".into()]).is_err());
        assert!(validate_entry_node("  ", &["n1".into()]).is_err());
    }

    // 从乱序节点取得确定的首节点，并验证空集合、超出数量上限或未从 1 起步时无法排序构图。
    #[test]
    fn ordered_nodes_enforce_count_and_continuity() {
        let second = node("id2", "n2", 2);
        let first = node("id1", "n1", 1);
        let nodes = [second, first];
        let ordered = ordered_nodes(&nodes).unwrap();
        assert_eq!(ordered[0].node_key, "n1");
        assert!(ordered_nodes(&[]).is_err());
        assert!(ordered_nodes(&[node("id1", "n1", 2)]).is_err());
        let too_many = (1..=21)
            .map(|index| node(&format!("id{index}"), &format!("n{index}"), index))
            .collect::<Vec<_>>();
        assert!(ordered_nodes(&too_many).is_err());
    }

    #[test]
    fn ordered_nodes_reject_duplicates_gaps_and_large_orders() {
        for second_order in [1, 3, u32::MAX] {
            let nodes = [node("id1", "n1", 1), node("id2", "n2", second_order)];
            assert_eq!(ordered_nodes(&nodes), Err(ModelError::InvalidField("节点顺序必须从 1 连续且无重复")));
        }

        let nodes = (1..=20)
            .rev()
            .map(|order| node(&format!("id{order}"), &format!("n{order}"), order))
            .collect::<Vec<_>>();
        let orders =
            ordered_nodes(&nodes).unwrap().into_iter().map(|node| node.display_order).collect::<Vec<_>>();
        assert_eq!(orders, (1..=20).collect::<Vec<_>>());
    }

    // 覆盖入口、连线和参与名单损坏，同时验证旧驳回边只在指回入口时兼容，且解析不会改写冻结图。
    #[test]
    fn linear_graph_validation_fails_closed() {
        let nodes = vec![node("id1", "n1", 1), node("id2", "n2", 2)];
        let definition = ApprovalProcessDefinition::new_draft(
            ApprovalProcessDefinitionId::new("def"),
            ProcessKind::StockAdjustment,
            1,
            "库存调整",
            "n1",
            ParticipantId::new("admin").unwrap(),
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap();
        let transitions = build_linear_transitions(
            &ApprovalProcessDefinitionId::new("def"),
            &nodes,
            (1..=4).map(|index| ApprovalTransitionDefinitionId::new(format!("t{index}"))).collect(),
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap();
        validate_linear_graph(&definition, &nodes, &transitions).unwrap();

        let mut legacy = transitions.clone();
        for transition in legacy.iter_mut().filter(|edge| edge.event == ApprovalTransitionEvent::Reject) {
            transition.to_node_key = Some("n1".into());
            transition.terminal_result = None;
        }
        validate_linear_graph(&definition, &nodes, &legacy).unwrap();
        let frozen = legacy.clone();
        let legacy_graph = DefinitionGraph {
            definition: definition.clone(),
            nodes: nodes.clone(),
            transitions: legacy.clone(),
        };
        assert_eq!(
            legacy_graph
                .decision_target_node_key("n2", crate::model::types::ApprovalDecision::Reject)
                .unwrap(),
            None
        );
        assert_eq!(legacy_graph.transitions, frozen);

        legacy[1].to_node_key = Some("n2".into());
        assert!(validate_linear_graph(&definition, &nodes, &legacy).is_err());

        let mut wrong_entry = definition.clone();
        wrong_entry.entry_node_key = "n2".to_string();
        assert!(validate_linear_graph(&wrong_entry, &nodes, &transitions).is_err());
        assert!(validate_linear_graph(&definition, &nodes, &transitions[..3]).is_err());

        let mut graph = DefinitionGraph { definition, nodes, transitions };
        graph.validate_linear().unwrap();
        let secondary = ApprovalParticipant::new(ParticipantId::new("secondary").unwrap(), "会签人").unwrap();
        graph.nodes[0].additional_assignees = vec![secondary.clone()];
        graph.validate_linear().unwrap();
        graph.nodes[0].additional_assignees.push(secondary);
        assert_eq!(graph.validate_linear(), Err(ModelError::InvalidField("同一节点审批人不能重复")));
    }
}
