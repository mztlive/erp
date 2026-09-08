//! 读取销售变更事实并组合创建时冻结的审批绑定。

use super::projection::document_approval_view;
use super::{SalesChangeOrderDetailView, SalesChangeReadService};
use crate::{Error, Result};
use erp_sales::entity::sales_review::SalesChangeOrder;
use erp_sales::repository::SalesReviewExt;
use erp_workflow::service::document_registry::find_approval_binding;
use erp_workflow::BpmExt;
use persistence_core::NoTransaction;

impl SalesChangeReadService {
    /// 查询销售变更单详情。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    ///
    /// # 返回
    /// 返回详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单不存在
    pub async fn sales_change_order_detail(&self, id: &str) -> Result<SalesChangeOrderDetailView> {
        let change_order = self
            .db
            .sales_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在".to_string()))?;
        let binding = self.load_change_binding(id).await?;
        // 只读取创建时绑定的定义，不能切换为当前最新发布版本。
        let graph = match binding.as_ref() {
            Some(binding) => Some(
                self.db
                    .bpm_workflow()
                    .load_definition_graph(&binding.approval_process_definition_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::ConflictError("销售变更绑定的审批定义不存在".to_string()))?,
            ),
            None => None,
        };
        let mut view = detail_view(change_order, binding);
        if let (Some(definition), Some(mut graph)) = (view.approval.definition.as_mut(), graph) {
            definition.name = graph.definition.name;
            graph.nodes.sort_by_key(|node| node.display_order);
            definition.nodes = graph
                .nodes
                .into_iter()
                .map(|node| super::dto::DocumentApprovalNodeView {
                    key: node.node_key,
                    name: node.node_name,
                })
                .collect();
        }
        Ok(view)
    }
    /// 读取变更单创建时冻结的审批绑定。未注册时返回空绑定。
    ///
    /// # 错误
    /// 仓储失败时返回错误。
    async fn load_change_binding(
        &self,
        id: &str,
    ) -> Result<Option<erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding>>
    {
        match find_approval_binding(&self.db, id, &mut NoTransaction)
            .await
            .map_err(crate::Error::from)
        {
            Ok(binding) => Ok(binding),
            Err(Error::NotFound(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

/// 构建详情视图，并附带只读审批结构。
///
/// # 参数
/// * `change_order` - 变更单
/// * `binding` - 创建时冻结的定义绑定
///
/// # 返回
/// 返回详情视图。
fn detail_view(
    change_order: SalesChangeOrder,
    binding: Option<erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding>,
) -> SalesChangeOrderDetailView {
    SalesChangeOrderDetailView {
        id: change_order.base.id,
        sales_order_id: change_order.sales_order_id.to_string(),
        base_revision_id: change_order.base_revision_id.to_string(),
        change_type: change_order.change_type,
        reason: change_order.reason,
        status: change_order.stable.status(),
        current_submission_id: change_order
            .current_submission_id
            .as_ref()
            .map(ToString::to_string),
        target_content_hash: change_order.target_content_hash,
        effective_revision_id: change_order
            .effective_revision_id
            .as_ref()
            .map(ToString::to_string),
        version: change_order.base.version,
        created_at: change_order.base.created_at,
        approval: document_approval_view(binding.as_ref(), None, change_order.stable.status()),
    }
}
