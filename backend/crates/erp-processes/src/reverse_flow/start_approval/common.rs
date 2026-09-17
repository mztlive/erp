//! 四类逆向单据共用的启动输入构图与节点绑定。
//!
//! 客户退款、供应商退款、回款冲正与付款冲正的启动输入仅单据类型、对象可读
//! 判定与三处文案不同；各单据的公开输入结构只做薄转换后调用本模块，不得再
//! 复制版本校验、绑定构图与入口查找。

use bpm::engine::{DefinitionGraph, StartAssigneeBinding};
use bpm::ids::{
    ApprovalCommandReceiptId, ApprovalInstanceAssigneeId, ApprovalNodeExecutionId, ApprovalProcessInstanceId,
};
use bpm::model::{ParticipantId, SubjectRef, Timestamp};
use erp_core::common::time::Instant;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::approval::execution::authorization::{AuthorizationFailure, converge_eligibility};
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::{ExecutionCommandInput, StartExecutionInput};
use erp_workflow::service::approval::process_kind::process_kind_of;
use id_generator::next_id;

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
            receipt,
            idempotency_key,
            now: timestamp,
        },
        process_kind: process_kind_of(contracts.document_type),
        subject,
        subject_version,
        binding_id: binding.approval_process_definition_id.as_ref().to_string(),
        definition_version: binding.approval_definition_version,
        actor,
        instance_id: ApprovalProcessInstanceId::new(next_id()),
        entry_execution_id: ApprovalNodeExecutionId::new(next_id()),
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
        bindings,
    })
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
    let mut bindings = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let assignee = node.assignee_participant_id.as_str();
        let failure = match readable(organization_id, assignee) {
            Ok(true) => None,
            Ok(false) | Err(_) => Some(AuthorizationFailure::CannotReadSubject),
        };
        bindings.push(StartAssigneeBinding {
            id: ApprovalInstanceAssigneeId::new(next_id()),
            node_key: node.node_key.clone(),
            participant: node.assignee_participant_id.clone(),
            eligibility: converge_eligibility(assignee, &node.assignee_label_snapshot, failure)?,
        });
    }
    if bindings.is_empty() {
        return Err(Error::ConflictError(empty_definition.to_string()));
    }
    Ok(bindings)
}
