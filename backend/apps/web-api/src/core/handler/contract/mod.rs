//! 域 D12 `contract` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::contract` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    contract::{
        ArchiveContractRevisionRequest, ContractDetailView, ContractListParams, ContractService,
        ContractView, CreateContractRequest, PageView, TerminateContractRequest,
    },
};

use crate::{
    app_state::AppState,
    core::{
        errors::Result, extractor::UserID, handler::customer::ensure_customer_access,
        middleware::RbacSubject, response::ApiResponse,
    },
};

#[permission_macros::permission(
    group = "合同",
    group_desc = "合同 PDF 档案管理",
    desc = "查询合同列表",
    resource = "contract",
    action = "list"
)]
/// 查询合同列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn contract_list(
    State(state): State<AppState>,
    Query(params): Query<ContractListParams>,
) -> Result<PageView<ContractView>> {
    let page = ContractService::new(state.db()).contract_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "合同",
    group_desc = "合同 PDF 档案管理",
    desc = "首次归档合同 PDF",
    resource = "contract",
    action = "create"
)]
/// 首次归档合同（合同身份 + 首个不可变版本 + PDF 关联原子形成）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（`contract_no`、客户、结算主体与版本快照）
///
/// # 返回
/// 返回新建合同的响应视图。
pub async fn contract_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Extension(subject): Extension<RbacSubject>,
    Extension(UserID(user_id)): Extension<UserID>,
    Json(req): Json<CreateContractRequest>,
) -> Result<ContractView> {
    ensure_customer_access(&state, &subject, &user_id, &req.customer_id).await?;
    let view = ContractService::new(state.db())
        .create_contract(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "合同",
    group_desc = "合同 PDF 档案管理",
    desc = "查询合同详情",
    resource = "contract",
    action = "detail"
)]
/// 查询合同详情（合同 + 全部不可变版本时间线）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 合同 ID
///
/// # 返回
/// 返回详情视图（版本按序号倒序）。
pub async fn contract_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<ContractDetailView> {
    let view = ContractService::new(state.db()).contract_detail(&id).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "合同",
    group_desc = "合同 PDF 档案管理",
    desc = "归档合同新版本",
    resource = "contract",
    action = "update"
)]
/// 归档合同新版本（追加不可变版本并切换当前版本指针，乐观锁）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 合同 ID
/// * `req` - 追加版本请求（含期望版本）
///
/// # 返回
/// 返回追加后的合同详情视图。
pub async fn contract_archive_revision(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<ArchiveContractRevisionRequest>,
) -> Result<ContractDetailView> {
    let view = ContractService::new(state.db())
        .archive_contract_revision(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "合同",
    group_desc = "合同 PDF 档案管理",
    desc = "终止合同",
    resource = "contract",
    action = "update"
)]
/// 终止合同（乐观锁；历史销售引用保持不变）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 合同 ID
/// * `req` - 终止请求（含期望版本）
///
/// # 返回
/// 返回终止后的合同详情视图。
pub async fn contract_terminate(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<TerminateContractRequest>,
) -> Result<ContractDetailView> {
    let view = ContractService::new(state.db())
        .terminate_contract(&id, req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}
