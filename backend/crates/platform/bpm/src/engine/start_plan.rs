//! 通用启动计划构造：定义版本、入口与逐节点资格校验。
//!
//! 本模块不读取账号、权限或仓储；调用方必须先按自身授权规则收敛每个节点的
//! [`Eligibility`] 并注入绑定主键，再交给 [`plan_start`] 校验与组装。

use std::collections::HashMap;

use super::{Eligibility, EngineError, EngineResult, StartAssigneeBinding};
use crate::graph::DefinitionGraph;
use crate::ids::ApprovalInstanceAssigneeId;
use crate::model::ApprovalNodeDefinition;

/// 单个节点的启动绑定输入：调用方已收敛授权资格并注入绑定主键。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartBindingInput {
    /// 定义内稳定节点键。
    pub node_key: String,
    /// 绑定主键。
    pub assignee_id: ApprovalInstanceAssigneeId,
    /// 调用方已收敛的资格；对象读取失败时收敛为 BLOCKED 资格。
    pub eligibility: Eligibility,
}

/// 通用启动计划输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartPlanInput<'a> {
    /// 定义图。
    pub graph: &'a DefinitionGraph,
    /// 调用方冻结的定义版本；与图漂移时失败关闭。
    pub expected_definition_version: u32,
    /// 与定义节点一一对应的绑定输入。
    pub bindings: Vec<StartBindingInput>,
}

/// 通用启动计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartPlan {
    /// 与定义节点一一对应的启动绑定。
    pub bindings: Vec<StartAssigneeBinding>,
    /// 入口节点键。
    pub entry_node_key: String,
    /// 入口资格。
    pub entry_eligibility: Eligibility,
}

/// 调用方收集逐节点资格后，按定义顺序将绑定与冻结版本对齐，得到可交给 [`start`](fn@super::start) 的绑定以及入口键和入口资格。
/// 允许读取失败收敛出的 `Blocked` 资格随计划传递，是否拒绝启动由引擎统一判断。调用方仍须用命令收据和事务 CAS 防止并发启动多个实例。
///
/// # 错误
/// 版本漂移、空图、绑定缺失或重复、绑定主键重复、人员资格归属不匹配或入口缺失时返回命令错误。
pub fn plan_start(input: StartPlanInput<'_>) -> EngineResult<StartPlan> {
    let StartPlanInput { graph, expected_definition_version, bindings } = input;
    if graph.definition.definition_version != expected_definition_version {
        return Err(EngineError::InvalidCommand("定义版本与冻结绑定不一致"));
    }
    if graph.nodes.is_empty() {
        return Err(EngineError::InvalidCommand("审批定义没有节点，无法启动"));
    }
    if bindings.len() != graph.nodes.len() {
        return Err(EngineError::InvalidCommand("启动绑定必须与定义节点一一对应"));
    }

    let mut by_node = HashMap::with_capacity(bindings.len());
    for binding in bindings {
        if by_node.insert(binding.node_key, (binding.assignee_id, binding.eligibility)).is_some() {
            return Err(EngineError::InvalidCommand("节点审批人绑定缺失或重复"));
        }
    }

    let mut frozen = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        frozen.push(freeze_binding(node, &mut by_node, &frozen)?);
    }

    let entry = graph.entry_node().map_err(|_| EngineError::InvalidCommand("审批定义缺少入口节点"))?;
    let entry_eligibility = frozen
        .iter()
        .find(|item| item.node_key == entry.node_key)
        .map(|item| item.eligibility.clone())
        .ok_or(EngineError::InvalidCommand("入口节点缺少审批人绑定"))?;
    Ok(StartPlan { bindings: frozen, entry_node_key: entry.node_key.clone(), entry_eligibility })
}

