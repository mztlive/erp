//! 销售方案开单预填入口。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_order_to_cash::order_to_cash::SalesOrderCommandProcess;
use erp_sales::dto::sales_order::selection_basis::SelectionOrderBasis;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

/// 读取按份采购方案开单内容，不执行写入。
/// # 参数
/// `state` 为应用依赖；`actor` 为认证操作人；`id` 为方案身份。
/// # 返回
/// 返回客户、冻结报价及独立 SKU 行。
/// # 错误
/// 无创建权限、方案或客户不可访问、非按份采购时拒绝。
pub async fn selection_basis(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SelectionOrderBasis> {
    let basis = SalesOrderCommandProcess::with_rbac(
        state.db(),
        state.rbac(),
        crate::wiring::sales::sales_services(state.db(), state.rbac()),
    )
    .selection_order_basis(&id, &actor)
    .await?;
    Ok(ApiResponse::ok_with_data(basis))
}

#[cfg(test)]
mod tests {
    use crate::permissions;

    #[test]
    fn basis_requires_sales_creation_permission() {
        assert_eq!(
            permissions::sales_order::SELECTION_SELECTION_BASIS.permission().to_string(),
            "sales_order:create"
        );
    }
}
