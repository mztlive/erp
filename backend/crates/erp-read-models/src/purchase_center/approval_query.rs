//! 采购单详情只读审批结构：绑定定义、运行实例与有界历史。

use bpm::ids::ApprovalProcessInstanceId;
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance, SubjectRef};
use erp_procurement::entity::purchase_order::PurchaseOrderStatus;
use erp_workflow::BpmExt;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::approval::execution::{
    RuntimeHistoryItem, RuntimeHistoryPage, history_item_from_execution, history_page_from,
    latest_rejection_reason,
};
use mongodb::Database;
use persistence_core::NoTransaction;

use super::approval::order::{
    RECENT_HISTORY_LIMIT, document_approval_view, document_approval_view_with_definition,
};
use super::dto::{
    DocumentApprovalHistoryItemView, DocumentApprovalHistoryPageView, DocumentApprovalInstanceView,
    DocumentApprovalView,
};
use crate::Result;

/// 加载采购单详情的只读审批结构。
///
/// 未提交时带出绑定流程名与有序节点；已启动后填入最新实例摘要与有界历史。
///
/// # 参数
/// * `db` - 数据库
/// * `purchase_order_id` - 采购单主键
/// * `binding` - 创建时冻结的定义绑定
/// * `status` - 当前采购单业务状态
///
/// # 返回
/// 返回绑定定义、可选实例、最近历史与单据允许动作。
///
/// # 错误
/// 绑定存在但定义图不存在、主体引用非法或仓储失败时返回错误。
///
/// # 关键业务约束
/// 无绑定不得补默认流程；节点不展开审批人；不得按角色推导下一节点。
pub async fn load_document_approval(
    db: &Database,
    purchase_order_id: &str,
    binding: Option<&ApprovalDefinitionBinding>,
    status: PurchaseOrderStatus,
) -> Result<DocumentApprovalView> {
    let Some(binding) = binding else {
        return Ok(document_approval_view(None, None, status));
    };
    let graph = load_bound_definition_graph(db, binding).await?;
    let subject = purchase_order_subject_ref(purchase_order_id)?;
    let runtime = load_runtime(db, &subject).await?;
    let mut view = document_approval_view_with_definition(Some(binding), Some(&graph), None, status);
    view.instance = runtime.instance;
    view.recent_history = runtime.recent_history;
    view.history_page = runtime.history_page;
    Ok(view)
}

/// 加载采购变更详情的绑定流程、当前实例和有界历史。
///
/// # 参数
/// * `db` - 数据库
/// * `id` - 采购变更单主键
/// * `binding` - 创建时冻结的审批定义绑定
/// * `status` - 采购变更业务状态
///
/// # 返回
/// 返回审批事实；未提交时实例为空，单据动作仍由变更状态决定。
///
/// # 错误
/// 主体非法、绑定定义缺失或仓储读取失败时返回错误。
pub(super) async fn load_change_document_approval(
    db: &Database,
    id: &str,
    binding: Option<&ApprovalDefinitionBinding>,
    status: erp_procurement::entity::purchase_order::PurchaseChangeOrderStatus,
) -> Result<DocumentApprovalView> {
    let mut view = super::approval::change::document_approval_view(binding, None, status);
    if let Some(binding) = binding {
        let graph = load_bound_definition_graph(db, binding).await?;
        view.definition = Some(super::approval::order::definition_view_from_binding(binding, Some(&graph)));
    }
    let subject = erp_workflow::entity::approval_integration::subject_ref_for(
        erp_workflow::entity::document_registry::DocumentType::PurchaseChangeOrder,
        id,
    )
    .map_err(|error| crate::Error::ValidationError(error.to_string()))?;
    let runtime = load_runtime(db, &subject).await?;
    view.instance = runtime.instance;
    view.recent_history = runtime.recent_history;
    view.history_page = runtime.history_page;
    Ok(view)
}

/// 已投影的运行事实。
struct LoadedRuntime {
    instance: Option<DocumentApprovalInstanceView>,
    recent_history: Vec<DocumentApprovalHistoryItemView>,
    history_page: DocumentApprovalHistoryPageView,
}

/// 按主体读取最新实例并投影。
///
/// # 参数
/// * `db` - 数据库
/// * `subject` - 采购单主体引用
///
/// # 返回
/// 没有实例时返回空投影。
///
/// # 错误
/// 仓储失败时返回错误。
async fn load_runtime(db: &Database, subject: &SubjectRef) -> Result<LoadedRuntime> {
    let Some(instance) = db.bpm_workflow().find_latest_by_subject(subject, &mut NoTransaction).await? else {
        return Ok(empty_runtime());
    };
    project_runtime(db, instance).await
}

