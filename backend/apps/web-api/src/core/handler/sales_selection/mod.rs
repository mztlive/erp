//! 销售选品 HTTP handler。

mod access;

use std::net::SocketAddr;

use application_core::AuditActor;
use axum::body::Body;
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS};
use axum::http::{HeaderValue, StatusCode};
use axum::response::Response;
use axum::{Extension, Json};
use erp_processes::sales_selection::{
    SalesSelectionProcess, SelectionBookletListView, SelectionProposalListView,
};
use erp_sales::dto::sales_selection::{
    CopyLinkView, CreateSalesSelectionBookletRequest, DeleteDisplayItemRequest, PrepareSalesSelectionRequest,
    PublicSelectionPageView, PublishSalesSelectionRequest, SalesSelectionBookletListParams,
    SalesSelectionBookletView, SalesSelectionCommandRequest, SalesSelectionProposalListParams,
    SalesSelectionProposalView, SalesSelectionSessionView, SaveSelectionSessionRequest,
    SubmitSelectionSessionRequest,
};
use serde::Deserialize;

use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::extractor::UserID;
use crate::core::handler::customer::ensure_customer_access;
use crate::core::middleware::RbacSubject;
use crate::core::response::ApiResponse;

fn process(state: &AppState) -> SalesSelectionProcess {
    SalesSelectionProcess::new(
        state.db(),
        state.storage().clone(),
        state.config_snapshot().app.secret.as_bytes(),
    )
    .with_rbac(state.rbac())
}

#[permission_macros::permission(
    group = "销售选品",
    group_desc = "选品册与销售方案",
    desc = "查询选品册列表",
    resource = "sales_selection_booklet",
    action = "list"
)]
/// 查询选品册列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `params` - 筛选
///
/// # 返回
/// 返回分页。
pub async fn booklet_list(
    State(state): State<AppState>,
    Extension(subject): Extension<RbacSubject>,
    Extension(actor): Extension<AuditActor>,
    Extension(UserID(user_id)): Extension<UserID>,
    Query(mut params): Query<SalesSelectionBookletListParams>,
) -> Result<SelectionBookletListView> {
    params.authorized_customer_ids = access::customer_ids(&state, &subject, &user_id).await?;
    Ok(ApiResponse::ok_with_data(process(&state).booklet_list(params, actor).await?))
}

#[permission_macros::permission(
    group = "销售选品",
    group_desc = "选品册与销售方案",
    desc = "查询选品册详情",
    resource = "sales_selection_booklet",
    action = "get"
)]
/// 查询选品册详情。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 选品册
///
/// # 返回
/// 返回详情。
pub async fn booklet_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SalesSelectionBookletView> {
    let view = process(&state).booklet_detail(&id, &actor).await?;
    ensure_customer_access(&state, &actor, "detail", &view.customer_id).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "销售选品",
    group_desc = "选品册与销售方案",
    desc = "创建选品册",
    resource = "sales_selection_booklet",
    action = "create"
)]
/// 创建选品册并排队首次准备。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回准备中详情。
pub async fn booklet_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSalesSelectionBookletRequest>,
) -> Result<SalesSelectionBookletView> {
    ensure_customer_access(&state, &actor, "detail", &req.customer_id).await?;
    Ok(ApiResponse::ok_with_data(process(&state).create(req, actor).await?))
}

#[permission_macros::permission(
    group = "销售选品",
    group_desc = "选品册与销售方案",
    desc = "准备及重生成选品册",
    resource = "sales_selection_booklet",
    action = "prepare"
)]
/// 启动准备。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 操作人
/// * `id` - 选品册
/// * `req` - 准备请求
///
/// # 返回
/// 返回准备中详情。
pub async fn booklet_prepare(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<PrepareSalesSelectionRequest>,
) -> Result<SalesSelectionBookletView> {
    access::booklet(&state, &actor, &id).await?;
    Ok(ApiResponse::ok_with_data(process(&state).start_prepare(id, req, actor).await?))
}