// 按定义节点取走一个待匹配的绑定，先核对资格归属再检查主键唯一性；遇到重复定义节点时仍按这一顺序报错。
fn freeze_binding(
    node: &ApprovalNodeDefinition,
    by_node: &mut HashMap<String, (ApprovalInstanceAssigneeId, Eligibility)>,
    frozen: &[StartAssigneeBinding],
) -> EngineResult<StartAssigneeBinding> {
    let Some((node_key, (assignee_id, eligibility))) = by_node.remove_entry(node.node_key.as_str()) else {
        let prior = frozen
            .iter()
            .find(|prior| prior.node_key == node.node_key)
            .ok_or(EngineError::InvalidCommand("节点审批人绑定缺失或重复"))?;
        if prior.eligibility.participant_ref() != node.assignee()? {
            return Err(EngineError::InvalidCommand("资格结果必须属于定义审批人"));
        }
        return Err(EngineError::InvalidCommand("实例审批人绑定主键不得重复"));
    };

    if eligibility.participant_ref() != node.assignee()? {
        return Err(EngineError::InvalidCommand("资格结果必须属于定义审批人"));
    }
    eligibility.validate_node(node)?;

    if frozen.iter().any(|prior| prior.id == assignee_id) {
        return Err(EngineError::InvalidCommand("实例审批人绑定主键不得重复"));
    }
    Ok(StartAssigneeBinding { id: assignee_id, node_key, participant: node.assignee()?.clone(), eligibility })
}

#[cfg(test)]
mod tests {
    use super::super::test_fixtures::{at, blocked, eligible, node, participant};
    use super::{StartBindingInput, StartPlanInput, plan_start};
    use crate::engine::{Eligibility, EngineError};
    use crate::graph::DefinitionGraph;
    use crate::ids::{
        ApprovalInstanceAssigneeId, ApprovalProcessDefinitionId, ApprovalTransitionDefinitionId,
    };
    use crate::model::types::{ApprovalBlockerCode, ApprovalTransitionEvent};
    use crate::model::{ApprovalProcessDefinition, ApprovalTransitionDefinition, ProcessKind};

    /// 构造单入口双节点线性定义图。
    fn two_node_graph() -> DefinitionGraph {
        let at = at(1);
        let definition = ApprovalProcessDefinition::new_draft(
            ApprovalProcessDefinitionId::new("def"),
            ProcessKind::PurchaseOrder,
            1,
            "采购审批",
            "n1",
            participant("admin"),
            at,
        )
        .unwrap();
        DefinitionGraph {
            definition,
            nodes: vec![
                node("nd1", "n1", "采购确认", 1, "u1", "张三", at),
                node("nd2", "n2", "财务复核", 2, "u2", "李四", at),
            ],
            transitions: vec![
                ApprovalTransitionDefinition::to_node(
                    ApprovalTransitionDefinitionId::new("t1"),
                    ApprovalProcessDefinitionId::new("def"),
                    "n1",
                    ApprovalTransitionEvent::Approve,
                    "n2",
                    at,
                )
                .unwrap(),
                ApprovalTransitionDefinition::to_approved(
                    ApprovalTransitionDefinitionId::new("t2"),
                    ApprovalProcessDefinitionId::new("def"),
                    "n2",
                    ApprovalTransitionEvent::Approve,
                    at,
                )
                .unwrap(),
            ],
        }
    }

    fn binding(node_key: &str, id: &str, eligibility: Eligibility) -> StartBindingInput {
        StartBindingInput {
            node_key: node_key.into(),
            assignee_id: ApprovalInstanceAssigneeId::new(id),
            eligibility,
        }
    }

    // 验证准备计划不只绑定入口，还为后继节点保留一一对应的身份和资格。
    #[test]
    fn start_plan_freezes_all_nodes_and_entry_eligibility() {
        let graph = two_node_graph();
        let plan = plan_start(StartPlanInput {
            graph: &graph,
            expected_definition_version: 1,
            bindings: vec![
                binding("n1", "a1", eligible("u1", "张三")),
                binding("n2", "a2", eligible("u2", "李四")),
            ],
        })
        .unwrap();
        assert_eq!(plan.bindings.len(), 2);
        assert_eq!(plan.entry_node_key, "n1");
        assert_eq!(plan.entry_eligibility.participant(), participant("u1"));
        assert_eq!(plan.bindings[0].node_key, "n1");
        assert_eq!(plan.bindings[0].participant, participant("u1"));
        assert_eq!(plan.bindings[1].node_key, "n2");
        assert_eq!(plan.bindings[1].participant, participant("u2"));
    }

