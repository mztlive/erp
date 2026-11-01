//! 四类逆向单据共用的启动输入构图与节点绑定。
//!
//! 客户退款、供应商退款、回款冲正与付款冲正的启动输入仅单据类型、对象可读
//! 判定与错误文案不同；各单据启动入口直接接收公共载荷，并声明各自合同，
//! 不得再复制输入结构、字段搬运、版本校验、绑定构图与入口查找。

use bpm::engine::{DefinitionGraph, StartAssigneeBinding};
use bpm::model::{ParticipantId, SubjectRef, Timestamp};
use erp_core::common::time::Instant;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::approval::execution::StartExecutionInput;
use erp_workflow::service::approval::execution::authorization::AuthorizationFailure;
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::start_input::{
    StartInputContext, assemble_start_input, start_bindings,
};

use crate::{Error, Result};

/// 逆向启动输入的共用载荷。
///
/// 收拢四类逆向单据启动输入的全部共用字段；单据差异由
/// [`ReverseStartContracts`] 声明。
pub struct ReverseStartInput<'a> {
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

/// 逆向启动随单据种类变化的声明。
///
/// 共用构图只从本结构读取单据差异；新增逆向单据类型只需新增一组声明。
pub struct ReverseStartContracts {
    /// 单据类型。
    pub document_type: DocumentType,
    /// 按单据组织判定审批人对象读取权的函数。
    pub readable: fn(&str, &str) -> Result<bool>,
    /// 绑定定义版本不一致时的冲突文案。
    pub version_mismatch: &'static str,
    /// 定义无节点时的冲突文案。
    pub empty_definition: &'static str,
}

