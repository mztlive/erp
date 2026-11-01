//! 各单据启动审批共用的节点绑定与输入装配。

use bpm::engine::{DefinitionGraph, StartAssigneeBinding};
use bpm::ids::{
    ApprovalCommandReceiptId, ApprovalInstanceAssigneeId, ApprovalNodeExecutionId, ApprovalProcessInstanceId,
};
use bpm::model::{ApprovalCommandReceipt, IdempotencyKey, ParticipantId, SubjectRef, Timestamp};
use id_generator::next_id;

use super::authorization::{AuthorizationFailure, converge_node_eligibility};
use super::{ExecutionCommandInput, StartExecutionInput};
use crate::entity::document_registry::DocumentType;
use crate::entity::document_registry::business_document::ApprovalDefinitionBinding;
use crate::error::{Error, Result};
use crate::service::approval::process_kind::process_kind_of;

/// 调用方已完成版本、幂等键、提交人及专属资格检查的启动上下文。
pub struct StartInputContext<'a> {
    /// 调用方已核对定义版本的冻结绑定。
    pub binding: &'a ApprovalDefinitionBinding,
    /// 本次审批的单据类型。
    pub document_type: DocumentType,
    /// 冻结业务对象引用。
    pub subject: SubjectRef,
    /// 冻结提交版本。
    pub subject_version: u32,
    /// 已验证的提交人。
    pub actor: ParticipantId,
    /// 已规范化的幂等键。
    pub idempotency_key: IdempotencyKey,
    /// 已读取的精确启动命令收据。
    pub receipt: Option<ApprovalCommandReceipt>,
    /// 调用方确定的命令时间。
    pub now: Timestamp,
}

/// 按定义顺序冻结节点绑定，使用调用方提供的对象读取资格。
///
/// # 参数
/// * `graph` - 本次启动使用的定义图。
/// * `check` - 各节点参与人的专属资格失败原因；调用方负责外部事实读取。
/// * `empty_definition` - 入口既有的空定义冲突提示。
///
/// # 返回
/// 返回保留定义顺序、审批人与资格的节点绑定。
///
/// # 错误
/// 节点审批人或显示名非法时返回校验错误；空定义返回入口约定的冲突错误。
pub fn start_bindings(
    graph: &DefinitionGraph,
    mut check: impl FnMut(&str) -> Option<AuthorizationFailure>,
    empty_definition: &str,
) -> Result<Vec<StartAssigneeBinding>> {
    let mut bindings = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        bindings.push(StartAssigneeBinding {
            id: ApprovalInstanceAssigneeId::new(next_id()),
            node_key: node.node_key.clone(),
            participant: node.assignee().map_err(|error| Error::ValidationError(error.to_string()))?.clone(),
            eligibility: converge_node_eligibility(node, &mut check)?,
        });
    }
    if bindings.is_empty() {
        return Err(Error::ConflictError(empty_definition.to_string()));
    }
    Ok(bindings)
}

/// 从已完成业务校验的绑定构造启动输入。
///
/// # 参数
/// * `graph` - 调用方核对冻结版本后的定义图。
/// * `bindings` - 调用方完成专属资格检查后的节点绑定。
/// * `context` - 已验证的命令身份、冻结业务引用与时间。
///
/// # 返回
/// 返回入口资格一致的启动输入；仅在入口及其绑定有效后分配实例、执行与收据 ID。
///
/// # 错误
/// 定义缺少入口或入口缺少审批人绑定时返回既有冲突错误。
pub fn assemble_start_input(
    graph: DefinitionGraph,
    bindings: Vec<StartAssigneeBinding>,
    context: StartInputContext<'_>,
) -> Result<StartExecutionInput> {
    let entry = graph.entry_node().map_err(|_| Error::ConflictError("审批定义缺少入口节点".to_string()))?;
    let entry_eligibility = bindings
        .iter()
        .find(|item| item.node_key == entry.node_key)
        .map(|item| item.eligibility.clone())
        .ok_or_else(|| Error::ConflictError("入口节点缺少审批人绑定".to_string()))?;
    Ok(StartExecutionInput {
        command: ExecutionCommandInput {
            graph,
            current_eligibility: entry_eligibility.clone(),
            next_eligibility: entry_eligibility,
            receipt: context.receipt,
            idempotency_key: context.idempotency_key,
            now: context.now,
        },
        process_kind: process_kind_of(context.document_type),
        subject: context.subject,
        subject_version: context.subject_version,
        binding_id: context.binding.approval_process_definition_id.as_ref().to_string(),
        definition_version: context.binding.approval_definition_version,
        actor: context.actor,
        instance_id: ApprovalProcessInstanceId::new(next_id()),
        entry_execution_id: ApprovalNodeExecutionId::new(next_id()),
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
        bindings,
    })
}

