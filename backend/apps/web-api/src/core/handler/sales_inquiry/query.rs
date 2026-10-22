//! 销售、采购分别使用独立白名单响应，不提供共享完整实体入口。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_processes::sales_inquiry::read::{
    InquiryListParams, InquiryListView, InquiryReadService, InquiryTaskParams, InquiryTemplateProduct,
    InquiryTemplateProductsRequest, ProcurementInquiryDetail,
};
use erp_sales::dto::sales_inquiry::InquirySalesView;
use erp_supply::inquiry::InquiryQuote;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::{Json, Query};
use crate::core::response::ApiResponse;

fn service(state: &AppState) -> InquiryReadService {
    InquiryReadService::new(state.db(), state.rbac())
}

#[permission_macros::permission(
    group = "销售询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "查询销售询价列表",
    resource = "sales_inquiry",
    action = "list"
)]
/// 查询销售询价列表。
/// # 参数
/// `state` 为应用，`actor` 为认证账号，其余参数限定当前对象或分页。
/// # 返回
/// 返回本视图允许读取的独立响应。
/// # 错误
/// 无权限、对象不可见、任务责任失效、参数或范围版本变化时拒绝。
pub async fn sales_list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<InquiryListParams>,
) -> Result<InquiryListView<InquirySalesView>> {
    Ok(ApiResponse::ok_with_data(service(&state).sales_list(&actor, &params).await?))
}

#[permission_macros::permission(
    group = "销售询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "读取销售询价详情",
    resource = "sales_inquiry",
    action = "detail"
)]
/// 读取销售询价详情。
/// # 参数
/// `state` 为应用，`actor` 为认证账号，其余参数限定当前对象或分页。
/// # 返回
/// 返回本视图允许读取的独立响应。
/// # 错误
/// 无权限、对象不可见、任务责任失效、参数或范围版本变化时拒绝。
pub async fn sales_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<InquirySalesView> {
    Ok(ApiResponse::ok_with_data(service(&state).sales_detail(&actor, &id).await?))
}

#[permission_macros::permission(
    group = "采购询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "查询当前采购询价任务",
    resource = "procurement_inquiry",
    action = "list"
)]
/// 查询当前采购询价任务。
/// # 参数
/// `state` 为应用，`actor` 为认证账号，其余参数限定当前对象或分页。
/// # 返回
/// 返回本视图允许读取的独立响应。
/// # 错误
/// 无权限、对象不可见、任务责任失效、参数或范围版本变化时拒绝。
pub async fn procurement_list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<InquiryListParams>,
) -> Result<InquiryListView<ProcurementInquiryDetail>> {
    Ok(ApiResponse::ok_with_data(service(&state).procurement_list(&actor, &params).await?))
}

#[permission_macros::permission(
    group = "采购询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "读取当前采购询价责任资料",
    resource = "procurement_inquiry",
    action = "detail"
)]
/// 读取当前采购询价责任资料。
/// # 参数
/// `state` 为应用，`actor` 为认证账号，其余参数限定当前对象或分页。
/// # 返回
/// 返回本视图允许读取的独立响应。
/// # 错误
/// 无权限、对象不可见、任务责任失效、参数或范围版本变化时拒绝。
pub async fn procurement_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(params): Query<InquiryTaskParams>,
) -> Result<ProcurementInquiryDetail> {
    Ok(ApiResponse::ok_with_data(service(&state).procurement_detail(&actor, &id, &params.task_id).await?))
}

#[permission_macros::permission(
    group = "采购询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "读取当前责任行内部报价",
    resource = "procurement_inquiry",
    action = "internal_quote"
)]
/// 读取当前责任行内部报价。
/// # 参数
/// `state` 为应用，`actor` 为认证账号，其余参数限定当前对象或分页。
/// # 返回
/// 返回本视图允许读取的独立响应。
/// # 错误
/// 无权限、对象不可见、任务责任失效、参数或范围版本变化时拒绝。
pub async fn procurement_internal_quotes(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(params): Query<InquiryTaskParams>,
) -> Result<Vec<InquiryQuote>> {
    Ok(ApiResponse::ok_with_data(service(&state).internal_quotes(&actor, &id, &params.task_id).await?))
}

#[permission_macros::permission(
    group = "销售询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "重验客户模板商品资格",
    resource = "sales_inquiry",
    action = "create"
)]
/// 重验客户模板商品资格。
/// # 参数
/// `state` 为应用，`actor` 为认证账号，其余参数限定当前对象或分页。
/// # 返回
/// 返回本视图允许读取的独立响应。
/// # 错误
/// 无权限、对象不可见、任务责任失效、参数或范围版本变化时拒绝。
pub async fn sales_template_products(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<InquiryTemplateProductsRequest>,
) -> Result<Vec<InquiryTemplateProduct>> {
    Ok(ApiResponse::ok_with_data(service(&state).template_products(&actor, &request.sku_ids).await?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn procurement_list_accepts_only_known_task_filters() {
        for value in [r#"{}"#, r#"{"task_status":"open"}"#, r#"{"task_status":"completed"}"#] {
            assert!(serde_json::from_str::<InquiryListParams>(value).is_ok());
        }
        assert!(serde_json::from_str::<InquiryListParams>(r#"{"task_status":"closed"}"#).is_err());
        assert!(serde_json::from_str::<InquiryListParams>(r#"{"owner_user_id":"other"}"#).is_err());
    }
    #[test]
    fn sales_and_internal_quote_routes_use_independent_permissions() {
        assert_eq!(sales_detail_permission_key().to_string(), "sales_inquiry:detail");
        assert_eq!(procurement_detail_permission_key().to_string(), "procurement_inquiry:detail");
        assert_eq!(
            procurement_internal_quotes_permission_key().to_string(),
            "procurement_inquiry:internal_quote"
        );
    }
    #[test]
    fn procurement_detail_requires_explicit_task_and_rejects_payload_fields() {
        assert!(serde_json::from_str::<InquiryTaskParams>(r#"{}"#).is_err());
        assert!(serde_json::from_str::<InquiryTaskParams>(r#"{"task_id":"t","supplier_id":"s"}"#).is_err());
        assert!(serde_json::from_str::<InquiryTaskParams>(r#"{"task_id":"t"}"#).is_ok());
    }
}
