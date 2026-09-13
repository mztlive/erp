//! 域 D13 `sales_order` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 销售域和销售中心的 DTO，禁止重复定义同构类型、禁止直连数据库。

use application_core::AuditActor;
use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use erp_sales::dto::sales_order::{
    CancelSalesOrderApprovalRequest, CreateSalesOrderRequest, SalesOrderListParams, SaveWorkingCopyRequest,
    SubmissionView, SubmitSalesOrderRequest, VoidSalesOrderRequest, WorkingCopyView,
};

use erp_processes::order_to_cash::SalesOrderCommandProcess;
use erp_read_models::sales_center::order::{
    dto::{SalesOrderDetailView, SalesOrderView},
    SalesOrderReadService,
};

use crate::{
    app_state::AppState,
    core::{
        errors::Result, extractor::UserID, handler::customer::ensure_customer_access,
        middleware::RbacSubject, response::ApiResponse,
    },
};

#[permission_macros::permission(
    group = "销售单",
    group_desc = "销售单（W05）管理",
    desc = "查询销售单列表",
    resource = "sales_order",
    action = "list"
)]
/// 查询销售单列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn sales_order_list(
    State(state): State<AppState>,
    Query(params): Query<SalesOrderListParams>,
) -> Result<application_core::FilteredPage<SalesOrderView>> {
    let page = SalesOrderReadService::new(state.db())
        .sales_order_list(&params)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "销售单",
    group_desc = "销售单（W05）管理",
    desc = "创建销售单",
    resource = "sales_order",
    action = "create"
)]
/// 创建销售单（订单 + 稳定明细 + 工作副本原子形成；`intent=SUBMIT` 时立即提交）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（含幂等键与意图）
///
/// # 返回
/// 返回销售单详情视图。
pub async fn sales_order_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Extension(subject): Extension<RbacSubject>,
    Extension(UserID(user_id)): Extension<UserID>,
    Json(req): Json<CreateSalesOrderRequest>,
) -> Result<SalesOrderDetailView> {
    let service = SalesOrderCommandProcess::with_rbac(state.db(), state.rbac())
        .with_object_read(state.approval_object_read());
    let customer_id = service.sales_command_customer_id(&req.contract_id).await?;
    ensure_customer_access(&state, &subject, &user_id, customer_id.as_ref()).await?;
    let view = service.create_sales_order(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售单",
    group_desc = "销售单（W05）管理",
    desc = "查询销售单详情",
    resource = "sales_order",
    action = "detail"
)]
/// 查询销售单详情（订单 + 稳定明细 + 草稿 + 提交历史 + 版本历史）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 销售单 ID
///
/// # 返回
/// 返回详情视图。
pub async fn sales_order_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SalesOrderDetailView> {
    let view = SalesOrderReadService::with_rbac(state.db(), state.rbac())
        .sales_order_detail(&id, Some(&actor))
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售单",
    group_desc = "销售单（W05）管理",
    desc = "保存销售单草稿",
    resource = "sales_order",
    action = "update"
)]
/// 保存草稿（整表头覆盖 + 明细整批替换，乐观锁）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 销售单 ID
/// * `req` - 保存请求（含期望版本）
///
/// # 返回
/// 返回保存后的工作副本视图。
pub async fn sales_order_save_working_copy(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SaveWorkingCopyRequest>,
) -> Result<WorkingCopyView> {
    let view = SalesOrderCommandProcess::new(state.db())
        .with_object_read(state.approval_object_read())
        .save_working_copy(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售单",
    group_desc = "销售单（W05）管理",
    desc = "提交销售单",
    resource = "sales_order",
    action = "submit"
)]
/// 提交销售单（冻结提交快照并推进审核轨；重复提交幂等返回既有提交）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 销售单 ID
/// * `req` - 提交请求（含期望版本与幂等键）
///
/// # 返回
/// 返回提交快照视图。
pub async fn sales_order_submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitSalesOrderRequest>,
) -> Result<SubmissionView> {
    let view = SalesOrderCommandProcess::with_rbac(state.db(), state.rbac())
        .with_object_read(state.approval_object_read())
        .submit_sales_order(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售单",
    group_desc = "销售单（W05）管理",
    desc = "撤回销售单审批",
    resource = "sales_order",
    action = "cancel_approval"
)]
/// 撤回尚未最终通过的销售单审批（含 `VoucherSalesOrder`）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 销售单 ID
/// * `req` - 撤回请求（原因必填）
///
/// # 返回
/// 返回撤回后的销售单详情。
pub async fn sales_order_cancel_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelSalesOrderApprovalRequest>,
) -> Result<SalesOrderDetailView> {
    let view = SalesOrderCommandProcess::with_rbac(state.db(), state.rbac())
        .with_object_read(state.approval_object_read())
        .cancel_approval_submission(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售单",
    group_desc = "销售单（W05）管理",
    desc = "作废销售单草稿",
    resource = "sales_order",
    action = "delete"
)]
/// 作废销售单草稿（主状态 `DRAFT → VOIDED`，乐观锁）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 销售单 ID
/// * `req` - 作废请求（含期望版本）
///
/// # 返回
/// 返回作废后的销售单详情视图。
pub async fn sales_order_void(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<VoidSalesOrderRequest>,
) -> Result<SalesOrderDetailView> {
    let view = SalesOrderCommandProcess::new(state.db())
        .with_object_read(state.approval_object_read())
        .void_sales_order(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[cfg(test)]
mod tests {
    /// HTTP 构造审批命令时必须同时接入授权源和对象读取端口，禁止退回未接线默认值。
    #[test]
    fn approval_commands_wire_object_read_port() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        let constructors: Vec<_> = production
            .split("SalesOrderCommandProcess::with_rbac(")
            .skip(1)
            .collect();
        assert!(!constructors.is_empty());
        for constructor in constructors {
            let statement = constructor.split(';').next().expect("构造语句");
            assert!(statement.contains(".with_object_read(state.approval_object_read())"));
        }
    }

    /// HTTP 调用方对卡券销售单只走统一提交/撤回，不得新增专用决定入口。
    #[test]
    fn voucher_sales_order_http_uses_unified_ports() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("submit_sales_order"));
        assert!(production.contains("cancel_approval_submission"));
        assert!(production.contains("VoucherSalesOrder"));
        assert!(!production.contains("CARD_SALES_APPROVAL"));
        assert!(!production.contains("CardSalesManagerApproval"));
        assert!(!production.contains("CardSalesOperationApproval"));
        assert!(!production.contains("InternalApprovalRuntime"));
    }
}

