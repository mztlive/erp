//! 采购单详情只读审批结构：绑定定义、运行实例与有界历史。

use bpm::engine::DefinitionGraph;
use bpm::model::SubjectRef;
use erp_procurement::entity::purchase_order::{PurchaseChangeOrderStatus, PurchaseOrderStatus};
use erp_workflow::entity::approval_integration::subject_ref_for;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::approval::read::bound_definition_graph;
use mongodb::Database;
use persistence_core::NoTransaction;

use super::approval::order::{
    RECENT_HISTORY_LIMIT, document_approval_view, document_approval_view_with_definition,
};
use super::dto::DocumentApprovalView;
use crate::Result;
use crate::approval_runtime::load_approval_summary;

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
    let runtime = load_approval_summary(
        db,
        &subject,
        u32::try_from(RECENT_HISTORY_LIMIT).expect("审批历史首屏条数在 u32 范围内"),
        &mut NoTransaction,
    )
    .await?;
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
    status: PurchaseChangeOrderStatus,
) -> Result<DocumentApprovalView> {
    let mut view = super::approval::change::document_approval_view(binding, None, status);
    if let Some(binding) = binding {
        let graph = load_bound_definition_graph(db, binding).await?;
        view.definition = Some(super::approval::order::definition_view_from_binding(binding, Some(&graph)));
    }
    let subject = subject_ref_for(DocumentType::PurchaseChangeOrder, id)
        .map_err(|error| crate::Error::ValidationError(error.to_string()))?;
    let runtime = load_approval_summary(
        db,
        &subject,
        u32::try_from(RECENT_HISTORY_LIMIT).expect("审批历史首屏条数在 u32 范围内"),
        &mut NoTransaction,
    )
    .await?;
    view.instance = runtime.instance;
    view.recent_history = runtime.recent_history;
    view.history_page = runtime.history_page;
    Ok(view)
}

/// 查询冻结定义并保持采购缺图错误；不补默认流程。
async fn load_bound_definition_graph(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
) -> Result<DefinitionGraph> {
    Ok(bound_definition_graph(db, binding, "采购单绑定的审批定义不存在", &mut NoTransaction).await?)
}
/// 构造采购审批主体，非法身份保持原参数错误。
fn purchase_order_subject_ref(id: &str) -> Result<SubjectRef> {
    subject_ref_for(DocumentType::PurchaseOrder, id)
        .map_err(|error| crate::Error::ValidationError(error.to_string()))
}