    // 打乱调用方绑定顺序，验证匹配以节点键为准，输出仍跟随定义节点顺序。
    #[test]
    fn start_plan_matches_bindings_by_node_key_regardless_of_order() {
        let graph = two_node_graph();
        let plan = plan_start(StartPlanInput {
            graph: &graph,
            expected_definition_version: 1,
            bindings: vec![
                binding("n2", "a2", eligible("u2", "李四")),
                binding("n1", "a1", eligible("u1", "张三")),
            ],
        })
        .unwrap();
        assert_eq!(plan.bindings[0].node_key, "n1");
        assert_eq!(plan.bindings[0].id, ApprovalInstanceAssigneeId::new("a1"));
        assert_eq!(plan.bindings[1].node_key, "n2");
    }

    // 用旧冻结版本搭配当前图，验证计划不会跨版本复用审批人绑定。
    #[test]
    fn start_plan_rejects_definition_version_drift() {
        let graph = two_node_graph();
        let error = plan_start(StartPlanInput {
            graph: &graph,
            expected_definition_version: 2,
            bindings: vec![
                binding("n1", "a1", eligible("u1", "张三")),
                binding("n2", "a2", eligible("u2", "李四")),
            ],
        })
        .unwrap_err();
        assert_eq!(error, EngineError::InvalidCommand("定义版本与冻结绑定不一致"));
    }

    // 验证无节点的定义即使接收空绑定集也不能规划启动。
    #[test]
    fn start_plan_rejects_empty_node_set() {
        let at = at(1);
        let graph = DefinitionGraph {
            definition: ApprovalProcessDefinition::new_draft(
                ApprovalProcessDefinitionId::new("def"),
                ProcessKind::PurchaseOrder,
                1,
                "采购审批",
                "n1",
                participant("admin"),
                at,
            )
            .unwrap(),
            nodes: vec![],
            transitions: vec![],
        };

        let error =
            plan_start(StartPlanInput { graph: &graph, expected_definition_version: 1, bindings: vec![] })
                .unwrap_err();
        assert_eq!(error, EngineError::InvalidCommand("审批定义没有节点，无法启动"));
    }

    // 验证输入总量看似正确也不能掩盖一个节点重复绑定、另一个节点缺绑定的问题。
    #[test]
    fn start_plan_rejects_missing_or_duplicate_binding() {
        let graph = two_node_graph();
        let missing = plan_start(StartPlanInput {
            graph: &graph,
            expected_definition_version: 1,
            bindings: vec![
                binding("n1", "a1", eligible("u1", "张三")),
                binding("n3", "a3", eligible("u3", "王五")),
            ],
        })
        .unwrap_err();
        assert_eq!(missing, EngineError::InvalidCommand("节点审批人绑定缺失或重复"));

        let duplicated = plan_start(StartPlanInput {
            graph: &graph,
            expected_definition_version: 1,
            bindings: vec![
                binding("n1", "a1", eligible("u1", "张三")),
                binding("n1", "a3", eligible("u1", "张三")),
            ],
        })
        .unwrap_err();
        assert_eq!(duplicated, EngineError::InvalidCommand("节点审批人绑定缺失或重复"));
    }

    // 绑定条数少于节点数时立即拒绝，避免生成不完整的实例绑定。
    #[test]
    fn start_plan_rejects_binding_count_mismatch() {
        let graph = two_node_graph();
        let error = plan_start(StartPlanInput {
            graph: &graph,
            expected_definition_version: 1,
            bindings: vec![binding("n1", "a1", eligible("u1", "张三"))],
        })
        .unwrap_err();
        assert_eq!(error, EngineError::InvalidCommand("启动绑定必须与定义节点一一对应"));
    }

    // 传入与定义责任人不同的资格身份，验证准备计划不接受替代审批人。
    #[test]
    fn start_plan_rejects_eligibility_for_another_participant() {
        let graph = two_node_graph();
        let error = plan_start(StartPlanInput {
            graph: &graph,
            expected_definition_version: 1,
            bindings: vec![
                binding("n1", "a1", eligible("u9", "钱七")),
                binding("n2", "a2", eligible("u2", "李四")),
            ],
        })
        .unwrap_err();
        assert_eq!(error, EngineError::InvalidCommand("资格结果必须属于定义审批人"));
    }

    // 为不同节点复用绑定主键，验证计划会拒绝无法独立保存的快照。
    #[test]
    fn start_plan_rejects_duplicate_assignee_ids() {
        let graph = two_node_graph();
        let error = plan_start(StartPlanInput {
            graph: &graph,
            expected_definition_version: 1,
            bindings: vec![
                binding("n1", "a1", eligible("u1", "张三")),
                binding("n2", "a1", eligible("u2", "李四")),
            ],
        })
        .unwrap_err();
        assert_eq!(error, EngineError::InvalidCommand("实例审批人绑定主键不得重复"));
    }

