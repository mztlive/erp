//! 采购启动输入构图：定义图、绑定与提交人收拢为引擎输入。
//!
//! 审批人取自已发布节点，不接受客户端选择；定义图加载复用
//! [`start_receipt`] 的带 executor 版本。

use bpm::engine::{DefinitionGraph, StartBindingInput, StartPlanInput, plan_start};
use bpm::ids::{
    ApprovalCommandReceiptId, ApprovalInstanceAssigneeId, ApprovalNodeExecutionId, ApprovalProcessInstanceId,
};
use bpm::model::{ParticipantId, SubjectRef, Timestamp};
use erp_core::common::time::Instant;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::approval::execution::authorization::{AuthorizationFailure, converge_eligibility};
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::start::map_engine_error;
use erp_workflow::service::approval::execution::{ExecutionCommandInput, StartExecutionInput};
use erp_workflow::service::approval::process_kind::process_kind_of;
use id_generator::next_id;

use super::super::adapter::purchase_order_object_readable;
#[cfg(test)]
use super::start_persist::{StartStep, StartSteps, execute_start_steps};
use crate::{Error, Result};

/// 采购单启动输入。
///
/// # 用途
/// 收拢 `build_purchase_order_start_input` 的定义图、绑定与提交人参数。
///
/// # 参数
/// 无。
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 审批人取自已发布节点，不接受客户端选择。
pub(crate) struct PurchaseOrderStartInput<'a> {
    /// 绑定定义图。
    pub graph: DefinitionGraph,
    /// 冻结绑定。
    pub binding: &'a ApprovalDefinitionBinding,
    /// 业务对象引用。
    pub subject: SubjectRef,
    /// 冻结提交版本。
    pub subject_version: u32,
    /// 提交人。
    pub actor_id: &'a str,
    /// 单据责任组织。
    pub organization_id: &'a str,
    /// 规范化前的幂等键。
    pub idempotency_key: &'a str,
    /// 已存在收据。
    pub receipt: Option<bpm::model::ApprovalCommandReceipt>,
    /// 调用方时间。
    pub now: Instant,
}

