//! 当前轮次的完整审批链条，人员只来自精确实例的冻结运行图。

use bpm::graph::{DefinitionGraph, ordered_nodes};
use bpm::model::types::ApprovalProcessInstanceStatus;
use bpm::model::{ApprovalNodeDefinition, ApprovalNodeExecution, ApprovalProcessInstance};
use serde::{Deserialize, Serialize};

use super::runtime_history::{RuntimeHistoryItem, history_item_from_execution};
use crate::error::{Error, Result};

/// 当前审批轮次的有序完整路线，不受执行历史分页影响。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeApprovalRoute {
    /// 当前轮次。
    pub round_no: u32,
    /// 按冻结定义顺序排列的全部节点。
    pub nodes: Vec<RuntimeApprovalRouteNode>,
}

/// 一个节点的冻结责任人及本轮最近执行结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeApprovalRouteNode {
    /// 定义内稳定节点键。
    pub node_key: String,
    /// 节点名称。
    pub node_name: String,
    /// 冻结的全部审批人显示名；或签人员以顿号连接并标注或签。
    pub assignee_name: String,
    /// 执行状态；尚未进入时为 PENDING，实例结束后为 NOT_REACHED。
    pub result: String,
    /// 本轮该节点最近一次执行；未进入时为空。
    pub execution: Option<RuntimeHistoryItem>,
}

impl RuntimeApprovalRoute {
    /// 从已授权实例、冻结运行图和本轮节点执行构造完整链条。
    /// 按定义顺序返回全部节点及服务端确定的状态。
    ///
    /// 定义版本、流程种类、执行身份或节点顺序不一致时返回冲突。
    ///
    /// `instance`：精确审批实例；`graph`：已恢复实例冻结审批人的运行图。
    /// `executions`：本轮各节点的最近执行。
    pub fn from_runtime(
        instance: &ApprovalProcessInstance,
        graph: &DefinitionGraph,
        executions: &[ApprovalNodeExecution],
    ) -> Result<Self> {
        if graph.definition.base.id != instance.process_definition_id.as_ref()
            || graph.definition.definition_version != instance.definition_version
            || graph.definition.process_kind != instance.process_kind
            || executions.iter().any(|execution| {
                execution.process_instance_id.as_ref() != instance.base.id
                    || execution.round_no != instance.current_round_no
                    || !graph.nodes.iter().any(|node| node.node_key == execution.node_key)
            })
        {
            return Err(Error::ConflictError("审批链条与本次审批不一致，请刷新后重试".into()));
        }

        let pending = matches!(
            instance.status,
            ApprovalProcessInstanceStatus::Running | ApprovalProcessInstanceStatus::Blocked
        );
        let nodes = ordered_nodes(&graph.nodes)
            .map_err(|error| Error::ConflictError(error.to_string()))?
            .into_iter()
            .map(|node| RuntimeApprovalRouteNode::from_node(node, executions, pending))
            .collect();
        Ok(Self { round_no: instance.current_round_no, nodes })
    }
}

impl RuntimeApprovalRouteNode {
    // 用冻结节点和其最近一次执行生成路线展示。尚未进入的节点根据当前审批实例是否仍开放确定显示状态。
    fn from_node(node: &ApprovalNodeDefinition, executions: &[ApprovalNodeExecution], pending: bool) -> Self {
        let execution = executions
            .iter()
            .filter(|execution| execution.node_key == node.node_key)
            .max_by_key(|execution| execution.execution_no)
            .map(history_item_from_execution);
        let names = node.participants().into_iter().map(|person| person.name).collect::<Vec<_>>().join("、");
        Self {
            node_key: node.node_key.clone(),
            node_name: node.node_name.clone(),
            assignee_name: if node.additional_assignees.is_empty() {
                names
            } else {
                format!("{names}（或签）")
            },
            result: execution
                .as_ref()
                .map(|item| item.result.clone())
                .unwrap_or_else(|| if pending { "PENDING" } else { "NOT_REACHED" }.into()),
            execution,
        }
    }
}

#[cfg(test)]
mod tests {
    use bpm::ProcessKind;
    use bpm::ids::{
        ApprovalInstanceAssigneeId, ApprovalNodeExecutionId, ApprovalProcessDefinitionId,
        ApprovalProcessInstanceId,
    };
    use bpm::model::types::{ApprovalBlockerCode, ApprovalExecutionAssignmentSource};
    use bpm::model::{
        ApprovalInstanceAssignee, ApprovalParticipant, AssigneeResolution, NewNodeExecution,
        NewProcessInstance, ParticipantId, SubjectRef, Timestamp,
    };

