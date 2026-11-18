//! 销售变更真实审批运行投影与当前经办人允许动作。

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::AuthorizationService;
use erp_permissions::{sales_order as sales_order_permissions, sales_review as sales_review_permissions};
use erp_sales::entity::sales_review::{SalesChangeOrder, SalesChangeOrderStatus};
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use mongodb::Database;
use persistence_core::Executor;

use super::dto::{DocumentApprovalHistoryItemView, DocumentApprovalHistoryPageView, DocumentApprovalView};
use super::projection::document_approval_view;
use crate::approval_runtime::load_document_runtime;
use crate::sales_center::access::SalesAccess;
use crate::{Error, Result};

// 查询已授权销售变更的实际实例、历史和当前操作者允许动作。
//
// 调用参数：
// * `db` - 原业务读取数据库
// * `rbac` - 当前共享权限服务
// * `change` - 已沿来源销售单 detail 授权的变更单
// * `binding` - 原单冻结的审批定义绑定
// * `executor` - 原授权和详情读取执行器
//
// 返回真实运行摘要，只有当前责任、静态权限和运行规则均允许时带出操作。
//
// 运行事实损坏、当前资格读取或仓储失败时拒绝。
pub(super) async fn load_change_document_approval(
    db: &Database,
    rbac: &Arc<AuthorizationService>,
    change: &SalesChangeOrder,
    binding: Option<&ApprovalDefinitionBinding>,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<DocumentApprovalView> {
    let mut view = document_approval_view(binding, None, change.stable.status());
    let runtime =
        load_document_runtime(db, DocumentType::SalesChangeOrder, &change.base.id, binding, executor).await?;
    let cancellable = runtime.as_ref().is_some_and(|runtime| runtime.cancellable);
    if let Some(runtime) = runtime {
        view.instance = Some(runtime.instance);
        view.recent_history =
            runtime.history.items.into_iter().map(DocumentApprovalHistoryItemView::from).collect();
        view.history_page = DocumentApprovalHistoryPageView {
            next_cursor: runtime.history.next_cursor,
            has_more: runtime.history.has_more,
        };
    }

    view.allowed_actions = change_actions(db, rbac, change, cancellable, actor, executor).await?;
    Ok(view)
}

// 投影销售变更字段时，当前提交动作沿静态 submit 权限与来源销售写范围独立解释。
async fn change_actions(
    db: &Database,
    rbac: &Arc<AuthorizationService>,
    change: &SalesChangeOrder,
    cancellable: bool,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    let action = match change.stable.status() {
        SalesChangeOrderStatus::Draft | SalesChangeOrderStatus::Rejected => &sales_order_permissions::SUBMIT,
        SalesChangeOrderStatus::InApproval if cancellable => &sales_order_permissions::CANCEL_APPROVAL,
        _ => return Ok(Vec::new()),
    };

    if !rbac.has_permission(actor, &sales_review_permissions::SALES_CHANGE_DRAFT, executor).await? {
        return Ok(Vec::new());
    }
    match SalesAccess::new(db.clone(), rbac.clone())
        .require_object(actor, action, change.sales_order_id.as_ref(), executor)
        .await
    {
        Ok(_) => {},
        Err(Error::Forbidden(_) | Error::NotFound(_)) => return Ok(Vec::new()),
        Err(error) => return Err(error),
    }
    Ok(vec![if action == &sales_order_permissions::SUBMIT { "SUBMIT" } else { "CANCEL" }.into()])
}
