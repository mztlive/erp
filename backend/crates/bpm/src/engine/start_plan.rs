//! 通用启动计划构造：定义版本、入口与逐节点资格校验。
//!
//! 本模块不读取账号、权限或仓储；调用方必须先按自身授权规则收敛每个节点的
//! [`Eligibility`] 并注入绑定主键，再交给 [`plan_start`] 校验与组装。

use crate::graph::DefinitionGraph;
use crate::ids::ApprovalInstanceAssigneeId;

use super::{Eligibility, EngineError, EngineResult, StartAssigneeBinding};

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

/// 构造通用启动计划：定义版本漂移、空节点、办理人非法与入口缺失均失败关闭。
///
/// 采购单、采购变更单及后续单据类型复用本规则，禁止在各业务 Service 复制。
///
/// # 参数
/// * `input` - 定义图、冻结定义版本与逐节点资格
///
/// # 返回
/// 返回与定义节点一一对应的启动绑定、入口节点键与入口资格。
///
/// # 错误
/// 定义版本与冻结绑定不一致、定义没有节点、绑定缺失或重复、资格不属于定义
/// 审批人、绑定主键重复或入口节点缺失时返回命令无效错误。
///
/// # 关键业务约束
/// 本计划接受 BLOCKED 资格（读取失败收敛结果），是否允许启动由引擎 `start`
/// 统一拒绝；启动并发单实例继续由调用方命令收据与事务 CAS 保证。
pub fn plan_start(input: StartPlanInput<'_>) -> EngineResult<StartPlan> {
    let StartPlanInput {
        graph,
        expected_definition_version,
        bindings,
    } = input;
    if graph.definition.definition_version != expected_definition_version {
        return Err(EngineError::InvalidCommand("定义版本与冻结绑定不一致"));
    }
    if graph.nodes.is_empty() {
        return Err(EngineError::InvalidCommand("审批定义没有节点，无法启动"));
    }
    if bindings.len() != graph.nodes.len() {
        return Err(EngineError::InvalidCommand("启动绑定必须与定义节点一一对应"));
    }
    let mut frozen = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let binding = binding_for(&bindings, &node.node_key)?;
        if binding.eligibility.participant() != node.assignee_participant_id {
            return Err(EngineError::InvalidCommand("资格结果必须属于定义审批人"));
        }
        if frozen
            .iter()
            .any(|prior: &StartAssigneeBinding| prior.id == binding.assignee_id)
        {
            return Err(EngineError::InvalidCommand("实例审批人绑定主键不得重复"));
        }
        frozen.push(StartAssigneeBinding {
            id: binding.assignee_id.clone(),
            node_key: node.node_key.clone(),
            participant: node.assignee_participant_id.clone(),
            eligibility: binding.eligibility.clone(),
        });
    }
    let entry = graph
        .entry_node()
        .map_err(|_| EngineError::InvalidCommand("审批定义缺少入口节点"))?;
    let entry_eligibility = frozen
        .iter()
        .find(|item| item.node_key == entry.node_key)
        .map(|item| item.eligibility.clone())
        .ok_or(EngineError::InvalidCommand("入口节点缺少审批人绑定"))?;
    Ok(StartPlan {
        bindings: frozen,
        entry_node_key: entry.node_key.clone(),
        entry_eligibility,
    })
}

/// 按节点键查找启动绑定输入；缺失或重复时失败关闭。
fn binding_for<'a>(bindings: &'a [StartBindingInput], node_key: &str) -> EngineResult<&'a StartBindingInput> {
    let matches: Vec<_> = bindings.iter().filter(|item| item.node_key == node_key).collect();
    match matches.as_slice() {
        [only] => Ok(*only),
        _ => Err(EngineError::InvalidCommand("节点审批人绑定缺失或重复")),
    }
}