    use super::*;
    use crate::service::approval::definition::test_support::two_node_publish_graph;

    fn fixture() -> (ApprovalProcessInstance, DefinitionGraph) {
        let mut graph = two_node_publish_graph();
        graph.definition.process_kind = ProcessKind::SalesOrder;
        graph.nodes[0].assignee_label_snapshot = "张三".into();
        graph.nodes[1].assignee_label_snapshot = "李四".into();
        let mut third = graph.nodes[1].clone();
        third.base.id = "node3".into();
        third.node_key = "n3".into();
        third.node_name = "财务审核".into();
        third.display_order = 3;
        third.assignee_label_snapshot = "王五".into();
        third.assignee_participant_id = Some(ParticipantId::new("u3").unwrap());
        graph.nodes.push(third);

        let instance = ApprovalProcessInstance::start_running(NewProcessInstance {
            id: ApprovalProcessInstanceId::new("instance"),
            process_definition_id: ApprovalProcessDefinitionId::new(graph.definition.base.id.clone()),
            definition_version: graph.definition.definition_version,
            process_kind: graph.definition.process_kind,
            subject: SubjectRef::new("sales_order", "order").unwrap(),
            subject_version: 1,
            started_by: ParticipantId::new("applicant").unwrap(),
            at: Timestamp::from_unix_secs(10).unwrap(),
        })
        .unwrap();
        (instance, graph)
    }

