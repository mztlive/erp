//! 销售选品 HTTP handler。

mod copy;
mod pricing;
pub mod protection;

use std::net::SocketAddr;

use application_core::AuditActor;
use axum::Extension;
use axum::body::Body;
use axum::extract::{ConnectInfo, Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::Response;
pub use copy::booklet_copy;
use erp_order_to_cash::sales_selection::SalesSelectionProcess;
use erp_read_models::sales_center::selection::{
    SalesSelectionReadService, SelectionBookletListView, SelectionProposalListView,
};
use erp_sales::dto::sales_selection::{
    CopyLinkView, CreateSalesSelectionBookletRequest, DeleteDisplayItemRequest, PrepareSalesSelectionRequest,
    PublicSelectionPageView, PublishSalesSelectionRequest, SalesSelectionBookletListParams,
    SalesSelectionBookletView, SalesSelectionCommandRequest, SalesSelectionProposalListParams,
    SalesSelectionProposalView, SalesSelectionSessionView, SaveSelectionSessionRequest,
    SubmitSelectionSessionRequest,
};
pub use pricing::booklet_update_prices;
use serde::Deserialize;

use crate::http::errors::{Error, Result};
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::sales::sales_services;

fn process(state: &HttpState) -> SalesSelectionProcess {
    SalesSelectionProcess::new(
        state.application().db(),
        state.application().storage().clone(),
        state.application().config_snapshot().app.secret.as_bytes(),
        sales_services(
            state.application().db(),
            state.application().rbac(),
            state.application().authorization(),
        ),
    )
}

// 为只读入口复用当前选品范围与目录 Port。
// 只读选品册及方案查询服务。
fn read_service(state: &HttpState) -> SalesSelectionReadService {
    let services = sales_services(
        state.application().db(),
        state.application().rbac(),
        state.application().authorization(),
    );
    SalesSelectionReadService::new(
        state.application().db(),
        services.selections.clone(),
        services.catalog.clone(),
    )
}

/// 按请求条件查询选品册列表。
pub async fn booklet_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<SalesSelectionBookletListParams>,
) -> Result<SelectionBookletListView> {
    Ok(ApiResponse::ok_with_data(read_service(&state).booklet_list(params, actor).await?))
}

/// 查询选品册详情。
pub async fn booklet_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SalesSelectionBookletView> {
    let view = read_service(&state).booklet_detail(&id, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 创建选品册并排队首次准备。
/// 返回准备中详情。
pub async fn booklet_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSalesSelectionBookletRequest>,
) -> Result<SalesSelectionBookletView> {
    Ok(ApiResponse::ok_with_data(process(&state).create(req, actor).await?))
}

/// 启动准备。
/// 返回准备中详情。
pub async fn booklet_prepare(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<PrepareSalesSelectionRequest>,
) -> Result<SalesSelectionBookletView> {
    Ok(ApiResponse::ok_with_data(process(&state).start_prepare(id, req, actor).await?))
}

/// 删除陈列项。
pub async fn booklet_delete_item(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, item_id)): Path<(String, String)>,
    Query(req): Query<DeleteDisplayItemRequest>,
) -> Result<SalesSelectionBookletView> {
    Ok(ApiResponse::ok_with_data(process(&state).delete_display_item(id, item_id, req, actor).await?))
}

/// 删除陈列项（POST 兼容路径）。
pub async fn booklet_delete_item_post(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, item_id)): Path<(String, String)>,
    Json(req): Json<DeleteDisplayItemRequest>,
) -> Result<SalesSelectionBookletView> {
    Ok(ApiResponse::ok_with_data(process(&state).delete_display_item(id, item_id, req, actor).await?))
}

/// 发布选品册。
/// 返回含链接的详情。
pub async fn booklet_publish(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<PublishSalesSelectionRequest>,
) -> Result<SalesSelectionBookletView> {
    Ok(ApiResponse::ok_with_data(process(&state).publish(id, req, actor).await?))
}

