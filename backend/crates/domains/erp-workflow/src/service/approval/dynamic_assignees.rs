//! 动态来源只在提交前解析；运行期间由实例绑定重建人员。
use bpm::graph::DefinitionGraph;
use bpm::model::{ApprovalInstanceAssignee, ApprovalProcessInstance};
use erp_core::common::time::Instant;
use persistence_core::Executor;

use super::assignee_source::AssigneeSource;
use super::business_adapter::ensure_separation_of_duties;
use super::policy::require_process_required;
use super::process_kind::document_type_of;
use crate::ports::approver_directory::ApproverDirectory;
use crate::{Error, Result};

/// 按本次申请人解析全部动态节点；已发布定义不被写回。
/// # 参数
/// 负责人端口、定义图、申请人、解析时点及调用方执行器。
/// # 返回
/// 返回可交给引擎的已解析图。
/// # 错误
/// 任一节点无法解析或违反岗位分离时拒绝整次启动。
pub async fn resolve_graph(
    directory: &impl ApproverDirectory,
    mut graph: DefinitionGraph,
    applicant: &str,
    at: Instant,
    ex: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    for node in &mut graph.nodes {
        let source = AssigneeSource::from_rule(node.assignee_rule.as_deref())?;
        if source == AssigneeSource::Specified {
            continue;
        }
        let resolved = directory.resolve(applicant, source, at, ex).await.map_err(|error| match error {
            Error::ValidationError(message) => {
                Error::ValidationError(format!("“{}”节点：{message}", node.node_name))
            },
            other => other,
        })?;
        node.assignee_participant_id = Some(resolved.participant.participant_id);
        node.assignee_label_snapshot = resolved.participant.name;
        node.resolution = Some(resolved.resolution);
        node.validate_assignment()
            .map_err(|e| Error::ValidationError(format!("“{}”节点：{e}", node.node_name)))?;
    }
    let policy = require_process_required(document_type_of(graph.definition.process_kind))?;
    ensure_separation_of_duties(policy.separation_of_duties_policy, applicant, &graph.assignee_ids())?;
    Ok(graph)
}