#[cfg(test)]
mod tests {
    use super::{plan_start, StartBindingInput, StartPlanInput};
    use crate::engine::{Eligibility, EngineError};
    use crate::graph::DefinitionGraph;
    use crate::ids::{
        ApprovalInstanceAssigneeId, ApprovalNodeDefinitionId, ApprovalProcessDefinitionId,
        ApprovalTransitionDefinitionId,
    };
    use crate::model::types::{ApprovalBlockerCode, ApprovalTransitionEvent};
    use crate::model::{
        ApprovalNodeDefinition, ApprovalProcessDefinition, ApprovalTransitionDefinition, ParticipantId,
        ProcessKind, Timestamp,
    };

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

    fn node(
        id: &str,
        key: &str,
        name: &str,
        order: u32,
        user: &str,
        label: &str,
        at: Timestamp,
    ) -> ApprovalNodeDefinition {
        ApprovalNodeDefinition::new(crate::model::NewNodeDefinition {
            id: ApprovalNodeDefinitionId::new(id),
            process_definition_id: ApprovalProcessDefinitionId::new("def"),
            node_key: key.into(),
            node_name: name.into(),
            node_purpose: None,
            display_order: order,
            assignee_participant_id: participant(user),
            assignee_label_snapshot: label.into(),
            at,
        })
        .unwrap()
    }

    fn binding(node_key: &str, id: &str, eligibility: Eligibility) -> StartBindingInput {
        StartBindingInput {
            node_key: node_key.into(),
            assignee_id: ApprovalInstanceAssigneeId::new(id),
            eligibility,
        }
    }

    fn eligible(user: &str, name: &str) -> Eligibility {
        Eligibility::Eligible {
            participant: participant(user),
            assignee_name_snapshot: name.into(),
        }
    }

    fn blocked(user: &str, name: &str, code: ApprovalBlockerCode) -> Eligibility {
        Eligibility::Blocked {
            participant: participant(user),
            code,
            assignee_name_snapshot: name.into(),
        }
    }

    fn participant(id: &str) -> ParticipantId {
        ParticipantId::new(id).unwrap()
    }

    fn at(secs: i64) -> Timestamp {
        Timestamp::from_unix_secs(secs).unwrap()
    }

    /// 通用计划逐节点冻结绑定，并解析入口资格。
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

    /// 绑定输入乱序时仍按节点键匹配，结果与节点顺序一致。
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

    /// 定义版本漂移必须失败关闭，禁止用旧冻结绑定启动新图。
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

    /// 空节点定义不得启动。
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
        let error = plan_start(StartPlanInput {
            graph: &graph,
            expected_definition_version: 1,
            bindings: vec![],
        })
        .unwrap_err();
        assert_eq!(error, EngineError::InvalidCommand("审批定义没有节点，无法启动"));
    }

    /// 节点缺少绑定或同一节点出现重复绑定均失败关闭。
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
        assert_eq!(
            duplicated,
            EngineError::InvalidCommand("节点审批人绑定缺失或重复")
        );
    }

    /// 绑定数量与节点数量不一致时失败关闭。
    #[test]
    fn start_plan_rejects_binding_count_mismatch() {
        let graph = two_node_graph();
        let error = plan_start(StartPlanInput {
            graph: &graph,
            expected_definition_version: 1,
            bindings: vec![binding("n1", "a1", eligible("u1", "张三"))],
        })
        .unwrap_err();
        assert_eq!(
            error,
            EngineError::InvalidCommand("启动绑定必须与定义节点一一对应")
        );
    }

    /// 资格主体不得伪装成定义责任人。
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

    /// 绑定主键重复时失败关闭，禁止同一审批人快照被复用。
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

    /// 入口键缺失时失败关闭，禁止无入口启动。
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

    /// 对象读取失败收敛的 BLOCKED 资格必须原样进入计划，由引擎启动校验统一拒绝。
    #[test]
    fn start_plan_carries_blocked_eligibility_for_read_failure() {
        let graph = two_node_graph();
        let plan = plan_start(StartPlanInput {
            graph: &graph,
            expected_definition_version: 1,
            bindings: vec![
                binding(
                    "n1",
                    "a1",
                    blocked("u1", "张三", ApprovalBlockerCode::ApproverCannotReadSubject),
                ),
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

    /// 计划冻结的绑定可直接交给引擎 `start` 使用。
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