/// 复制当前链接。
/// 返回含路径的详情。
pub async fn booklet_copy_link(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SalesSelectionBookletView> {
    Ok(ApiResponse::ok_with_data(process(&state).copy_link(&id, &actor).await?))
}

/// 复制当前链接相对地址。
/// 返回 `{ public_url }`。
pub async fn booklet_copy_link_url(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<CopyLinkView> {
    Ok(ApiResponse::ok_with_data(process(&state).copy_link_url(&id, &actor).await?))
}

/// 内部会话快照。
/// 返回当前会话。
pub async fn booklet_session(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SalesSelectionSessionView> {
    Ok(ApiResponse::ok_with_data(process(&state).admin_session(&id, &actor).await?))
}

/// 更换链接。
/// 返回新链接。
pub async fn booklet_rotate_link(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SalesSelectionCommandRequest>,
) -> Result<SalesSelectionBookletView> {
    Ok(ApiResponse::ok_with_data(process(&state).rotate_link(id, req, actor).await?))
}

/// 关闭未提交选品册。
/// 返回已关闭详情。
pub async fn booklet_close(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SalesSelectionCommandRequest>,
) -> Result<SalesSelectionBookletView> {
    Ok(ApiResponse::ok_with_data(process(&state).close(id, req, actor).await?))
}

/// 撤销已提交链接访问。
pub async fn booklet_revoke(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SalesSelectionCommandRequest>,
) -> Result<SalesSelectionBookletView> {
    Ok(ApiResponse::ok_with_data(process(&state).revoke(id, req, actor).await?))
}

/// 作废选品册。
/// 返回已作废详情。
pub async fn booklet_void(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SalesSelectionCommandRequest>,
) -> Result<SalesSelectionBookletView> {
    Ok(ApiResponse::ok_with_data(process(&state).void(id, req, actor).await?))
}

/// 按请求条件查询销售方案列表。
pub async fn proposal_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<SalesSelectionProposalListParams>,
) -> Result<SelectionProposalListView> {
    Ok(ApiResponse::ok_with_data(read_service(&state).proposal_list(params, actor).await?))
}

/// 查询销售方案详情。
pub async fn proposal_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SalesSelectionProposalView> {
    let proposal = read_service(&state).proposal_detail(&id, &actor).await?;
    Ok(ApiResponse::ok_with_data(proposal))
}

/// 读取管理端受权限保护的快照图片。
/// 客户越权、跨册资产和存储读取失败时拒绝。
pub async fn admin_image(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(query): Query<PublicImageQuery>,
) -> std::result::Result<Response, Error> {
    let (bytes, content_type) = process(&state).admin_image(&id, &query.asset_id, &actor).await?;
    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(&content_type).unwrap_or_else(|_| HeaderValue::from_static("image/jpeg")),
    );
    response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    Ok(response)
}

/// 公开选品页。
/// 返回公开视图。
pub async fn public_page(
    State(state): State<HttpState>,
    Path(token): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<PublicSelectionPageView> {
    let access = protection::selection_access(&headers);
    Ok(ApiResponse::ok_with_data(process(&state).public_page(&token, access, &addr.ip().to_string()).await?))
}

/// 公开保存会话。
/// 返回最新公开页。
pub async fn public_save(
    State(state): State<HttpState>,
    Path(token): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<SaveSelectionSessionRequest>,
) -> Result<PublicSelectionPageView> {
    let access = protection::selection_access(&headers).unwrap_or_default().to_owned();
    Ok(ApiResponse::ok_with_data(
        process(&state).public_save(token, access, req, addr.ip().to_string()).await?,
    ))
}

/// 公开提交。
/// 返回回执。
pub async fn public_submit(
    State(state): State<HttpState>,
    Path(token): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<SubmitSelectionSessionRequest>,
) -> Result<PublicSelectionPageView> {
    let access = protection::selection_access(&headers).unwrap_or_default().to_owned();
    Ok(ApiResponse::ok_with_data(
        process(&state).public_submit(token, access, req, addr.ip().to_string()).await?,
    ))
}

/// 公开图片。
/// 返回图片字节；禁止公共缓存。
pub async fn public_image(
    State(state): State<HttpState>,
    Path(token): Path<String>,
    Query(query): Query<PublicImageQuery>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> std::result::Result<Response, Error> {
    respond_public_image(
        &state,
        &token,
        &query.asset_id,
        query.access_token.as_deref().unwrap_or_default(),
        addr,
    )
    .await
}

/// 公开图片（路径参数兼容）。
/// 返回图片字节。
pub async fn public_image_path(
    State(state): State<HttpState>,
    Path((token, asset_id)): Path<(String, String)>,
    Query(query): Query<protection::ImageAccessQuery>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> std::result::Result<Response, Error> {
    respond_public_image(&state, &token, &asset_id, query.access_token.as_deref().unwrap_or_default(), addr)
        .await
}

/// 公开选品页只支持整套选择；旧自组或换品请求返回 422，不修改当前选品。
pub async fn public_customize() -> Result<PublicSelectionPageView> {
    Err(Error::Unprocessable("换品与自组为后续能力，当前仅支持整套选择".into()))
}

/// 公开图片查询。
#[derive(Deserialize)]
pub struct PublicImageQuery {
    /// 资产引用。
    #[serde(rename = "ref", alias = "asset_id")]
    pub asset_id: String,
    /// 已解锁的访问凭证，不写入日志。
    pub access_token: Option<String>,
}

// 读取并封装公开图片响应。
// 返回禁止公共缓存的图片响应。
// 越权或对象不存在。
async fn respond_public_image(
    state: &HttpState,
    token: &str,
    asset_id: &str,
    access: &str,
    addr: SocketAddr,
) -> std::result::Result<Response, Error> {
    let proc = process(state);
    let key = proc.public_image_key(token, asset_id, access, &addr.ip().to_string()).await?;
    let (bytes, content_type) = proc.read_image_bytes(&key).await?;
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(&content_type).unwrap_or_else(|_| HeaderValue::from_static("image/jpeg")),
    );
    response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response.headers_mut().insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    response.headers_mut().insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    Ok(response)
}
