//! 域 D15 `purchase_order` 的 HTTP handler。
//!
//! Handler 只做协议适配：校验请求、调用采购命令流程或读模型、返回 `ApiResponse`。
//! DTO 使用采购领域与读模型的唯一定义，禁止重复定义同构类型、禁止直连数据库。

pub mod change_draft;
pub mod change_void;
pub mod correspondence;
pub mod delivery_options;
mod deposit_summary;
pub mod expense;
pub mod freight;
pub mod payment_arrangement;
pub mod payments;
pub mod sales_material;

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_procure_to_pay::procure_to_pay::PurchaseOrderProcess;
use erp_procurement::PurchaseDataScope;
use erp_procurement::dto::purchase_order::{
    CancelPurchaseChangeApprovalRequest, CancelPurchaseOrderApprovalRequest,
    CreatePurchaseOrderFromBasisRequest, CreatePurchaseOrderResult, CreatePurchaseOrdersFromSourcingRequest,
    CreatePurchaseOrdersFromSourcingResult, EffectPurchaseChangeRequest, MaintainPurchaseSourceRequest,
    PurchaseChangeEffectResult, PurchaseChangeOrderListParams, PurchaseOrderListParams,
    SavePurchaseOrderDraftRequest, SavePurchaseOrderDraftResult, StartPurchaseChangeRequest,
    StartPurchaseChangeResult, SubmitPurchaseChangeRequest, SubmitPurchaseOrderRequest,
    SubmitPurchaseOrderResult, VoidPurchaseOrderRequest, VoidPurchaseOrderResult,
};
use erp_procurement::service::purchase_order::PurchaseOrderService;
use erp_read_models::purchase_center::dto::{
    CreationBasisListParams, CreationBasisView, PurchaseChangeOrderView, PurchaseOrderCenterView,
};
use erp_read_models::purchase_center::{PurchaseChangeListView, PurchaseListView, PurchaseOrderReadService};

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::purchase_ports;

// 构造已注入采购范围 Port 的只读服务。
// 返回可解析采购范围的读服务。
//
// HTTP 列表、详情、候选、导出和变更单必须经此入口，不得回退失败关闭 Port。
fn purchase_reads(state: &HttpState) -> PurchaseOrderReadService {
    PurchaseOrderReadService::new(
        state.application().db(),
        state.application().authorization(),
        PurchaseDataScope::shared(state.application().db(), state.application().authorization()),
    )
}