/// 由定义图与单据组织构造启动输入。
///
/// 审批人取自已发布节点，不接受客户端选择。对象读取权失败时收敛为 BLOCKED。
///
/// # 用途
/// 计算逐节点对象读取授权结果并交给 BPM 通用启动计划，再把计划组装为引擎
/// `prepare_start` 输入。
///
/// # 参数
/// * `input` - 定义图、绑定、主体与提交人
///
/// # 返回
/// 返回可交给 `prepare_start` 的输入。
///
/// # 错误
/// 入口缺失、审批人非法、幂等键非法或读取权校验失败时返回错误。
///
/// # 关键业务约束
/// 定义版本漂移、空节点、入口缺失与办理人校验由 BPM `plan_start` 失败关闭；
/// 对象读取权失败收敛为 BLOCKED 资格，由引擎启动校验统一拒绝。采购变更单
/// 与采购单必须复用同一 BPM 规则。
pub(crate) fn build_purchase_order_start_input(
    input: PurchaseOrderStartInput<'_>,
) -> Result<StartExecutionInput> {
    let PurchaseOrderStartInput {
        graph,
        binding,
        subject,
        subject_version,
        actor_id,
        organization_id,
        idempotency_key,
        receipt,
        now,
    } = input;
    let idempotency_key = normalize_idempotency_key(idempotency_key)?;
    let actor =
        ParticipantId::new(actor_id).map_err(|_| Error::ValidationError("提交人引用无效".to_string()))?;
    let timestamp = Timestamp::from_utc(now.as_utc());
    let binding_inputs = graph
        .nodes
        .iter()
        .map(|node| {
            let assignee = node.assignee_participant_id.as_str();
            let failure = match purchase_order_object_readable(organization_id, assignee) {
                Ok(_) => None,
                Err(_) => Some(AuthorizationFailure::CannotReadSubject),
            };
            Ok(StartBindingInput {
                node_key: node.node_key.clone(),
                assignee_id: ApprovalInstanceAssigneeId::new(next_id()),
                eligibility: converge_eligibility(assignee, &node.assignee_label_snapshot, failure)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let plan = plan_start(StartPlanInput {
        graph: &graph,
        expected_definition_version: binding.approval_definition_version,
        bindings: binding_inputs,
    })
    .map_err(map_start_plan_error)?;
    Ok(StartExecutionInput {
        command: ExecutionCommandInput {
            graph,
            current_eligibility: plan.entry_eligibility.clone(),
            next_eligibility: plan.entry_eligibility,
            receipt,
            idempotency_key,
            now: timestamp,
        },
        process_kind: process_kind_of(DocumentType::PurchaseOrder),
        subject,
        subject_version,
        binding_id: binding.approval_process_definition_id.as_ref().to_string(),
        definition_version: binding.approval_definition_version,
        actor,
        instance_id: ApprovalProcessInstanceId::new(next_id()),
        entry_execution_id: ApprovalNodeExecutionId::new(next_id()),
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
        bindings: plan.bindings,
    })
}

/// 将启动计划错误映射为服务错误；计划前置条件保持冲突语义。
///
/// # 参数
/// * `error` - BPM 启动计划错误
///
/// # 返回
/// 计划前置条件返回冲突，其余错误按引擎错误映射。
fn map_start_plan_error(error: bpm::engine::EngineError) -> Error {
    match error {
        bpm::engine::EngineError::InvalidCommand(message) => Error::ConflictError(message.to_string()),
        other => Error::from(map_engine_error(other)),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use bpm::engine::EngineError;
    use bpm::graph::DefinitionGraph;
    use bpm::ids::{
        ApprovalCommandReceiptId, ApprovalNodeDefinitionId, ApprovalProcessDefinitionId,
        ApprovalTransitionDefinitionId,
    };
    use bpm::model::types::{ApprovalBlockerCode, ApprovalTransitionEvent};
    use bpm::model::{
        ApprovalNodeDefinition, ApprovalProcessDefinition, ApprovalTransitionDefinition, ParticipantId,
        ProcessKind, Timestamp,
    };
    use erp_core::common::time::Instant;
    use erp_core::ids::WorkItemId;
    use erp_workflow::entity::document_registry::DocumentType;
    use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
    use erp_workflow::entity::work_item::{DocumentApprovalWorkItemData, WorkItem, WorkItemPriority};
    use erp_workflow::service::approval::execution::idempotency::{StartIdentityParams, start_identity};
    use erp_workflow::service::approval::execution::{PreparedExecution, prepare_start};
    use erp_workflow::service::approval::process_kind::process_kind_of;

    use super::{PurchaseOrderStartInput, build_purchase_order_start_input, map_start_plan_error};
    use crate::Error;

    fn node(
        id: &str,
        key: &str,
        name: &str,
        order: u32,
        user: &str,
        label: &str,
        at: Timestamp,
    ) -> ApprovalNodeDefinition {
        ApprovalNodeDefinition::new(bpm::model::NewNodeDefinition {
            id: ApprovalNodeDefinitionId::new(id),
            process_definition_id: ApprovalProcessDefinitionId::new("def"),
            node_key: key.into(),
            node_name: name.into(),
            node_purpose: None,
            display_order: order,
            assignee_participant_id: ParticipantId::new(user).unwrap(),
            assignee_label_snapshot: label.into(),
            at,
        })
        .unwrap()
    }

    /// 双节点采购审批图夹具：取消输入组装测试复用。
    pub(crate) fn two_node_graph() -> DefinitionGraph {
        let at = at(1);
        DefinitionGraph {
            definition: ApprovalProcessDefinition::new_draft(
                ApprovalProcessDefinitionId::new("def"),
                ProcessKind::PurchaseOrder,
                1,
                "采购审批",
                "n1",
                ParticipantId::new("admin").unwrap(),
                at,
            )
            .unwrap(),
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

    fn empty_graph(entry_node_key: &str) -> DefinitionGraph {
        let at = at(1);
        DefinitionGraph {
            definition: ApprovalProcessDefinition::new_draft(
                ApprovalProcessDefinitionId::new("def"),
                ProcessKind::PurchaseOrder,
                1,
                "采购审批",
                entry_node_key,
                ParticipantId::new("admin").unwrap(),
                at,
            )
            .unwrap(),
            nodes: vec![],
            transitions: vec![],
        }
    }

    fn binding(definition_version: u32) -> ApprovalDefinitionBinding {
        ApprovalDefinitionBinding::new(
            bpm::ids::ApprovalProcessDefinitionId::new("def"),
            definition_version,
            Instant::from_unix_secs(1),
        )
        .unwrap()
    }

    fn at(secs: i64) -> Timestamp {
        Timestamp::from_unix_secs(secs).unwrap()
    }

    fn input<'a>(
        graph: DefinitionGraph,
        binding: &'a ApprovalDefinitionBinding,
        organization_id: &'a str,
    ) -> PurchaseOrderStartInput<'a> {
        PurchaseOrderStartInput {
            graph,
            binding,
            subject: bpm::model::SubjectRef::new("purchase_order", "po-1").unwrap(),
            subject_version: 1,
            actor_id: "starter",
            organization_id,
            idempotency_key: "key-1",
            receipt: None,
            now: Instant::from_unix_secs(10),
        }
    }

    /// 采购单启动输入逐节点冻结绑定，并复用 BPM 通用计划解析入口资格。
    #[test]
    fn purchase_order_start_freezes_all_nodes_with_bpm_plan() {
        let graph = two_node_graph();
        let built = build_purchase_order_start_input(input(graph, &binding(1), "org-1")).unwrap();
        assert_eq!(built.bindings.len(), 2);
        assert_eq!(built.bindings[0].node_key, "n1");
        assert_eq!(built.bindings[0].participant.as_str(), "u1");
        assert_eq!(built.bindings[1].node_key, "n2");
        assert_eq!(built.command.current_eligibility.participant().as_str(), "u1");
        assert_eq!(built.command.graph.definition.definition_version, 1);
        assert_eq!(built.process_kind, process_kind_of(DocumentType::PurchaseOrder));
        assert_eq!(built.definition_version, 1);
        assert_eq!(built.subject_version, 1);
    }

    /// 定义版本漂移时失败关闭，不得用旧冻结绑定启动新图。
    #[test]
    fn purchase_order_start_rejects_definition_version_drift() {
        let graph = two_node_graph();
        let error = build_purchase_order_start_input(input(graph, &binding(2), "org-1")).unwrap_err();
        assert!(
            matches!(error, Error::ConflictError(message) if message.contains("定义版本与冻结绑定不一致"))
        );
    }

    /// 空节点定义不得启动。
    #[test]
    fn purchase_order_start_rejects_empty_node_graph() {
        let error =
            build_purchase_order_start_input(input(empty_graph("n1"), &binding(1), "org-1")).unwrap_err();
        assert!(matches!(error, Error::ConflictError(message) if message.contains("审批定义没有节点")));
    }

    /// 入口键缺失时失败关闭。
    #[test]
    fn purchase_order_start_rejects_missing_entry_node() {
        let mut graph = two_node_graph();
        graph.definition.entry_node_key = "missing".to_string();
        let error = build_purchase_order_start_input(input(graph, &binding(1), "org-1")).unwrap_err();
        assert!(matches!(error, Error::ConflictError(message) if message.contains("审批定义缺少入口节点")));
    }

    /// 对象读取失败收敛为 BLOCKED 资格，并由引擎启动校验统一拒绝。
    #[test]
    fn purchase_order_start_converges_read_failure_to_blocked() {
        let built = build_purchase_order_start_input(input(two_node_graph(), &binding(1), "")).unwrap();
        assert_eq!(
            built.command.current_eligibility.blocked_code(),
            Some(ApprovalBlockerCode::ApproverCannotReadSubject)
        );
        let error = prepare_start(built).unwrap_err();
        assert!(
            matches!(error, erp_workflow::Error::ValidationError(message) if message.contains("启动时全部审批人必须有效"))
        );
    }

    /// 同键同载荷启动收据必须重放，不重复规划写入。
    #[test]
    fn purchase_order_start_replays_matching_receipt() {
        let identity = start_identity(StartIdentityParams {
            idempotency_key: bpm::model::IdempotencyKey::parse("key-1").unwrap(),
            process_kind: process_kind_of(DocumentType::PurchaseOrder).as_str(),
            subject_kind: "purchase_order",
            subject_id: "po-1",
            subject_version: 1,
            binding_id: "def",
            definition_version: 1,
            actor_participant_id: "starter",
        })
        .unwrap();
        let receipt = bpm::model::ApprovalCommandReceipt::new(
            ApprovalCommandReceiptId::new("r1"),
            identity.current(),
            "inst-1",
            at(10),
        )
        .unwrap();
        let mut built =
            build_purchase_order_start_input(input(two_node_graph(), &binding(1), "org-1")).unwrap();
        built.command.receipt = Some(receipt);
        assert!(matches!(prepare_start(built).unwrap(), PreparedExecution::Replay { .. }));
    }

    /// 计划前置条件映射保持冲突语义。
    #[test]
    fn start_plan_error_maps_to_conflict() {
        let error = map_start_plan_error(EngineError::InvalidCommand("定义版本与冻结绑定不一致"));
        assert!(matches!(error, Error::ConflictError(_)));
        let error = map_start_plan_error(EngineError::Uncommittable("无法形成合法快照"));
        assert!(matches!(error, Error::Internal(_)));
    }

    /// 开放任务夹具：取消输入组装测试复用。
    pub(crate) fn open_task(version: u64) -> WorkItem {
        let mut item = WorkItem::new_document_approval(
            WorkItemId::new("wi-1"),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: bpm::ids::ApprovalNodeExecutionId::new("e1"),
                business_object_type: DocumentType::PurchaseOrder.as_str().to_string(),
                business_object_id: "po-1".to_string(),
                subject_version: "1".to_string(),
                owner_role: "purchase_manager".to_string(),
                owner_organization_id: "org-1".to_string(),
                owner_user_id: "u1".to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(10),
        )
        .unwrap();
        item.base.version = version;
        item
    }
    /// 非零大小执行器用于核验传入生产步骤的实例身份。
    struct TestExecutor {
        _identity: u8,
    }

    impl persistence_core::Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }

    struct RecordingSteps {
        expected_executor: usize,
        seen: Vec<super::StartStep>,
        fail_at: Option<super::StartStep>,
    }

    #[async_trait::async_trait]
    impl super::StartSteps for RecordingSteps {
        async fn apply(
            &mut self,
            step: super::StartStep,
            executor: &mut dyn persistence_core::Executor,
        ) -> crate::Result<()> {
            assert_eq!(
                executor as *mut dyn persistence_core::Executor as *mut () as usize,
                self.expected_executor
            );
            self.seen.push(step);
            if self.fail_at == Some(step) {
                return Err(crate::Error::ConflictError("original-step-error".to_string()));
            }
            Ok(())
        }
    }

    /// 实际生产写序使用同一调用方执行器，覆盖领域写入两侧的跨域步骤。
    #[tokio::test]
    async fn start_posting_keeps_original_order_and_executor() {
        use super::StartStep::*;
        let mut executor = TestExecutor { _identity: 1 };
        let mut steps = RecordingSteps {
            expected_executor: &mut executor as *mut TestExecutor as usize,
            seen: vec![],
            fail_at: None,
        };
        super::execute_start_steps(&mut steps, &mut executor).await.unwrap();
        assert_eq!(
            steps.seen,
            [
                Receipt,
                DocumentGuard,
                ProcurementGuard,
                SupplierQualification,
                Submission,
                SupersededDraft,
                Runtime,
                Audit
            ]
        );
    }

    /// 每一生产步骤失败后立即停止；保留原冲突错误，不推进任何后续写入。
    #[tokio::test]
    async fn start_posting_stops_at_each_failure() {
        use super::StartStep::*;
        let expected = [
            Receipt,
            DocumentGuard,
            ProcurementGuard,
            SupplierQualification,
            Submission,
            SupersededDraft,
            Runtime,
            Audit,
        ];
        for (index, step) in expected.iter().copied().enumerate() {
            let mut executor = TestExecutor { _identity: 1 };
            let mut steps = RecordingSteps {
                expected_executor: &mut executor as *mut TestExecutor as usize,
                seen: vec![],
                fail_at: Some(step),
            };
            let error = super::execute_start_steps(&mut steps, &mut executor).await.unwrap_err();
            assert!(
                matches!(error, crate::Error::ConflictError(message) if message == "original-step-error")
            );
            assert_eq!(steps.seen, expected[..=index]);
        }
    }
}
