//! 域 D13 `sales_order` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 销售域和销售中心的 DTO，禁止重复定义同构类型、禁止直连数据库。

pub mod contract_check;
pub mod evidence;
pub mod selection;

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_order_to_cash::order_to_cash::SalesOrderCommandProcess;
use erp_read_models::sales_center::order::dto::SalesOrderDetailView;
use erp_read_models::sales_center::order::{SalesListParams, SalesListView, SalesOrderReadService};
use erp_sales::dto::sales_order::{
    BindSalesOrderContractRequest, CancelSalesOrderApprovalRequest, CreateSalesOrderRequest,
    HandoverCandidateView, HandoverSalesOrderRequest, HandoverSalesOrderView, SaveWorkingCopyRequest,
    SubmissionView, SubmitSalesOrderRequest, VoidSalesOrderRequest, WorkingCopyView,
};

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::sales::sales_services;

/// 为销售工作台按条件和范围版本分页读取销售单；负责销售筛选只收窄订单，展示姓名来自当前页单据事实。创建人筛选独立，此处不生成负责销售候选。
pub async fn sales_order_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<SalesListParams>,
) -> Result<SalesListView> {
    let page = SalesOrderReadService::new(state.application().db(), state.application().authorization())
        .sales_order_list(&params, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 创建销售单时将订单、稳定明细与工作副本作为一次原子写入；命令携带幂等键，`intent=SUBMIT` 时随建单提交。编排在事务内分别核验合同详情及当前客户选择资格，
/// 不允许选用范围外对象。
pub async fn sales_order_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSalesOrderRequest>,
) -> Result<SalesOrderDetailView> {
    let service = SalesOrderCommandProcess::new(
        state.application().db(),
        state.application().authorization(),
        sales_services(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    );
    let view = service.create_sales_order(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 打开单笔销售单时读取订单、稳定明细、当前草稿及提交和版本历史；将当前操作人交给读模型校验详情访问。
pub async fn sales_order_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SalesOrderDetailView> {
    let view = SalesOrderReadService::new(state.application().db(), state.application().authorization())
        .sales_order_detail(&id, Some(&actor))
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 编辑销售草稿时按期望版本整批替换表头与明细，不将缺失行视为保留。编排在原事务按现有或拟变更归属复验合同与客户资格，成功后返回工作副本。
pub async fn sales_order_save_working_copy(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SaveWorkingCopyRequest>,
) -> Result<WorkingCopyView> {
    let service = SalesOrderCommandProcess::new(
        state.application().db(),
        state.application().authorization(),
        sales_services(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    );
    let view = service.save_working_copy(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 销售草稿完成后，以期望版本和幂等键提交并冻结快照，推进审核轨；同一提交重试返回已有快照。编排在原事务按当前或拟变更归属校验合同与客户资格，越界时拒绝。
pub async fn sales_order_submit(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitSalesOrderRequest>,
) -> Result<SubmissionView> {
    let service = SalesOrderCommandProcess::new(
        state.application().db(),
        state.application().authorization(),
        sales_services(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    );
    let view = service.submit_sales_order(&id, req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 要求说明原因，将仍未最终通过的销售审批退回撤回流程，涵盖 `VoucherSalesOrder`；成功后返回销售单最新详情。
pub async fn sales_order_cancel_approval(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelSalesOrderApprovalRequest>,
) -> Result<SalesOrderDetailView> {
    let view = SalesOrderCommandProcess::new(
        state.application().db(),
        state.application().authorization(),
        sales_services(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
    .cancel_approval_submission(&id, req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 不再继续办理草稿时，按期望版本把销售单从 `DRAFT` 转为 `VOIDED`，并返回作废后的详情。
pub async fn sales_order_void(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<VoidSalesOrderRequest>,
) -> Result<SalesOrderDetailView> {
    let view = SalesOrderCommandProcess::new(
        state.application().db(),
        state.application().authorization(),
        sales_services(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
    .void_sales_order(&id, req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 变更负责销售及业务组织时，按原版本和幂等键提交交接原因及目标，并原子转交现有开放验收任务。审批任务、已完成验收和历史归属不改写；新验收任务使用交接后负责人。
pub async fn sales_order_handover(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<HandoverSalesOrderRequest>,
) -> Result<HandoverSalesOrderView> {
    let view = SalesOrderCommandProcess::new(
        state.application().db(),
        state.application().authorization(),
        sales_services(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
    .handover_sales_order(&id, req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 填销售单交接目标时列出仍有效且具备验收执行资格的账号；候选只作选择提示，提交时重新校验。
pub async fn sales_order_handover_candidates(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<HandoverCandidateView>> {
    let candidates = SalesOrderCommandProcess::new(
        state.application().db(),
        state.application().authorization(),
        sales_services(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
    .handover_candidates(&id, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(candidates))
}

#[cfg(test)]
mod owner_query_tests {
    use axum::http::Uri;
    use erp_contract::dto::contract::ContractListParams;
    use erp_customer::CustomerListParams;
    use erp_procurement::dto::purchase_order::PurchaseOrderListParams;
    use erp_sales::dto::sales_order::SalesOrderListParams;

    use crate::http::extractor::Query;

    // 通过四类业务的实际查询解码器验证账号 ID 去重和旧姓名拒绝；客户、合同与销售支持组织过滤，采购仍须拒绝。
    #[test]
    fn all_four_resources_validate_identity_queries() {
        let valid: Uri = "/?owner_user_ids=user-2,user-1,user-2&page=2".parse().unwrap();
        assert_eq!(
            Query::<CustomerListParams>::try_from_uri(&valid).unwrap().0.owner_user_ids.unwrap().as_slice(),
            &["user-1", "user-2"]
        );
        assert!(Query::<ContractListParams>::try_from_uri(&valid).is_ok());
        assert!(Query::<PurchaseOrderListParams>::try_from_uri(&valid).is_ok());
        assert!(Query::<SalesOrderListParams>::try_from_uri(&valid).is_ok());
        for raw in ["/?owner=Zhang", "/?owner_user_ids=", "/?handler_user_ids=user-1"] {
            let uri: Uri = raw.parse().unwrap();
            assert!(Query::<CustomerListParams>::try_from_uri(&uri).is_err());
            assert!(Query::<ContractListParams>::try_from_uri(&uri).is_err());
            assert!(Query::<PurchaseOrderListParams>::try_from_uri(&uri).is_err());
            assert!(Query::<SalesOrderListParams>::try_from_uri(&uri).is_err());
        }
        let org: Uri = "/?org_unit_ids=org-1".parse().unwrap();
        assert!(Query::<CustomerListParams>::try_from_uri(&org).is_ok());
        assert!(Query::<ContractListParams>::try_from_uri(&org).is_ok());
        assert!(Query::<PurchaseOrderListParams>::try_from_uri(&org).is_err());
        assert!(Query::<SalesOrderListParams>::try_from_uri(&org).is_ok());
    }
}

#[cfg(test)]
mod scope_query_tests {
    use axum::http::Uri;

    use super::*;

    #[test]
    fn sales_scope_version_does_not_break_url_numbers_or_id_filters() {
        let uri: Uri = "/?page=2&page_size=25&scope_version=v1&owner_user_ids=a,b&org_unit_ids=org-1&include_descendants=true&my_todo=true"
            .parse()
            .unwrap();
        let Query(params) = Query::<SalesListParams>::try_from_uri(&uri).unwrap();
        assert_eq!(params.page, Some(2));
        assert_eq!(params.scope_version.as_deref(), Some("v1"));
        assert!(params.owner_user_ids.is_some());
        assert_eq!(params.org_unit_ids.unwrap().as_slice(), &["org-1".to_string()]);
        assert_eq!(params.include_descendants, Some(true));
        let legacy: Uri = "/?owner_name=someone".parse().unwrap();
        assert!(Query::<SalesListParams>::try_from_uri(&legacy).is_err());
    }
}

/// 为原销售单首次补录合同，保留既有客户、结算主体与全部历史商业快照。
/// 返回更新后的销售单详情。
/// 越权、冲突或不符合关联规则时拒绝。
pub async fn sales_order_bind_contract(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<BindSalesOrderContractRequest>,
) -> Result<SalesOrderDetailView> {
    let view = SalesOrderCommandProcess::new(
        state.application().db(),
        state.application().authorization(),
        sales_services(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
    .bind_sales_order_contract(&id, req, &actor)
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}

pub mod timeline;
