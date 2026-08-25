//! 审批定义图的节点顺序、入口、连线形状与完整线性模型校验。

use crate::model::types::{ModelError, ModelResult, NODE_KEY_MAX_LEN};
use crate::model::{ApprovalNodeDefinition, ApprovalProcessDefinition, ApprovalTransitionDefinition};

use super::{generate_linear_transitions, LinearTransitionDraft, MAX_DEFINITION_NODES};

/// 校验单条连线实体形状。
///
/// 本函数不把来源节点解释为入口或末节点。
///
/// # 参数
/// * `transition` - 已构造的连线
///
/// # 返回
/// 连线形状合法时返回 `Ok(())`。
///
/// # 错误
/// 形状不合法时返回 [`ModelError::InvalidTransition`]。
///
/// # 关键业务约束
/// 目标节点与终态结果必须恰有一个。
pub fn validate_transition(transition: &ApprovalTransitionDefinition) -> ModelResult<()> {
    transition.validate_shape()
}

/// 校验入口键存在于已给出的节点键集合。
///
/// # 参数
/// * `entry_node_key` - 定义入口
/// * `node_keys` - 定义内节点键
///
/// # 返回
/// 入口规范且存在于节点集合时返回 `Ok(())`。
///
/// # 错误
/// 入口为空、超长或不在集合内时返回错误。
///
/// # 关键业务约束
/// 本方法只验证入口存在性，不推断展示顺序。
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

/// 按展示顺序排列节点并校验数量与连续性。
///
/// # 参数
/// * `nodes` - 未保证存储顺序的完整节点集合
///
/// # 返回
/// 返回从 `display_order = 1` 开始连续排列的节点引用。
///
/// # 错误
/// 节点数量不在 `1..=20`、顺序溢出或不连续时返回模型错误。
///
/// # 关键业务约束
/// 存储返回顺序不参与流程语义，展示顺序是唯一权威顺序。
pub fn ordered_nodes(nodes: &[ApprovalNodeDefinition]) -> ModelResult<Vec<&ApprovalNodeDefinition>> {
    if !(1..=MAX_DEFINITION_NODES).contains(&nodes.len()) {
        return Err(ModelError::InvalidField("审批节点数量必须在 1 到 20 之间"));
    }
    let mut ordered = nodes.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|node| node.display_order);
    for (index, node) in ordered.iter().enumerate() {
        let expected = u32::try_from(index + 1).map_err(|_| ModelError::InvalidField("节点顺序溢出"))?;
        if node.display_order != expected {
            return Err(ModelError::InvalidField("节点顺序必须从 1 连续且无重复"));
        }
    }
    Ok(ordered)
}

/// 校验完整定义图与线性生成器完全一致。
///
/// # 参数
/// * `definition` - 流程定义
/// * `nodes` - 定义节点
/// * `transitions` - 已持久化连线
///
/// # 返回
/// 节点、入口和连线构成合法线性审批图时返回 `Ok(())`。
///
/// # 错误
/// 节点数量、顺序、入口、连线数量、形状或目标不一致时返回模型错误。
///
/// # 关键业务约束
/// 入口必须是顺序第一节点，全部驳回回入口，末节点通过进入已批准终态。
pub fn validate_linear_graph(
    definition: &ApprovalProcessDefinition,
    nodes: &[ApprovalNodeDefinition],
    transitions: &[ApprovalTransitionDefinition],
) -> ModelResult<()> {
    let ordered = ordered_nodes(nodes)?;
    let keys = ordered
        .into_iter()
        .map(|node| node.node_key.clone())
        .collect::<Vec<_>>();
    validate_entry_node(&definition.entry_node_key, &keys)?;
    if definition.entry_node_key.trim() != keys[0] {
        return Err(ModelError::InvalidField("入口必须是顺序第一节点"));
    }
    let expected = generate_linear_transitions(&keys)?;
    ensure_transitions_match(transitions, &expected)?;
    for transition in transitions {
        validate_transition(transition)?;
    }
    Ok(())
}

/// 比较已持久化连线与线性生成器输出。
///
/// # 参数
/// * `actual` - 已持久化连线
/// * `expected` - 线性生成器输出
///
/// # 返回
/// 忽略存储顺序后形态完全一致时返回 `Ok(())`。
///
/// # 错误
/// 数量或任一来源、事件、目标、终态不一致时返回模型错误。
fn ensure_transitions_match(
    actual: &[ApprovalTransitionDefinition],
    expected: &[LinearTransitionDraft],
) -> ModelResult<()> {
    if actual.len() != expected.len() {
        return Err(ModelError::InvalidTransition("连线与线性生成器结果不一致"));
    }
    let mut actual_keys = actual.iter().map(transition_key).collect::<Vec<_>>();
    let mut expected_keys = expected
        .iter()
        .map(|draft| {
            (
                draft.from_node_key.clone(),
                draft.event.as_str(),
                draft.to_node_key.clone(),
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

/// 构造连线比较使用的稳定键。
///
/// # 参数
/// * `transition` - 已持久化连线
///
/// # 返回
/// 返回来源、事件、目标与终态组成的比较元组。
///
/// # 错误
/// 无。
fn transition_key(
    transition: &ApprovalTransitionDefinition,
) -> (String, &'static str, Option<String>, Option<&'static str>) {
    (
        transition.from_node_key.clone(),
        transition.event.as_str(),
        transition.to_node_key.clone(),
        transition.terminal_result.map(|result| result.as_str()),
    )
}

#[cfg(test)]
mod tests {
    use super::{ordered_nodes, validate_entry_node, validate_linear_graph, validate_transition};
    use crate::graph::{build_linear_transitions, DefinitionGraph};
    use crate::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId, ApprovalTransitionDefinitionId};
    use crate::model::types::ApprovalTransitionEvent;
    use crate::model::{
        ApprovalNodeDefinition, ApprovalProcessDefinition, ApprovalTransitionDefinition, NewNodeDefinition,
        ParticipantId, ProcessKind, Timestamp,
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
            assignee_participant_id: ParticipantId::new(format!("u{order}")).unwrap(),
            assignee_label_snapshot: format!("人员{order}"),
            at: Timestamp::from_unix_secs(1).unwrap(),
        })
        .unwrap()
    }

    /// 合法连线通过；入口必须命中节点集合。
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

    /// 节点顺序确定化，并拒绝空、超限与不连续顺序。
    ///
    /// 测试集合必须在排序引用存续期间保持所有权。
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

    /// 完整线性图必须入口正确且连线与生成器一致。
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
            (1..=4)
                .map(|index| ApprovalTransitionDefinitionId::new(format!("t{index}")))
                .collect(),
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap();
        validate_linear_graph(&definition, &nodes, &transitions).unwrap();

        let mut wrong_entry = definition.clone();
        wrong_entry.entry_node_key = "n2".to_string();
        assert!(validate_linear_graph(&wrong_entry, &nodes, &transitions).is_err());
        assert!(validate_linear_graph(&definition, &nodes, &transitions[..3]).is_err());

        let graph = DefinitionGraph {
            definition,
            nodes,
            transitions,
        };
        graph.validate_linear().unwrap();
    }
}
