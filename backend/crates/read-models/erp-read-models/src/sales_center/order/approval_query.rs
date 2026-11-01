//! 销售单详情审批实例与最近历史投影。
//!
//! 查询按主体读取最新实例，再映射为单据只读审批结构；不得在此推导下一节点。

use erp_sales::entity::sales_order::{BusinessType, CommercialStatus, ReviewStatus};
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use mongodb::Database;
use persistence_core::NoTransaction;

use super::adapter::{RECENT_HISTORY_LIMIT, document_approval_view_with_history};
use super::dto::DocumentApprovalView;
use crate::Result;
use crate::approval_runtime::{binding_route, load_approval_summary};

/// 加载销售单详情的只读审批结构。
///
/// 未提交时实例为空；已启动后填入最新实例摘要与有界历史。
///
/// # 参数
/// * `db` - 数据库
/// * `business_type` - 用于分派 `SalesOrder` / `VoucherSalesOrder` 主体
/// * `sales_order_id` - 销售单主键
/// * `binding` - 创建时冻结的定义绑定
/// * `commercial` - 当前商业主状态
/// * `review` - 当前审核轨
///
/// # 返回
/// 返回绑定、可选实例与最近历史。
///
/// # 错误
/// 主体引用非法或仓储失败时返回错误。
#[tracing::instrument(
    name = "sales_order.load_document_approval",
    skip_all,
    fields(layer = "service", domain = "sales_order", operation = "load_document_approval")
)]
pub(super) async fn load_document_approval(
    db: &Database,
    business_type: BusinessType,
    sales_order_id: &str,
    binding: Option<&ApprovalDefinitionBinding>,
    commercial: CommercialStatus,
    review: ReviewStatus,
) -> Result<DocumentApprovalView> {
    let subject = super::subject_ref_for_sales_business(business_type, sales_order_id)
        .map_err(|error| crate::Error::ValidationError(error.to_string()))?;
    let runtime = load_approval_summary(
        db,
        &subject,
        u32::try_from(RECENT_HISTORY_LIMIT).expect("审批历史首屏条数在 u32 范围内"),
        &mut NoTransaction,
    )
    .await?;
    let mut view = document_approval_view_with_history(
        binding,
        runtime.instance,
        runtime.recent_history,
        runtime.history_page,
        commercial,
        review,
    );
    if let (Some(binding), Some(definition)) = (binding, view.definition.as_mut()) {
        let (name, nodes) = binding_route(db, binding, &mut NoTransaction).await?;
        definition.name = name;
        definition.nodes = nodes;
    }
    Ok(view)
}