    // 构造重复定义节点，验证它的资格归属错误不会被重复主键错误掩盖。
    #[test]
    fn start_plan_preserves_validation_order_for_duplicate_definition_nodes() {
        let mut graph = two_node_graph();
        graph.nodes[1] = graph.nodes[0].clone();
        for changed_participant in [false, true] {
            if changed_participant {
                graph.nodes[1].assignee_participant_id = Some(participant("u9"));
            }
            let error = plan_start(StartPlanInput {
                graph: &graph,
                expected_definition_version: 1,
                bindings: vec![
                    binding("n1", "a1", eligible("u1", "张三")),
                    binding("n2", "a2", eligible("u2", "李四")),
                ],
            })
            .unwrap_err();
            let expected = if changed_participant {
                "资格结果必须属于定义审批人"
            } else {
                "实例审批人绑定主键不得重复"
            };
            assert_eq!(error, EngineError::InvalidCommand(expected));
        }
    }

    // 验证逐节点绑定齐全仍不足以启动：定义入口必须实际命中节点。
    #[test]
    fn start_plan_rejects_missing_entry_node() {
        let at = at(1);
        let graph = DefinitionGraph {
            definition: ApprovalProcessDefinition::new_draft(
                ApprovalProcessDefinitionId::new("def"),
                ProcessKind::PurchaseOrder,
                1,
                "采购审批",
                "missing",
                participant("admin"),
                at,
            )
            .unwrap(),
            nodes: vec![node("nd1", "n1", "采购确认", 1, "u1", "张三", at)],
            transitions: vec![],
        };

        let error = plan_start(StartPlanInput {
            graph: &graph,
            expected_definition_version: 1,
            bindings: vec![binding("n1", "a1", eligible("u1", "张三"))],
        })
        .unwrap_err();
        assert_eq!(error, EngineError::InvalidCommand("审批定义缺少入口节点"));
    }

    // 验证读取失败收敛的阻塞资格原样传递，是否允许启动由后续引擎判断。
    #[test]
    fn start_plan_carries_blocked_eligibility_for_read_failure() {
        let graph = two_node_graph();
        let plan = plan_start(StartPlanInput {
            graph: &graph,
            expected_definition_version: 1,
            bindings: vec![
                binding("n1", "a1", blocked("u1", "张三", ApprovalBlockerCode::ApproverCannotReadSubject)),
                binding("n2", "a2", eligible("u2", "李四")),
            ],
        })
        .unwrap();
        assert_eq!(
            plan.bindings[0].eligibility.blocked_code(),
            Some(ApprovalBlockerCode::ApproverCannotReadSubject)
        );
        assert_eq!(
            plan.entry_eligibility.blocked_code(),
            Some(ApprovalBlockerCode::ApproverCannotReadSubject)
        );
        assert_eq!(plan.bindings[1].eligibility, eligible("u2", "李四"));
    }

    // 把计划生成的绑定交给启动入口，验证准备阶段与进入引擎使用同一份人员事实。
    #[test]
    fn start_plan_bindings_feed_engine_start() {
        let graph = two_node_graph();
        let plan = plan_start(StartPlanInput {
            graph: &graph,
            expected_definition_version: 1,
            bindings: vec![
                binding("n1", "a1", eligible("u1", "张三")),
                binding("n2", "a2", eligible("u2", "李四")),
            ],
        })
        .unwrap();

        let started = crate::engine::start(
            crate::engine::StartCommand {
                instance_id: crate::ids::ApprovalProcessInstanceId::new("inst"),
                process_kind: ProcessKind::PurchaseOrder,
                subject: crate::model::SubjectRef::new("purchase_order", "po-1").unwrap(),
                subject_version: 1,
                started_by: participant("starter"),
                entry_execution_id: crate::ids::ApprovalNodeExecutionId::new("e1"),
                now: at(10),
            },
            &graph,
            &plan.bindings,
        )
        .unwrap();
        assert_eq!(started.created_assignees.len(), 2);
        assert_eq!(started.created_executions[0].node_key, "n1");
    }
}