#[cfg(test)]
mod owner_query_tests {
    use axum::{extract::Query, http::Uri};
    use erp_contract::dto::contract::ContractListParams;
    use erp_customer::CustomerListParams;
    use erp_procurement::dto::purchase_order::PurchaseOrderListParams;
    use erp_sales::dto::sales_order::SalesOrderListParams;

    /// 实际 Query 解码器支持去重 ID，拒绝旧姓名和未接入组织参数，防止忽略后查全量。
    #[test]
    fn all_four_resources_validate_identity_queries() {
        let valid: Uri = "/?owner_user_ids=user-2,user-1,user-2&page=2".parse().unwrap();
        assert_eq!(
            Query::<CustomerListParams>::try_from_uri(&valid)
                .unwrap()
                .0
                .owner_user_ids
                .unwrap()
                .as_slice(),
            &["user-1", "user-2"]
        );
        assert!(Query::<ContractListParams>::try_from_uri(&valid).is_ok());
        assert!(Query::<PurchaseOrderListParams>::try_from_uri(&valid).is_ok());
        assert!(Query::<SalesOrderListParams>::try_from_uri(&valid).is_ok());
        for raw in [
            "/?owner=Zhang",
            "/?owner_user_ids=",
            "/?org_unit_ids=org-1",
            "/?handler_user_ids=user-1",
        ] {
            let uri: Uri = raw.parse().unwrap();
            assert!(Query::<CustomerListParams>::try_from_uri(&uri).is_err());
            assert!(Query::<ContractListParams>::try_from_uri(&uri).is_err());
            assert!(Query::<PurchaseOrderListParams>::try_from_uri(&uri).is_err());
            assert!(Query::<SalesOrderListParams>::try_from_uri(&uri).is_err());
        }
    }
}