#[permission_macros::permission(
    group = "销售选品",
    group_desc = "选品册与销售方案",
    desc = "维护选品册陈列",
    resource = "sales_selection_booklet",
    action = "maintain"
)]
/// 删除陈列项。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 操作人
/// * `ids` - 册与项
/// * `req` - 版本
///
/// # 返回
/// 返回详情。
pub async fn booklet_delete_item(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, item_id)): Path<(String, String)>,
    Query(req): Query<DeleteDisplayItemRequest>,
) -> Result<SalesSelectionBookletView> {
    access::booklet(&state, &actor, &id).await?;
    Ok(ApiResponse::ok_with_data(process(&state).delete_display_item(id, item_id, req, actor).await?))
}

#[permission_macros::permission(
    group = "销售选品",
    group_desc = "选品册与销售方案",
    desc = "维护选品册陈列",
    resource = "sales_selection_booklet",
    action = "maintain"
)]
/// 删除陈列项（POST 兼容路径）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 操作人
/// * `ids` - 册与项
/// * `req` - 版本
///
/// # 返回
/// 返回详情。
pub async fn booklet_delete_item_post(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, item_id)): Path<(String, String)>,
    Json(req): Json<DeleteDisplayItemRequest>,
) -> Result<SalesSelectionBookletView> {
    access::booklet(&state, &actor, &id).await?;
    Ok(ApiResponse::ok_with_data(process(&state).delete_display_item(id, item_id, req, actor).await?))
}

#[permission_macros::permission(
    group = "销售选品",
    group_desc = "选品册与销售方案",
    desc = "发布选品册",
    resource = "sales_selection_booklet",
    action = "publish"
)]
/// 发布选品册。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 发布人
/// * `id` - 选品册
/// * `req` - 发布请求
///
/// # 返回
/// 返回含链接的详情。
pub async fn booklet_publish(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<PublishSalesSelectionRequest>,
) -> Result<SalesSelectionBookletView> {
    access::booklet(&state, &actor, &id).await?;
    Ok(ApiResponse::ok_with_data(process(&state).publish(id, req, actor).await?))
}

#[permission_macros::permission(
    group = "销售选品",
    group_desc = "选品册与销售方案",
    desc = "复制选品链接",
    resource = "sales_selection_booklet",
    action = "copy_link"
)]
/// 复制当前链接。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 选品册
///
/// # 返回
/// 返回含路径的详情。
pub async fn booklet_copy_link(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SalesSelectionBookletView> {
    access::booklet(&state, &actor, &id).await?;
    Ok(ApiResponse::ok_with_data(process(&state).copy_link(&id, &actor).await?))
}

#[permission_macros::permission(
    group = "销售选品",
    group_desc = "选品册与销售方案",
    desc = "复制选品链接",
    resource = "sales_selection_booklet",
    action = "copy_link"
)]
/// 复制当前链接相对地址。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 选品册
///
/// # 返回
/// 返回 `{ public_url }`。
pub async fn booklet_copy_link_url(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<CopyLinkView> {
    access::booklet(&state, &actor, &id).await?;
    Ok(ApiResponse::ok_with_data(process(&state).copy_link_url(&id, &actor).await?))
}

#[permission_macros::permission(
    group = "销售选品",
    group_desc = "选品册与销售方案",
    desc = "查询选品册详情",
    resource = "sales_selection_booklet",
    action = "get"
)]
/// 内部会话快照。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 选品册
///
/// # 返回
/// 返回当前会话。
pub async fn booklet_session(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SalesSelectionSessionView> {
    access::booklet(&state, &actor, &id).await?;
    Ok(ApiResponse::ok_with_data(process(&state).admin_session(&id, &actor).await?))
}

#[permission_macros::permission(
    group = "销售选品",
    group_desc = "选品册与销售方案",
    desc = "更换选品链接",
    resource = "sales_selection_booklet",
    action = "rotate_link"
)]
/// 更换链接。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 操作人
/// * `id` - 选品册
/// * `req` - 命令
///
/// # 返回
/// 返回新链接。
pub async fn booklet_rotate_link(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SalesSelectionCommandRequest>,
) -> Result<SalesSelectionBookletView> {
    access::booklet(&state, &actor, &id).await?;
    Ok(ApiResponse::ok_with_data(process(&state).rotate_link(id, req, actor).await?))
}