#[cfg(test)]
mod tests {
    use bpm::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId};
    use bpm::model::types::ApprovalBlockerCode;
    use bpm::model::{ApprovalNodeDefinition, ApprovalProcessDefinition, NewNodeDefinition};
    use erp_core::common::time::Instant;

    use super::*;
    use crate::service::approval::execution::idempotency::normalize_idempotency_key;
    use crate::service::approval::execution::{PreparedExecution, prepare_start};

    fn graph() -> DefinitionGraph {
        let at = Timestamp::from_unix_secs(1).unwrap();
        let definition_id = ApprovalProcessDefinitionId::new("definition");
        DefinitionGraph {
            definition: ApprovalProcessDefinition::new_draft(
                definition_id.clone(),
                process_kind_of(DocumentType::SalesOrder),
                1,
                "销售审批",
                "entry",
                ParticipantId::new("admin").unwrap(),
                at,
            )
            .unwrap(),
            nodes: vec![
                ApprovalNodeDefinition::new(NewNodeDefinition {
                    id: ApprovalNodeDefinitionId::new("node"),
                    process_definition_id: definition_id,
                    node_key: "entry".into(),
                    node_name: "复核".into(),
                    node_purpose: None,
                    display_order: 1,
                    assignee_participant_id: Some(ParticipantId::new("reviewer").unwrap()),
                    assignee_label_snapshot: "复核人".into(),
                    at,
                })
                .unwrap(),
            ],
            transitions: vec![],
        }
    }

    fn binding() -> ApprovalDefinitionBinding {
        ApprovalDefinitionBinding::new(
            ApprovalProcessDefinitionId::new("definition"),
            1,
            Instant::from_unix_secs(1),
        )
        .unwrap()
    }

    fn context(binding: &ApprovalDefinitionBinding) -> StartInputContext<'_> {
        StartInputContext {
            binding,
            document_type: DocumentType::SalesOrder,
            subject: SubjectRef::new("sales_order", "order-1").unwrap(),
            subject_version: 3,
            actor: ParticipantId::new("seller").unwrap(),
            idempotency_key: normalize_idempotency_key(" operation-key ").unwrap(),
            receipt: None,
            now: Timestamp::from_unix_secs(10).unwrap(),
        }
    }

    #[test]
    fn assembled_input_keeps_frozen_identity_and_drives_real_start_and_replay() {
        let graph = graph();
        let bindings = start_bindings(&graph, |_| None, "空定义").unwrap();
        let input = assemble_start_input(graph, bindings, context(&binding())).unwrap();
        assert_eq!(input.subject.subject_id(), "order-1");
        assert_eq!(input.subject_version, 3);
        assert_eq!(input.binding_id, "definition");
        assert_eq!(input.definition_version, 1);
        assert_eq!(input.actor.as_str(), "seller");
        assert_eq!(input.command.idempotency_key.as_str(), "operation-key");
        assert_eq!(input.command.now, Timestamp::from_unix_secs(10).unwrap());
        assert_eq!(input.command.current_eligibility, input.command.next_eligibility);
        let PreparedExecution::Apply(writes) = prepare_start(input.clone()).unwrap() else {
            panic!("首次启动必须产生写入计划");
        };
        assert_eq!(writes.instance.base.id, input.instance_id.as_ref());
        assert_eq!(writes.created_executions[0].base.id, input.entry_execution_id.as_ref());
        assert_eq!(writes.receipt.base.id, input.receipt_id.as_ref());
        let mut replay = input;
        replay.command.receipt = Some(writes.receipt.clone());
        assert!(matches!(prepare_start(replay).unwrap(), PreparedExecution::Replay { .. }));
    }

    #[test]
    fn binding_qualification_preserves_blocked_subject_and_definition_order() {
        let mut graph = graph();
        let mut second = graph.nodes[0].clone();
        second.base.id = "node-2".into();
        second.node_key = "second".into();
        second.display_order = 2;
        second.assignee_participant_id = Some(ParticipantId::new("next-reviewer").unwrap());
        graph.nodes.push(second);
        let mut checked = Vec::new();
        let bindings = start_bindings(
            &graph,
            |participant| {
                checked.push(participant.to_string());
                Some(AuthorizationFailure::CannotReadSubject)
            },
            "空定义",
        )
        .unwrap();
        assert_eq!(checked, ["reviewer", "next-reviewer"]);
        assert_eq!(bindings[0].node_key, graph.nodes[0].node_key);
        assert_eq!(bindings[1].node_key, graph.nodes[1].node_key);
        assert_ne!(bindings[0].id, bindings[1].id);
        let input = assemble_start_input(graph, bindings, context(&binding())).unwrap();
        assert_eq!(
            input.command.current_eligibility.blocked_code(),
            Some(ApprovalBlockerCode::ApproverCannotReadSubject)
        );
    }

    #[test]
    fn empty_definition_and_missing_entry_or_binding_keep_distinct_conflicts() {
        let mut empty = graph();
        empty.nodes.clear();
        let error = start_bindings(&empty, |_| None, "入口专属空定义提示").unwrap_err();
        assert!(matches!(error, Error::ConflictError(message) if message == "入口专属空定义提示"));
        let error = assemble_start_input(empty, vec![], context(&binding())).unwrap_err();
        assert!(matches!(error, Error::ConflictError(message) if message == "审批定义缺少入口节点"));
        let error = assemble_start_input(graph(), vec![], context(&binding())).unwrap_err();
        assert!(matches!(error, Error::ConflictError(message) if message == "入口节点缺少审批人绑定"));
    }

    #[test]
    fn unresolved_assignee_stops_before_checking_subject_access() {
        let mut graph = graph();
        graph.nodes[0].assignee_participant_id = None;
        let mut checks = 0;
        let error = start_bindings(
            &graph,
            |_| {
                checks += 1;
                None
            },
            "空定义",
        )
        .unwrap_err();
        assert!(matches!(error, Error::ValidationError(message) if message.contains("审批人来源尚未解析")));
        assert_eq!(checks, 0);
    }
}