/// 读取当前执行与有界历史，组装实例摘要。
///
/// # 参数
/// * `db` - 数据库
/// * `instance` - 最新审批实例
///
/// # 返回
/// 返回实例摘要、最近历史与分页游标。
///
/// # 错误
/// 仓储失败时返回错误。
async fn project_runtime(db: &Database, instance: ApprovalProcessInstance) -> Result<LoadedRuntime> {
    let instance_id = ApprovalProcessInstanceId::new(instance.base.id.clone());
    let current = db.bpm_workflow().find_current_execution(&instance_id, &mut NoTransaction).await?;
    let page = load_recent_history(db, &instance_id).await?;
    Ok(LoadedRuntime {
        instance: Some(instance_view(&instance, current.as_ref(), latest_rejection_reason(&page.items))),
        recent_history: page.items.iter().map(history_item_view).collect(),
        history_page: DocumentApprovalHistoryPageView {
            next_cursor: page.next_cursor,
            has_more: page.has_more,
        },
    })
}

/// 读取详情首屏历史，多取一条以判断是否还有后续。
///
/// # 参数
/// * `db` - 数据库
/// * `instance_id` - 审批实例主键
///
/// # 返回
/// 返回截断到 [`RECENT_HISTORY_LIMIT`] 的历史页。
///
/// # 错误
/// 仓储失败时返回错误。
async fn load_recent_history(
    db: &Database,
    instance_id: &ApprovalProcessInstanceId,
) -> Result<RuntimeHistoryPage> {
    let limit = RECENT_HISTORY_LIMIT as u32;
    let rows = db
        .bpm_workflow()
        .list_execution_history(instance_id, None, limit.saturating_add(1), &mut NoTransaction)
        .await?;
    let items = rows.iter().map(history_item_from_execution).collect();
    Ok(history_page_from(items, limit))
}

/// 未启动审批时的空投影。
///
/// # 参数
/// 无。
///
/// # 返回
/// 实例与历史均为空。
fn empty_runtime() -> LoadedRuntime {
    LoadedRuntime {
        instance: None,
        recent_history: Vec::new(),
        history_page: DocumentApprovalHistoryPageView::default(),
    }
}

/// 由实例与当前执行构造详情摘要。
///
/// # 参数
/// * `instance` - 最新审批实例
/// * `current` - 当前 `ACTIVE|BLOCKED` 执行
/// * `latest_rejection` - 最近驳回原因
///
/// # 返回
/// 返回审批摘要所需字段；缺失节点或审批人时省略，不得补默认称谓。
fn instance_view(
    instance: &ApprovalProcessInstance,
    current: Option<&ApprovalNodeExecution>,
    latest_rejection: Option<String>,
) -> DocumentApprovalInstanceView {
    DocumentApprovalInstanceView::new(instance.base.id.clone(), instance.status.as_str().to_string())
        .with_current_round_no(instance.current_round_no)
        .with_current_node(current.map(|item| item.node_key.clone()))
        .with_current_node_name(current.map(|item| item.node_name.clone()))
        .with_current_assignee(current.map(|item| item.assignee_participant_id.as_str().to_string()))
        .with_current_assignee_name(current.and_then(|item| optional_text(&item.assignee_name_snapshot)))
        .with_latest_rejection(latest_rejection)
        .with_process_version(Some(instance.definition_version))
        .with_blocker_code(instance.blocker_code.map(|code| code.as_str().to_string()))
}

/// 把运行历史项转为采购单详情 DTO。
///
/// # 参数
/// * `item` - 通用历史项
///
/// # 返回
/// 返回单据详情 `recent_history` 项。
fn history_item_view(item: &RuntimeHistoryItem) -> DocumentApprovalHistoryItemView {
    DocumentApprovalHistoryItemView::new(
        item.execution_id.clone(),
        item.node_key.clone(),
        item.node_name.clone(),
        item.result.clone(),
    )
    .with_round_no(item.round_no)
    .with_execution_no(item.execution_no)
    .with_assignee_name(item.assignee_name.clone())
    .with_decided_by(item.decided_by.clone())
    .with_decision_reason(item.decision_reason.clone())
    .with_decided_at(item.decided_at)
}

/// 去掉空白显示名。
///
/// # 参数
/// * `value` - 原始文本
///
/// # 返回
/// 非空文本；空白返回 `None`。
fn optional_text(value: &str) -> Option<String> {
    let text = value.trim();
    if text.is_empty() { None } else { Some(text.to_string()) }
}

/// 查询冻结定义并保持采购缺图错误；不补默认流程。
async fn load_bound_definition_graph(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
) -> Result<bpm::engine::DefinitionGraph> {
    let graph = db
        .bpm_workflow()
        .load_definition_graph(&binding.approval_process_definition_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| crate::Error::ConflictError("采购单绑定的审批定义不存在".to_string()))?;
    Ok(bpm::engine::DefinitionGraph {
        definition: graph.definition,
        nodes: graph.nodes,
        transitions: graph.transitions,
    })
}
/// 构造采购审批主体，非法身份保持原参数错误。
fn purchase_order_subject_ref(id: &str) -> Result<SubjectRef> {
    erp_workflow::entity::approval_integration::subject_ref_for(
        erp_workflow::entity::document_registry::DocumentType::PurchaseOrder,
        id,
    )
    .map_err(|error| crate::Error::ValidationError(error.to_string()))
}