/// 用认证账号和筛选条件分页读取采购单；跨页携带 `scope_version`，范围变化时拒绝。缺动作权限返回 403，缺范围返回 `no_scope` 空集；
/// 负责人筛选只收窄，不产生候选或补公司范围。
pub async fn purchase_order_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<PurchaseOrderListParams>,
) -> Result<PurchaseListView> {
    let page = purchase_reads(&state).purchase_order_list(&params, &actor).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 打开采购对象中心时独立核验详情权限及对象可见性，不复用列表授权作凭证。不可见时隐藏对象是否存在，组装期间范围变化返回冲突。
pub async fn purchase_order_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<PurchaseOrderCenterView> {
    let mut view = purchase_reads(&state).purchase_order_detail(&id, Some(&actor)).await?;
    deposit_summary::attach(&state, &actor, &mut view).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 采购确认后按建单依据、采购类型、付款条件和幂等键创建采购单并启动审批；同一拆单维度复用已有结果。
pub async fn purchase_order_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreatePurchaseOrderFromBasisRequest>,
) -> Result<CreatePurchaseOrderResult> {
    let view = PurchaseOrderProcess::new(
        state.application().db(),
        state.application().authorization(),
        purchase_ports(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
    .create_from_basis(req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 完成销售供给分配时，按逐行库存或采购依据和数量一次确认预占与采购。以原任务和幂等键重试，同键同载荷回放原结果。
pub async fn purchase_order_create_from_sourcing(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreatePurchaseOrdersFromSourcingRequest>,
) -> Result<CreatePurchaseOrdersFromSourcingResult> {
    let view = PurchaseOrderProcess::new(
        state.application().db(),
        state.application().authorization(),
        purchase_ports(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
    .create_from_sourcing(req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 办理中的采购草稿保存整份表头和明细，并返回新版本及汇总；期望版本冲突返回 409，防止覆盖并发修改。
pub async fn purchase_order_save_draft(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SavePurchaseOrderDraftRequest>,
) -> Result<SavePurchaseOrderDraftResult> {
    let view = PurchaseOrderProcess::new(
        state.application().db(),
        state.application().authorization(),
        purchase_ports(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
    .save_draft(&id, req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 取消未继续办理的采购草稿时，按版本、原因和幂等键作废，同时释放原销售采购覆盖；返回更新后的状态及版本。
pub async fn purchase_order_void(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<VoidPurchaseOrderRequest>,
) -> Result<VoidPurchaseOrderResult> {
    let view = PurchaseOrderProcess::new(
        state.application().db(),
        state.application().authorization(),
        purchase_ports(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
    .void_draft(&id, req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 草稿就绪时，以期望版本和幂等键提交采购单至统一审批，返回提交标识、序号及审核待办。
pub async fn purchase_order_submit(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitPurchaseOrderRequest>,
) -> Result<SubmitPurchaseOrderResult> {
    let view = PurchaseOrderProcess::new(
        state.application().db(),
        state.application().authorization(),
        purchase_ports(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
    .submit(&id, req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 销售变更影响采购但无法调整时，登记维持原采购。
/// `req` 为记录期望版本与原因。
/// 登记成功返回空数据。
pub async fn purchase_order_maintain_source(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<MaintainPurchaseSourceRequest>,
) -> Result<()> {
    PurchaseOrderProcess::new(
        state.application().db(),
        state.application().authorization(),
        purchase_ports(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
    .maintain_source(&id, req, &actor)
    .await?;
    Ok(ApiResponse::ok())
}

/// 在采购审批尚未最终通过时，按请求中的必填原因发起撤回；成功响应不带业务数据。
pub async fn purchase_order_cancel_approval(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelPurchaseOrderApprovalRequest>,
) -> Result<()> {
    PurchaseOrderProcess::new(
        state.application().db(),
        state.application().authorization(),
        purchase_ports(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
    .cancel_approval(&id, req, &actor)
    .await?;

    Ok(ApiResponse::ok())
}

/// 查询当前账号开放任务范围内的库存与采购供给依据。
/// `params` 为可选销售单和供给分配任务筛选。
/// 返回当前账号可处理的精确库存与采购供给依据。
pub async fn purchase_creation_basis_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<CreationBasisListParams>,
) -> Result<Vec<CreationBasisView>> {
    let views = purchase_reads(&state).creation_basis_list(&params, &actor).await?;

    Ok(ApiResponse::ok_with_data(views))
}

/// 发起采购变更（基于当前生效版本创建变更单）。
/// 返回变更单结果。
pub async fn purchase_change_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<StartPurchaseChangeRequest>,
) -> Result<StartPurchaseChangeResult> {
    let view = PurchaseOrderProcess::new(
        state.application().db(),
        state.application().authorization(),
        purchase_ports(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
    .start_change(&id, req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 采购变更草稿确认后，按路径变更单和当前身份提交统一审批；该命令不等于直接将变更生效。
pub async fn purchase_change_submit(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SubmitPurchaseChangeRequest>,
) -> Result<PurchaseChangeOrderView> {
    let view = PurchaseOrderProcess::new(
        state.application().db(),
        state.application().authorization(),
        purchase_ports(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
    .submit_change_view(&id, req, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 保留旧采购变更直接生效入口，但恒返回冲突，不改写采购单。只有审批运行时的 `on_final_approve` 可以触发最终动作。
pub async fn purchase_change_effect(
    State(_state): State<HttpState>,
    Extension(_actor): Extension<AuditActor>,
    Path(_id): Path<String>,
    Json(_req): Json<EffectPurchaseChangeRequest>,
) -> Result<PurchaseChangeEffectResult> {
    match PurchaseOrderService::reject_client_effect() {
        Err(error) => Err(error.into()),
        Ok(result) => Ok(ApiResponse::ok_with_data(result)),
    }
}

/// 撤回尚未最终通过的采购变更审批。
/// `req` 为撤回请求（原因必填）。
/// 撤回成功返回空数据。
pub async fn purchase_change_cancel_approval(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CancelPurchaseChangeApprovalRequest>,
) -> Result<()> {
    PurchaseOrderProcess::new(
        state.application().db(),
        state.application().authorization(),
        purchase_ports(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
    .cancel_change_approval(&id, req, &actor)
    .await?;

    Ok(ApiResponse::ok())
}

/// 按请求条件查询采购变更单列表。
/// `query` 为分页与筛选参数（扁平传递，含跨页 `scope_version`）。
/// 返回带范围版本的分页视图。
/// 无动作权限、范围变化、筛选非法或仓储失败时拒绝。
///
/// 沿来源采购单责任接入；缺范围返回空集并标记 `no_scope`。
pub async fn purchase_change_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<PurchaseChangeOrderListParams>,
) -> Result<PurchaseChangeListView> {
    let page = purchase_reads(&state).change_order_list(&params, &actor).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 查询采购变更单详情。
/// 返回变更单视图。
/// 无动作权限、不可见或不存在时拒绝。
///
/// 列表已授权不能作为详情凭证；沿来源采购单 detail 动作重验。
pub async fn purchase_change_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<PurchaseChangeOrderView> {
    let view = purchase_reads(&state).change_order_detail(&id, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[cfg(test)]
mod scope_query_tests {
    use axum::http::Uri;
    use erp_procurement::dto::purchase_order::PurchaseOrderListParams;

    use crate::http::extractor::Query;

    // 跨页必须能解码范围版本，且不得把组织筛选静默当成全量。
    //
    // 未接入的 `org_unit_ids` 必须拒绝，不得忽略后查全量。
    #[test]
    fn purchase_scope_version_is_consumed_and_org_filter_stays_unsupported() {
        let uri: Uri = "/?page=2&page_size=25&scope_version=v1&owner_user_ids=a,b".parse().unwrap();
        let query = Query::<PurchaseOrderListParams>::try_from_uri(&uri).unwrap().0;
        assert_eq!(query.scope_version.as_deref(), Some("v1"));
        assert_eq!(query.page, Some(2));
        let org: Uri = "/?org_unit_ids=org-1".parse().unwrap();
        assert!(Query::<PurchaseOrderListParams>::try_from_uri(&org).is_err());
    }
}