#[permission_macros::permission(
    group = "销售选品",
    group_desc = "选品册与销售方案",
    desc = "关闭或撤销选品链接",
    resource = "sales_selection_booklet",
    action = "close"
)]
/// 关闭未提交选品册。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 操作人
/// * `id` - 选品册
/// * `req` - 命令
///
/// # 返回
/// 返回已关闭详情。
pub async fn booklet_close(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SalesSelectionCommandRequest>,
) -> Result<SalesSelectionBookletView> {
    access::booklet(&state, &actor, &id).await?;
    Ok(ApiResponse::ok_with_data(process(&state).close(id, req, actor).await?))
}

#[permission_macros::permission(
    group = "销售选品",
    group_desc = "选品册与销售方案",
    desc = "关闭或撤销选品链接",
    resource = "sales_selection_booklet",
    action = "revoke"
)]
/// 撤销已提交链接访问。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 操作人
/// * `id` - 选品册
/// * `req` - 命令
///
/// # 返回
/// 返回详情。
pub async fn booklet_revoke(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SalesSelectionCommandRequest>,
) -> Result<SalesSelectionBookletView> {
    access::booklet(&state, &actor, &id).await?;
    Ok(ApiResponse::ok_with_data(process(&state).revoke(id, req, actor).await?))
}

#[permission_macros::permission(
    group = "销售选品",
    group_desc = "选品册与销售方案",
    desc = "发布前作废选品册",
    resource = "sales_selection_booklet",
    action = "void"
)]
/// 作废选品册。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 操作人
/// * `id` - 选品册
/// * `req` - 命令
///
/// # 返回
/// 返回已作废详情。
pub async fn booklet_void(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SalesSelectionCommandRequest>,
) -> Result<SalesSelectionBookletView> {
    access::booklet(&state, &actor, &id).await?;
    Ok(ApiResponse::ok_with_data(process(&state).void(id, req, actor).await?))
}

#[permission_macros::permission(
    group = "销售选品",
    group_desc = "选品册与销售方案",
    desc = "查询销售方案列表",
    resource = "sales_selection_proposal",
    action = "list"
)]
/// 查询销售方案列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `params` - 筛选
///
/// # 返回
/// 返回分页。
pub async fn proposal_list(
    State(state): State<AppState>,
    Extension(subject): Extension<RbacSubject>,
    Extension(actor): Extension<AuditActor>,
    Extension(UserID(user_id)): Extension<UserID>,
    Query(mut params): Query<SalesSelectionProposalListParams>,
) -> Result<SelectionProposalListView> {
    params.authorized_customer_ids = access::customer_ids(&state, &subject, &user_id).await?;
    Ok(ApiResponse::ok_with_data(process(&state).proposal_list(params, actor).await?))
}

#[permission_macros::permission(
    group = "销售选品",
    group_desc = "选品册与销售方案",
    desc = "查询销售方案详情",
    resource = "sales_selection_proposal",
    action = "get"
)]
/// 查询销售方案详情。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 方案
///
/// # 返回
/// 返回详情。
pub async fn proposal_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SalesSelectionProposalView> {
    let proposal = process(&state).proposal_detail(&id, &actor).await?;
    ensure_customer_access(&state, &actor, "detail", &proposal.customer_id).await?;
    Ok(ApiResponse::ok_with_data(proposal))
}