    fn execution(graph: &DefinitionGraph, index: usize, execution_no: u32) -> ApprovalNodeExecution {
        let node = &graph.nodes[index];
        ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new(format!("execution{execution_no}")),
            process_instance_id: ApprovalProcessInstanceId::new("instance"),
            node_key: node.node_key.clone(),
            node_name: node.node_name.clone(),
            round_no: 1,
            execution_no,
            assignment_source: ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: node.assignee().unwrap().clone(),
            assignee_name_snapshot: node.assignee_label_snapshot.clone(),
            at: Timestamp::from_unix_secs(11).unwrap(),
        })
        .unwrap()
    }

    #[test]
    fn full_chain_keeps_approved_current_and_future_people_in_definition_order() {
        let (instance, mut graph) = fixture();
        let mut approved = execution(&graph, 0, 1);
        approved
            .record_approve(
                ParticipantId::new("u1").unwrap(),
                Some("核对无误".into()),
                Timestamp::from_unix_secs(12).unwrap(),
            )
            .unwrap();
        let current = execution(&graph, 1, 2);
        graph.nodes.reverse();

        let route = RuntimeApprovalRoute::from_runtime(&instance, &graph, &[current, approved]).unwrap();
        assert_eq!(
            route
                .nodes
                .iter()
                .map(|node| (node.node_key.as_str(), node.assignee_name.as_str(), node.result.as_str()))
                .collect::<Vec<_>>(),
            vec![("n1", "张三", "APPROVED"), ("n2", "李四", "ACTIVE"), ("n3", "王五", "PENDING")]
        );
        assert_eq!(route.nodes[0].execution.as_ref().unwrap().decision_reason.as_deref(), Some("核对无误"));
        assert!(route.nodes[2].execution.is_none());
    }

    #[test]
    fn rejection_and_cancellation_do_not_leave_future_people_waiting() {
        for status in [ApprovalProcessInstanceStatus::Rejected, ApprovalProcessInstanceStatus::Cancelled] {
            let (mut instance, graph) = fixture();
            let mut first = execution(&graph, 0, 1);
            if status == ApprovalProcessInstanceStatus::Rejected {
                first
                    .record_reject(
                        ParticipantId::new("u1").unwrap(),
                        "补齐资料",
                        Timestamp::from_unix_secs(12).unwrap(),
                    )
                    .unwrap();
            } else {
                first.cancel(Timestamp::from_unix_secs(12).unwrap()).unwrap();
            }
            instance.status = status;
            let route = RuntimeApprovalRoute::from_runtime(&instance, &graph, &[first]).unwrap();
            assert_eq!(
                route.nodes[0].result,
                if status == ApprovalProcessInstanceStatus::Rejected { "REJECTED" } else { "CANCELLED" }
            );
            assert_eq!(route.nodes[1].result, "NOT_REACHED");
            assert_eq!(route.nodes[2].result, "NOT_REACHED");
        }
    }

    #[test]
    fn recovery_uses_latest_execution_and_preserves_blocked_state() {
        let (mut instance, graph) = fixture();
        instance.status = ApprovalProcessInstanceStatus::Blocked;
        let mut old = execution(&graph, 0, 1);
        old.cancel(Timestamp::from_unix_secs(12).unwrap()).unwrap();
        let mut current = execution(&graph, 0, 2);
        current
            .block(ApprovalBlockerCode::ApproverAccountInactive, Timestamp::from_unix_secs(13).unwrap())
            .unwrap();

        let route = RuntimeApprovalRoute::from_runtime(&instance, &graph, &[current, old]).unwrap();
        assert_eq!(route.nodes[0].result, "BLOCKED");
        assert_eq!(route.nodes[0].execution.as_ref().unwrap().execution_id, "execution2");
        assert_eq!(route.nodes[1].result, "PENDING");
    }

    #[test]
    fn or_sign_node_keeps_every_frozen_person() {
        let (instance, mut graph) = fixture();
        graph.nodes[1].additional_assignees.push(ApprovalParticipant {
            participant_id: ParticipantId::new("u4").unwrap(),
            name: "赵六".into(),
        });
        let route = RuntimeApprovalRoute::from_runtime(&instance, &graph, &[]).unwrap();
        assert_eq!(route.nodes[1].assignee_name, "李四、赵六（或签）");
        assert_eq!(route.nodes[1].result, "PENDING");
    }

    #[test]
    fn dynamic_person_comes_from_instance_resolution() {
        let (instance, mut graph) = fixture();
        let mut bindings = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| {
                ApprovalInstanceAssignee::from_definition(
                    ApprovalInstanceAssigneeId::new(format!("binding{index}")),
                    instance.typed_id(),
                    node.node_key.clone(),
                    node.assignee().unwrap().clone(),
                    Timestamp::from_unix_secs(10).unwrap(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        graph.nodes[1].assignee_rule = Some("APPLICANT_TEAM_LEADER".into());
        graph.nodes[1].assignee_participant_id = None;
        graph.nodes[1].assignee_label_snapshot = "现任负责人".into();
        bindings[1].resolution = Some(AssigneeResolution {
            rule: "APPLICANT_TEAM_LEADER".into(),
            subject_id: "applicant".into(),
            source_id: "team".into(),
            source_name: "销售组".into(),
            assignment_id: "assignment".into(),
            participant_name: "原负责人".into(),
            resolved_at: Timestamp::from_unix_secs(10).unwrap(),
        });

        let frozen = graph.with_frozen_assignees(&instance, &bindings).unwrap();
        let route = RuntimeApprovalRoute::from_runtime(&instance, &frozen, &[]).unwrap();
        assert_eq!(route.nodes[1].assignee_name, "原负责人");
    }

    #[test]
    fn wrong_instance_round_version_and_unknown_node_are_rejected() {
        let (instance, graph) = fixture();
        let original = execution(&graph, 0, 1);
        for mismatch in 0..3 {
            let mut execution = original.clone();
            match mismatch {
                0 => execution.process_instance_id = ApprovalProcessInstanceId::new("other"),
                1 => execution.round_no = 2,
                _ => execution.node_key = "unknown".into(),
            }
            assert!(matches!(
                RuntimeApprovalRoute::from_runtime(&instance, &graph, &[execution]),
                Err(Error::ConflictError(_))
            ));
        }

        let mut newer = graph.clone();
        newer.definition.definition_version += 1;
        assert!(matches!(
            RuntimeApprovalRoute::from_runtime(&instance, &newer, &[]),
            Err(Error::ConflictError(_))
        ));
    }

    #[test]
    fn completed_chain_and_history_page_serialize_independently() {
        let (mut instance, graph) = fixture();
        let executions = (0..3)
            .map(|index| {
                let mut execution = execution(&graph, index, u32::try_from(index + 1).unwrap());
                execution
                    .record_approve(
                        execution.assignee_participant_id.clone(),
                        None,
                        Timestamp::from_unix_secs(12).unwrap(),
                    )
                    .unwrap();
                execution
            })
            .collect::<Vec<_>>();
        instance.status = ApprovalProcessInstanceStatus::Approved;
        let route = RuntimeApprovalRoute::from_runtime(&instance, &graph, &executions).unwrap();
        assert!(route.nodes.iter().all(|node| node.result == "APPROVED"));

        let mut page = super::super::runtime_history::history_page_from(
            executions.iter().map(history_item_from_execution).collect(),
            1,
        );
        page.route = Some(route);
        let wire = serde_json::to_value(page).unwrap();
        assert_eq!(wire["items"].as_array().unwrap().len(), 1);
        assert_eq!(wire["route"]["nodes"].as_array().unwrap().len(), 3);
        assert_eq!(wire["route"]["round_no"], 1);
        assert_eq!(wire["has_more"], true);
        assert_eq!(wire["next_cursor"], "1");
    }
}
