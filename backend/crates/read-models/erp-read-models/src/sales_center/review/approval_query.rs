//! 销售变更真实审批运行投影与当前经办人允许动作。

use application_core::AuditActor;
use erp_identity::SharedRbacService;
use erp_sales::entity::sales_review::{SalesChangeOrder, SalesChangeOrderStatus};
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use mongodb::Database;
use persistence_core::Executor;

use super::dto::{DocumentApprovalHistoryItemView, DocumentApprovalHistoryPageView, DocumentApprovalView};
use super::projection::document_approval_view;
use crate::approval_runtime::{has_document_permission, load_document_runtime};
use crate::sales_center::access::SalesAccess;
use crate::{Error, Result};

/// 读取已授权销售变更的实际实例、历史和当前操作者允许动作。
///
/// # 参数
/// * `db` - 原业务读取数据库
/// * `rbac` - 当前共享权限服务
/// * `change` - 已沿来源销售单 detail 授权的变更单
/// * `binding` - 原单冻结的审批定义绑定
/// * `actor` - 当前已认证操作人
/// * `executor` - 原授权和详情读取执行器
/// # 返回
/// 返回真实运行摘要，只有当前责任、静态权限和运行规则均允许时带出操作。
/// # 错误
/// 运行事实损坏、当前资格读取或仓储失败时拒绝。
pub(super) async fn load_change_document_approval(
    db: &Database,
    rbac: &SharedRbacService,
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
    view.allowed_actions = change_actions(db, rbac, change, &view, cancellable, actor, executor).await?;
    Ok(view)
}

/// 当前提交动作沿静态 submit 权限与来源销售写范围独立解释。
async fn change_actions(
    db: &Database,
    rbac: &SharedRbacService,
    change: &SalesChangeOrder,
    view: &DocumentApprovalView,
    cancellable: bool,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    let action = match change.stable.status() {
        SalesChangeOrderStatus::Draft => "submit",
        SalesChangeOrderStatus::InApproval if cancellable => "cancel_approval",
        _ => return Ok(Vec::new()),
    };
    if !has_document_permission(db, rbac, actor, "sales_change_order", "submit", executor).await? {
        return Ok(Vec::new());
    }
    let order = match SalesAccess::new(db.clone(), rbac.clone())
        .require_object(actor, action, change.sales_order_id.as_ref(), &[], executor)
        .await
    {
        Ok(order) => order,
        Err(Error::Forbidden(_) | Error::NotFound(_)) => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let submitter = view.instance.as_ref().and_then(|instance| instance.started_by.as_deref());
    if !action_owner(actor.id(), &order.sales_owner_user_id, &change.stable.created_by, submitter) {
        return Ok(Vec::new());
    }
    Ok(vec![if action == "submit" { "SUBMIT" } else { "CANCEL" }.into()])
}

/// 普通详情读取或历史参与不能代替当前负责人、原创建人或原提交人。
fn action_owner(actor: &str, owner: &str, creator: &str, submitter: Option<&str>) -> bool {
    !actor.is_empty() && (actor == owner || actor == creator || submitter == Some(actor))
}

#[cfg(test)]
mod tests {
    use super::action_owner;

    /// 有详情范围的其他人员不会被投影为原单取消经办人。
    #[test]
    fn sales_change_actions_require_current_or_original_responsibility() {
        assert!(action_owner("owner", "owner", "creator", Some("submitter")));
        assert!(action_owner("creator", "owner", "creator", Some("submitter")));
        assert!(action_owner("submitter", "owner", "creator", Some("submitter")));
        assert!(!action_owner("reader", "owner", "creator", Some("submitter")));
        assert!(!action_owner("", "", "", Some("")));
    }
}