#[permission_macros::permission(
    group = "销售选品",
    group_desc = "选品册与销售方案",
    desc = "查询选品册详情",
    resource = "sales_selection_booklet",
    action = "get"
)]
/// 读取管理端受权限保护的快照图片。
/// # 错误
/// 客户越权、跨册资产和存储读取失败时拒绝。
pub async fn admin_image(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(query): Query<PublicImageQuery>,
) -> std::result::Result<Response, Error> {
    access::booklet(&state, &actor, &id).await?;
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
///
/// # 参数
/// * `state` - 应用状态
/// * `token` - 令牌
/// * `addr` - 来源地址
///
/// # 返回
/// 返回公开视图。
pub async fn public_page(
    State(state): State<AppState>,
    Path(token): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<PublicSelectionPageView> {
    Ok(ApiResponse::ok_with_data(process(&state).public_page(&token, &addr.ip().to_string()).await?))
}

/// 公开保存会话。
///
/// # 参数
/// * `state` - 应用状态
/// * `token` - 令牌
/// * `req` - 保存请求
/// * `addr` - 来源地址
///
/// # 返回
/// 返回最新公开页。
pub async fn public_save(
    State(state): State<AppState>,
    Path(token): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<SaveSelectionSessionRequest>,
) -> Result<PublicSelectionPageView> {
    Ok(ApiResponse::ok_with_data(process(&state).public_save(token, req, addr.ip().to_string()).await?))
}

/// 公开提交。
///
/// # 参数
/// * `state` - 应用状态
/// * `token` - 令牌
/// * `req` - 提交请求
/// * `addr` - 来源地址
///
/// # 返回
/// 返回回执。
pub async fn public_submit(
    State(state): State<AppState>,
    Path(token): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<SubmitSelectionSessionRequest>,
) -> Result<PublicSelectionPageView> {
    Ok(ApiResponse::ok_with_data(process(&state).public_submit(token, req, addr.ip().to_string()).await?))
}

/// 公开图片。
///
/// # 参数
/// * `state` - 应用状态
/// * `token` - 令牌
/// * `asset_id` - 资产
/// * `addr` - 来源地址
///
/// # 返回
/// 返回图片字节；禁止公共缓存。
pub async fn public_image(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(query): Query<PublicImageQuery>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> std::result::Result<Response, Error> {
    respond_public_image(&state, &token, &query.asset_id, addr).await
}

/// 公开图片（路径参数兼容）。
///
/// # 参数
/// * `state` - 应用状态
/// * `ids` - 令牌与资产
/// * `addr` - 来源地址
///
/// # 返回
/// 返回图片字节。
pub async fn public_image_path(
    State(state): State<AppState>,
    Path((token, asset_id)): Path<(String, String)>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> std::result::Result<Response, Error> {
    respond_public_image(&state, &token, &asset_id, addr).await
}

/// 拒绝 P0 换品/自组。
///
/// # 参数
/// 无。
///
/// # 返回
/// 无。
///
/// # 错误
/// 始终拒绝并提示后续能力。
pub async fn public_customize() -> Result<PublicSelectionPageView> {
    Err(Error::Unprocessable("换品与自组为后续能力，当前仅支持整套选择".into()))
}

/// 公开图片查询。
#[derive(Debug, Deserialize)]
pub struct PublicImageQuery {
    /// 资产引用。
    #[serde(rename = "ref", alias = "asset_id")]
    pub asset_id: String,
}

/// 读取并封装公开图片响应。
///
/// # 参数
/// * `state` - 应用状态
/// * `token` - 令牌
/// * `asset_id` - 资产
/// * `addr` - 来源地址
///
/// # 返回
/// 返回禁止公共缓存的图片响应。
///
/// # 错误
/// 越权或对象不存在。
async fn respond_public_image(
    state: &AppState,
    token: &str,
    asset_id: &str,
    addr: SocketAddr,
) -> std::result::Result<Response, Error> {
    let proc = process(state);
    let key = proc.public_image_key(token, asset_id, &addr.ip().to_string()).await?;
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
