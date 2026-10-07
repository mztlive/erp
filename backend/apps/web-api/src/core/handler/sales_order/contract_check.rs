//! 后补合同只读预检；实际补录仍在事务内重新核对。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, Query, State};
use erp_processes::order_to_cash::SalesOrderCommandProcess;
use erp_sales::dto::sales_order::BindSalesOrderContractRequest;
use erp_sales::entity::sales_order::contract_terms::ContractBindingCheck;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "销售单",
    group_desc = "销售单（W05）管理",
    desc = "核对后补销售合同",
    resource = "sales_order",
    action = "update"
)]
/// 返回原单与所选合同的商业条款差异。
///
/// # 参数
/// * `state` / `actor` - 应用状态及当前用户
/// * `id` / `req` - 原单身份、版本及合同有效修订
/// # 返回
/// 返回条款来源和逐项核对结果。
/// # 错误
/// 无权修改、版本变化或关联不合法时拒绝。
pub async fn sales_order_contract_check(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(req): Query<BindSalesOrderContractRequest>,
) -> Result<ContractBindingCheck> {
    let check = SalesOrderCommandProcess::with_rbac(state.db(), state.rbac())
        .with_object_read(state.approval_object_read())
        .check_sales_order_contract(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(check))
}

#[cfg(test)]
mod tests {
    use axum::http::Uri;

    use super::*;

    #[test]
    fn preflight_requires_order_and_contract_versions() {
        let valid = Uri::from_static("/?version=2&contract_id=c1&requested_contract_revision_id=r1");
        let Query(req) = Query::<BindSalesOrderContractRequest>::try_from_uri(&valid).unwrap();
        assert_eq!(req.version, 2);
        assert_eq!(req.requested_contract_revision_id.as_ref(), "r1");
        for uri in
            ["/?contract_id=c1", "/?version=2&contract_id=c1&requested_contract_revision_id=r1&tax_point=6"]
        {
            assert!(Query::<BindSalesOrderContractRequest>::try_from_uri(&uri.parse().unwrap()).is_err());
        }
    }

    #[test]
    fn preflight_uses_sales_update_permission() {
        assert_eq!(sales_order_contract_check_permission_key().to_string(), "sales_order:update");
    }
}