/// 在最终写入事务中复验预解析依据，拒绝提交期间负责人或资格变化。
/// # 参数
/// 负责人端口、待写实例及全部冻结指派、提交时点和原事务。
/// # 返回
/// 全部动态指派与当前有效关系一致时允许写入。
/// # 错误
/// 申请人、组织、负责人指派发生变化或资格失效时拒绝；不得替换已准备的任务。
pub async fn revalidate_assignments(
    directory: &impl ApproverDirectory,
    instance: &ApprovalProcessInstance,
    assignments: &[ApprovalInstanceAssignee],
    at: Instant,
    ex: &mut dyn Executor,
) -> Result<()> {
    for assignment in assignments {
        let Some(frozen) = &assignment.resolution else { continue };
        if frozen.subject_id != instance.started_by.as_str() {
            return Err(Error::ValidationError("审批人来源与申请人不一致，请重新提交".into()));
        }
        let source = AssigneeSource::from_rule(Some(&frozen.rule))?;
        let current = directory.resolve(&frozen.subject_id, source, at, ex).await?;
        if current.participant.participant_id != assignment.current_assignee_participant_id
            || current.resolution.source_id != frozen.source_id
            || current.resolution.assignment_id != frozen.assignment_id
        {
            return Err(Error::ValidationError("组织负责人已变化，请重新提交以更新审批路线".into()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use bpm::engine::{DecideCommand, Eligibility, StartAssigneeBinding, StartCommand, decide, start};
    use bpm::ids::{ApprovalInstanceAssigneeId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
    use bpm::model::types::ApprovalDecision;
    use bpm::model::{ApprovalParticipant, AssigneeResolution, SubjectRef};
    use bpm::{ParticipantId, Timestamp};
    use persistence_core::NoTransaction;

    use super::super::definition::test_support::two_node_publish_graph;
    use super::*;
    use crate::ports::approver_directory::ResolvedApprover;

    struct Directory {
        user: &'static str,
        assignment: &'static str,
        fail: bool,
    }
    impl ApproverDirectory for Directory {
        async fn resolve(
            &self,
            applicant: &str,
            source: AssigneeSource,
            at: Instant,
            _: &mut dyn Executor,
        ) -> Result<ResolvedApprover> {
            if self.fail {
                return Err(Error::ValidationError("未配置负责人".into()));
            }
            Ok(ResolvedApprover {
                participant: ApprovalParticipant {
                    participant_id: ParticipantId::new(self.user).unwrap(),
                    name: "负责人甲".into(),
                },
                resolution: AssigneeResolution {
                    rule: source.rule().unwrap().into(),
                    subject_id: applicant.into(),
                    source_id: "team".into(),
                    source_name: "销售一组".into(),
                    assignment_id: self.assignment.into(),
                    participant_name: "负责人甲".into(),
                    resolved_at: Timestamp::from_unix_secs(at.unix_secs()).unwrap(),
                },
            })
        }
    }
    fn graph() -> DefinitionGraph {
        let mut graph = two_node_publish_graph();
        graph.nodes[0].assignee_participant_id = None;
        graph.nodes[0].assignee_rule = Some("APPLICANT_TEAM_LEADER".into());
        graph
            .definition
            .publish(ParticipantId::new("admin").unwrap(), Timestamp::from_unix_secs(2).unwrap())
            .unwrap();
        graph
    }
    fn start_graph(resolved: &DefinitionGraph) -> bpm::engine::EngineResult<bpm::engine::TransitionPlan> {
        let bindings = resolved
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| StartAssigneeBinding {
                id: ApprovalInstanceAssigneeId::new(format!("binding{index}")),
                node_key: node.node_key.clone(),
                participant: node.assignee().unwrap().clone(),
                eligibility: Eligibility::Eligible {
                    participant: node.assignee().unwrap().clone(),
                    assignee_name_snapshot: node.assignee_label_snapshot.clone(),
                },
            })
            .collect::<Vec<_>>();
        start(
            StartCommand {
                instance_id: ApprovalProcessInstanceId::new("instance"),
                process_kind: resolved.definition.process_kind,
                subject: SubjectRef::new("stock_adjustment", "doc").unwrap(),
                subject_version: 1,
                started_by: ParticipantId::new("applicant").unwrap(),
                entry_execution_id: ApprovalNodeExecutionId::new("exec1"),
                now: Timestamp::from_unix_secs(10).unwrap(),
            },
            resolved,
            &bindings,
        )
    }

    struct NamedDirectory {
        bad_source: Option<AssigneeSource>,
        bad_name: String,
    }
    impl ApproverDirectory for NamedDirectory {
        async fn resolve(
            &self,
            applicant: &str,
            source: AssigneeSource,
            at: Instant,
            ex: &mut dyn Executor,
        ) -> Result<ResolvedApprover> {
            let directory = Directory { user: "leader", assignment: "a1", fail: false };
            let mut resolved = directory.resolve(applicant, source, at, ex).await?;
            resolved.resolution.source_name = "部".repeat(100);
            resolved.resolution.participant_name = "张三".into();
            let mut resolved =
                ResolvedApprover::new(resolved.participant.participant_id, resolved.resolution)?;
            if self.bad_source == Some(source) {
                // A faulty port can bypass the constructor; graph validation must still reject it.
                resolved.participant.name = self.bad_name.clone();
                resolved.resolution.participant_name = self.bad_name.clone();
            }
            Ok(resolved)
        }
    }
    fn both_dynamic() -> DefinitionGraph {
        let mut graph = graph();
        graph.nodes[1].assignee_participant_id = None;
        graph.nodes[1].assignee_rule = Some("APPLICANT_DEPARTMENT_LEADER".into());
        graph
    }
    #[tokio::test]
    async fn long_organization_names_allow_entry_and_later_execution() {
        let original = both_dynamic();
        let directory = NamedDirectory { bad_source: None, bad_name: String::new() };
        let resolved = resolve_graph(
            &directory,
            original.clone(),
            "applicant",
            Instant::from_unix_secs(10),
            &mut NoTransaction,
        )
        .await
        .unwrap();
        let plan = start_graph(&resolved).unwrap();
        assert_eq!(plan.created_executions[0].assignee_name_snapshot, "张三");
        for binding in &plan.created_assignees {
            let snapshot = binding.resolution.as_ref().unwrap();
            assert_eq!(snapshot.source_name, "部".repeat(100));
            assert_eq!(snapshot.participant_name, "张三");
        }
        let restored = original.with_frozen_assignees(&plan.instance, &plan.created_assignees).unwrap();
        let eligible = |index: usize| Eligibility::Eligible {
            participant: restored.nodes[index].assignee().unwrap().clone(),
            assignee_name_snapshot: restored.nodes[index].assignee_label_snapshot.clone(),
        };
        let next = decide(
            plan.instance,
            plan.created_executions[0].clone(),
            &restored,
            DecideCommand {
                decision: ApprovalDecision::Approve,
                reason: None,
                actor: restored.nodes[0].assignee().unwrap().clone(),
                current_eligibility: eligible(0),
                next_eligibility: eligible(1),
                next_execution_id: ApprovalNodeExecutionId::new("long-org-exec2"),
                next_execution_no: 2,
                now: Timestamp::from_unix_secs(11).unwrap(),
            },
        )
        .unwrap();
        assert_eq!(next.created_executions[0].node_key, "n2");
        assert_eq!(next.created_executions[0].assignee_name_snapshot, "张三");
    }
    #[tokio::test]
    async fn invalid_names_on_any_dynamic_node_reject_before_start() {
        for source in [AssigneeSource::ApplicantTeamLeader, AssigneeSource::ApplicantDepartmentLeader] {
            for bad_name in [" ".into(), "人".repeat(43)] {
                let directory = NamedDirectory { bad_source: Some(source), bad_name };
                let error = resolve_graph(
                    &directory,
                    both_dynamic(),
                    "applicant",
                    Instant::from_unix_secs(10),
                    &mut NoTransaction,
                )
                .await
                .unwrap_err();
                assert!(
                    matches!(error, Error::ValidationError(message) if message.contains("节点") && message.contains("审批人显示名"))
                );
            }
        }
        let directory = NamedDirectory { bad_source: None, bad_name: String::new() };
        let mut resolved = resolve_graph(
            &directory,
            both_dynamic(),
            "applicant",
            Instant::from_unix_secs(10),
            &mut NoTransaction,
        )
        .await
        .unwrap();
        resolved.nodes[1].resolution.as_mut().unwrap().participant_name = "不一致".into();
        assert!(start_graph(&resolved).is_err());
        resolved.nodes[1].assignee_label_snapshot = "人".repeat(43);
        resolved.nodes[1].resolution.as_mut().unwrap().participant_name = "人".repeat(43);
        assert!(start_graph(&resolved).is_err());
    }

    #[tokio::test]
    async fn resolves_freezes_and_reloads_original_person() {
        let original = graph();
        let directory = Directory { user: "leader", assignment: "a1", fail: false };
        let at = Instant::from_unix_secs(10);
        let resolved =
            resolve_graph(&directory, original.clone(), "applicant", at, &mut NoTransaction).await.unwrap();
        assert!(original.nodes[0].assignee_participant_id.is_none());
        let plan = start_graph(&resolved).unwrap();
        revalidate_assignments(&directory, &plan.instance, &plan.created_assignees, at, &mut NoTransaction)
            .await
            .unwrap();
        let changed = Directory { user: "new-leader", assignment: "a2", fail: false };
        assert!(
            revalidate_assignments(&changed, &plan.instance, &plan.created_assignees, at, &mut NoTransaction)
                .await
                .is_err()
        );
        assert!(
            original.clone().with_frozen_assignees(&plan.instance, &plan.created_assignees[..1]).is_err()
        );
        let mut tampered = plan.created_assignees.clone();
        tampered[0].resolution.as_mut().unwrap().subject_id = "other".into();
        assert!(original.clone().with_frozen_assignees(&plan.instance, &tampered).is_err());
        let restored = original.with_frozen_assignees(&plan.instance, &plan.created_assignees).unwrap();
        assert_eq!(restored.nodes[0].assignee().unwrap().as_str(), "leader");
        assert_eq!(restored.nodes[0].resolution.as_ref().unwrap().source_name, "销售一组");
        let eligible = |index: usize| Eligibility::Eligible {
            participant: restored.nodes[index].assignee().unwrap().clone(),
            assignee_name_snapshot: restored.nodes[index].assignee_label_snapshot.clone(),
        };
        let next = decide(
            plan.instance,
            plan.created_executions[0].clone(),
            &restored,
            DecideCommand {
                decision: ApprovalDecision::Approve,
                reason: None,
                actor: restored.nodes[0].assignee().unwrap().clone(),
                current_eligibility: eligible(0),
                next_eligibility: eligible(1),
                next_execution_id: ApprovalNodeExecutionId::new("exec2"),
                next_execution_no: 2,
                now: Timestamp::from_unix_secs(11).unwrap(),
            },
        )
        .unwrap();
        assert_eq!(next.created_executions[0].node_key, "n2");
        let rejected = decide(
            next.instance,
            next.created_executions[0].clone(),
            &restored,
            DecideCommand {
                decision: ApprovalDecision::Reject,
                reason: Some("重新核对".into()),
                actor: restored.nodes[1].assignee().unwrap().clone(),
                current_eligibility: eligible(1),
                next_eligibility: eligible(0),
                next_execution_id: ApprovalNodeExecutionId::new("exec3"),
                next_execution_no: 3,
                now: Timestamp::from_unix_secs(12).unwrap(),
            },
        )
        .unwrap();
        assert_eq!(rejected.created_executions[0].assignee_participant_id.as_str(), "leader");
        assert_eq!(rejected.created_executions[0].round_no, 2);
    }
    #[tokio::test]
    async fn duplicate_leaders_keep_separate_nodes_and_copy_preserves_rules() {
        use bpm::graph::{CopiedNodeIdentity, copy_nodes_for_definition};
        use bpm::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId};
        let mut original = graph();
        original.nodes[1].assignee_participant_id = None;
        original.nodes[1].assignee_rule = Some("APPLICANT_DEPARTMENT_LEADER".into());
        original.validate_linear().unwrap();
        let identities = (1..=2)
            .map(|i| CopiedNodeIdentity {
                node_id: ApprovalNodeDefinitionId::new(format!("copy{i}")),
                node_key: format!("copy{i}"),
            })
            .collect::<Vec<_>>();
        let copied = copy_nodes_for_definition(
            &original.nodes,
            ApprovalProcessDefinitionId::new("copy"),
            &identities,
            Timestamp::from_unix_secs(3).unwrap(),
        )
        .unwrap();
        assert_eq!(copied[0].assignee_rule, original.nodes[0].assignee_rule);
        assert_eq!(copied[1].assignee_rule, original.nodes[1].assignee_rule);
        assert!(copied.iter().all(|n| n.assignee_participant_id.is_none() && n.resolution.is_none()));
        let directory = Directory { user: "leader", assignment: "a1", fail: false };
        let resolved =
            resolve_graph(&directory, original, "applicant", Instant::from_unix_secs(10), &mut NoTransaction)
                .await
                .unwrap();
        assert_eq!(resolved.nodes.len(), 2);
        assert_ne!(resolved.nodes[0].node_key, resolved.nodes[1].node_key);
        assert_eq!(resolved.nodes[0].assignee().unwrap(), resolved.nodes[1].assignee().unwrap());
    }

    #[tokio::test]
    async fn missing_leader_and_separation_failure_reject_start() {
        let at = Instant::from_unix_secs(10);
        let unavailable = Directory { user: "leader", assignment: "a1", fail: true };
        assert!(resolve_graph(&unavailable, graph(), "applicant", at, &mut NoTransaction).await.is_err());
        let own = Directory { user: "applicant", assignment: "a1", fail: false };
        assert!(resolve_graph(&own, graph(), "applicant", at, &mut NoTransaction).await.is_err());
        let mut sales = graph();
        sales.definition.process_kind = bpm::ProcessKind::SalesOrder;
        assert!(resolve_graph(&own, sales, "applicant", at, &mut NoTransaction).await.is_ok());
    }
}