/// 由定义图与单据组织构造逆向启动输入。
///
/// 审批人取自已发布节点，不接受客户端选择。对象读取权失败时收敛为 BLOCKED。
///
/// # 参数
/// * `input` - 定义图、绑定、主体与提交人共用载荷
/// * `contracts` - 随单据种类变化的声明
///
/// # 返回
/// 返回可交给 `prepare_start` 的输入。
///
/// # 错误
/// 定义版本漂移、入口缺失、审批人非法、幂等键非法或读取权校验失败时返回错误。
pub fn build_reverse_start_input(
    input: ReverseStartInput<'_>,
    contracts: &ReverseStartContracts,
) -> Result<StartExecutionInput> {
    let ReverseStartInput {
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
    if graph.definition.definition_version != binding.approval_definition_version {
        return Err(Error::ConflictError(contracts.version_mismatch.to_string()));
    }
    let idempotency_key = normalize_idempotency_key(idempotency_key)?;
    let actor =
        ParticipantId::new(actor_id).map_err(|_| Error::ValidationError("提交人引用无效".to_string()))?;
    let timestamp = Timestamp::from_utc(now.as_utc());
    let bindings =
        reverse_start_bindings(&graph, organization_id, contracts.readable, contracts.empty_definition)?;
    Ok(assemble_start_input(
        graph,
        bindings,
        StartInputContext {
            binding,
            document_type: contracts.document_type,
            subject,
            subject_version,
            actor,
            idempotency_key,
            receipt,
            now: timestamp,
        },
    )?)
}

/// 为定义全部节点冻结启动绑定，并按单据组织重验对象读取权。
///
/// # 参数
/// * `graph` - 定义图
/// * `organization_id` - 单据责任组织
/// * `readable` - 单据的对象读取权判定函数
/// * `empty_definition` - 定义无节点时的冲突文案
///
/// # 返回
/// 返回与节点一一对应的绑定。
///
/// # 错误
/// 节点审批人引用非法、显示名为空或定义无节点时返回错误。
pub fn reverse_start_bindings(
    graph: &DefinitionGraph,
    organization_id: &str,
    readable: fn(&str, &str) -> Result<bool>,
    empty_definition: &str,
) -> Result<Vec<StartAssigneeBinding>> {
    Ok(start_bindings(
        graph,
        |assignee| match readable(organization_id, assignee) {
            Ok(_) => None,
            Err(_) => Some(AuthorizationFailure::CannotReadSubject),
        },
        empty_definition,
    )?)
}

#[cfg(test)]
mod tests {
    use bpm::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId};
    use bpm::model::{ApprovalNodeDefinition, ApprovalProcessDefinition, NewNodeDefinition};
    use erp_workflow::service::approval::process_kind::process_kind_of;

    use super::*;
    use crate::reverse_flow::start_approval::{
        build_customer_refund_start_input, build_payment_reversal_start_input,
        build_receipt_reversal_start_input, build_supplier_refund_start_input,
    };

    type BuildStart = for<'a> fn(ReverseStartInput<'a>) -> Result<StartExecutionInput>;

    fn cases() -> [(DocumentType, BuildStart, &'static str, &'static str); 4] {
        [
            (
                DocumentType::CustomerRefund,
                build_customer_refund_start_input,
                "客户退款单绑定定义版本与已加载定义不一致",
                "审批定义没有节点，无法启动客户退款审批",
            ),
            (
                DocumentType::SupplierRefund,
                build_supplier_refund_start_input,
                "供应商退款单绑定定义版本与已加载定义不一致",
                "审批定义没有节点，无法启动供应商退款审批",
            ),
            (
                DocumentType::PaymentReversal,
                build_payment_reversal_start_input,
                "付款冲正单绑定定义版本与已加载定义不一致",
                "审批定义没有节点，无法启动付款冲正审批",
            ),
            (
                DocumentType::ReceiptReversal,
                build_receipt_reversal_start_input,
                "回款冲正单绑定定义版本与已加载定义不一致",
                "审批定义没有节点，无法启动回款冲正审批",
            ),
        ]
    }

    fn graph(document_type: DocumentType) -> DefinitionGraph {
        let at = Timestamp::from_unix_secs(1).unwrap();
        let definition_id = ApprovalProcessDefinitionId::new("definition");
        DefinitionGraph {
            definition: ApprovalProcessDefinition::new_draft(
                definition_id.clone(),
                process_kind_of(document_type),
                1,
                "逆向审批",
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

    fn binding(version: u32) -> ApprovalDefinitionBinding {
        ApprovalDefinitionBinding::new(
            ApprovalProcessDefinitionId::new("definition"),
            version,
            Instant::from_unix_secs(1),
        )
        .unwrap()
    }

    fn input<'a>(
        document_type: DocumentType,
        graph: DefinitionGraph,
        binding: &'a ApprovalDefinitionBinding,
    ) -> ReverseStartInput<'a> {
        ReverseStartInput {
            graph,
            binding,
            subject: SubjectRef::new(document_type.as_str(), "document").unwrap(),
            subject_version: 3,
            actor_id: "submitter",
            organization_id: "business-org",
            idempotency_key: "  operation-key  ",
            receipt: None,
            now: Instant::from_unix_secs(10),
        }
    }

    #[test]
    fn all_reverse_entries_keep_shared_payload_and_document_process_kind() {
        for (document_type, build, _, _) in cases() {
            let built = build(input(document_type, graph(document_type), &binding(1))).unwrap();
            assert_eq!(built.process_kind, process_kind_of(document_type));
            assert_eq!(built.subject, SubjectRef::new(document_type.as_str(), "document").unwrap());
            assert_eq!(built.subject_version, 3);
            assert_eq!(built.actor.as_str(), "submitter");
            assert_eq!(built.binding_id, "definition");
            assert_eq!(built.definition_version, 1);
            assert_eq!(built.command.idempotency_key.as_str(), "operation-key");
            assert_eq!(built.command.now, Timestamp::from_unix_secs(10).unwrap());
            assert_eq!(built.bindings.len(), 1);
            assert_eq!(built.bindings[0].participant.as_str(), "reviewer");
            assert_eq!(built.command.current_eligibility.participant().as_str(), "reviewer");
        }
    }

    #[test]
    fn all_reverse_entries_preserve_definition_version_conflict_messages() {
        for (document_type, build, version_mismatch, _) in cases() {
            let error = build(input(document_type, graph(document_type), &binding(2))).unwrap_err();
            assert!(matches!(error, Error::ConflictError(message) if message == version_mismatch));
        }
    }

    #[test]
    fn all_reverse_entries_preserve_empty_definition_conflict_messages() {
        for (document_type, build, _, empty_definition) in cases() {
            let mut empty = graph(document_type);
            empty.nodes.clear();
            let error = build(input(document_type, empty, &binding(1))).unwrap_err();
            assert!(matches!(error, Error::ConflictError(message) if message == empty_definition));
        }
    }
}
