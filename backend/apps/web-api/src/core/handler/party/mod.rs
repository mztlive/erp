//! 域 D07 `party` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::party` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::audit::AuditActor;
use services::party::{
    address::PartyAddressService, bank_account::PartyBankAccountService, contact::PartyContactService,
    tax_profile::PartyTaxProfileService, CreatePartyAddressRequest, CreatePartyBankAccountRequest,
    CreatePartyContactRequest, CreatePartyRequest, CreatePartyTaxProfileRequest, PageView,
    PartyAddressListParams, PartyAddressView, PartyBankAccountListParams, PartyBankAccountView,
    PartyContactListParams, PartyContactView, PartyDetailView, PartyListParams, PartyRevisionListParams,
    PartyRevisionView, PartyService, PartyTaxProfileListParams, PartyTaxProfileView, PartyView,
    UpdatePartyAddressRequest, UpdatePartyBankAccountRequest, UpdatePartyContactRequest, UpdatePartyRequest,
    UpdatePartyTaxProfileRequest,
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

#[permission_macros::permission(
    group = "主体",
    group_desc = "企业主体稳定身份与历史名称资料管理",
    desc = "查询主体列表",
    resource = "party",
    action = "list"
)]
/// 查询主体列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`keyword`/`party_kind`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn party_list(
    State(state): State<AppState>,
    Query(params): Query<PartyListParams>,
) -> Result<PageView<PartyView>> {
    let page = PartyService::new(state.db()).party_list(&params).await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "主体",
    group_desc = "企业主体稳定身份与历史名称资料管理",
    desc = "创建主体",
    resource = "party",
    action = "create"
)]
/// 创建主体（同事务建立主体 + 首版修订）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建主体的响应视图。
pub async fn party_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreatePartyRequest>,
) -> Result<PartyView> {
    let view = PartyService::new(state.db()).create_party(req, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "主体",
    group_desc = "企业主体稳定身份与历史名称资料管理",
    desc = "查询主体详情",
    resource = "party",
    action = "detail"
)]
/// 查询主体详情（主体 + 当前生效修订快照）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 主体 ID
///
/// # 返回
/// 返回主体详情视图。
pub async fn party_detail(State(state): State<AppState>, Path(id): Path<String>) -> Result<PartyDetailView> {
    let view = PartyService::new(state.db()).party_detail(&id).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "主体",
    group_desc = "企业主体稳定身份与历史名称资料管理",
    desc = "更新主体并形成新修订",
    resource = "party",
    action = "update"
)]
/// 更新主体（乐观锁 + 追加修订）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 主体 ID
/// * `req` - 更新请求（含期望版本与新修订快照）
///
/// # 返回
/// 返回更新后主体的响应视图。
pub async fn party_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePartyRequest>,
) -> Result<PartyView> {
    let view = PartyService::new(state.db())
        .update_party(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "主体",
    group_desc = "企业主体稳定身份与历史名称资料管理",
    desc = "删除主体",
    resource = "party",
    action = "delete"
)]
/// 软删除主体。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 主体 ID
///
/// # 返回
/// 返回统一成功信封。
pub async fn party_delete(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    PartyService::new(state.db()).delete_party(&id, &actor).await?;
    Ok(ApiResponse::ok())
}

#[permission_macros::permission(
    group = "主体",
    group_desc = "企业主体稳定身份与历史名称资料管理",
    desc = "查询主体修订列表",
    resource = "party_revision",
    action = "list"
)]
/// 查询主体修订列表（版本链历史）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 主体 ID
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn party_revision_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<PartyRevisionListParams>,
) -> Result<PageView<PartyRevisionView>> {
    let page = PartyService::new(state.db())
        .party_revision_list(&id, &params)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "主体",
    group_desc = "企业主体稳定身份与历史名称资料管理",
    desc = "查询联系人列表",
    resource = "party_contact",
    action = "list"
)]
/// 查询联系人列表（敏感字段不进投影）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 主体 ID
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn party_contact_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<PartyContactListParams>,
) -> Result<PageView<PartyContactView>> {
    let page = PartyContactService::new(state.db())
        .party_contact_list(&id, &params)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "主体",
    group_desc = "企业主体稳定身份与历史名称资料管理",
    desc = "创建联系人",
    resource = "party_contact",
    action = "create"
)]
/// 创建联系人（手机号为明文入参，实体只保留指纹与密文）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 主体 ID
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建联系人的响应视图。
pub async fn party_contact_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CreatePartyContactRequest>,
) -> Result<PartyContactView> {
    let view = PartyContactService::new(state.db())
        .create_party_contact(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "主体",
    group_desc = "企业主体稳定身份与历史名称资料管理",
    desc = "更新联系人",
    resource = "party_contact",
    action = "update"
)]
/// 更新联系人（仅生命周期字段）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 联系人 ID
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后联系人的响应视图。
pub async fn party_contact_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePartyContactRequest>,
) -> Result<PartyContactView> {
    let view = PartyContactService::new(state.db())
        .update_party_contact(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "主体",
    group_desc = "企业主体稳定身份与历史名称资料管理",
    desc = "查询地址列表",
    resource = "party_address",
    action = "list"
)]
/// 查询地址列表（敏感字段不进投影）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 主体 ID
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn party_address_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<PartyAddressListParams>,
) -> Result<PageView<PartyAddressView>> {
    let page = PartyAddressService::new(state.db())
        .party_address_list(&id, &params)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "主体",
    group_desc = "企业主体稳定身份与历史名称资料管理",
    desc = "创建地址",
    resource = "party_address",
    action = "create"
)]
/// 创建地址（地址内容为明文入参，实体只保留指纹与密文）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 主体 ID
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建地址的响应视图。
pub async fn party_address_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CreatePartyAddressRequest>,
) -> Result<PartyAddressView> {
    let view = PartyAddressService::new(state.db())
        .create_party_address(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "主体",
    group_desc = "企业主体稳定身份与历史名称资料管理",
    desc = "更新地址",
    resource = "party_address",
    action = "update"
)]
/// 更新地址（仅生命周期字段）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 地址 ID
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后地址的响应视图。
pub async fn party_address_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePartyAddressRequest>,
) -> Result<PartyAddressView> {
    let view = PartyAddressService::new(state.db())
        .update_party_address(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "主体",
    group_desc = "企业主体稳定身份与历史名称资料管理",
    desc = "查询税务资料列表",
    resource = "party_tax_profile",
    action = "list"
)]
/// 查询税务资料列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 主体 ID
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn party_tax_profile_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<PartyTaxProfileListParams>,
) -> Result<PageView<PartyTaxProfileView>> {
    let page = PartyTaxProfileService::new(state.db())
        .party_tax_profile_list(&id, &params)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "主体",
    group_desc = "企业主体稳定身份与历史名称资料管理",
    desc = "创建税务资料",
    resource = "party_tax_profile",
    action = "create"
)]
/// 创建税务资料。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 主体 ID
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建税务资料的响应视图。
pub async fn party_tax_profile_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CreatePartyTaxProfileRequest>,
) -> Result<PartyTaxProfileView> {
    let view = PartyTaxProfileService::new(state.db())
        .create_party_tax_profile(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "主体",
    group_desc = "企业主体稳定身份与历史名称资料管理",
    desc = "更新税务资料",
    resource = "party_tax_profile",
    action = "update"
)]
/// 更新税务资料（仅生命周期字段）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 税务资料 ID
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后税务资料的响应视图。
pub async fn party_tax_profile_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePartyTaxProfileRequest>,
) -> Result<PartyTaxProfileView> {
    let view = PartyTaxProfileService::new(state.db())
        .update_party_tax_profile(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "主体",
    group_desc = "企业主体稳定身份与历史名称资料管理",
    desc = "查询银行账户列表",
    resource = "party_bank_account",
    action = "list"
)]
/// 查询银行账户列表（敏感字段不进投影）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 主体 ID
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn party_bank_account_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<PartyBankAccountListParams>,
) -> Result<PageView<PartyBankAccountView>> {
    let page = PartyBankAccountService::new(state.db())
        .party_bank_account_list(&id, &params)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "主体",
    group_desc = "企业主体稳定身份与历史名称资料管理",
    desc = "创建银行账户",
    resource = "party_bank_account",
    action = "create"
)]
/// 创建银行账户（账号为明文入参，实体只保留指纹与密文）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 主体 ID
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建银行账户的响应视图。
pub async fn party_bank_account_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CreatePartyBankAccountRequest>,
) -> Result<PartyBankAccountView> {
    let view = PartyBankAccountService::new(state.db())
        .create_party_bank_account(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "主体",
    group_desc = "企业主体稳定身份与历史名称资料管理",
    desc = "更新银行账户",
    resource = "party_bank_account",
    action = "update"
)]
/// 更新银行账户（仅生命周期字段）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 银行账户 ID
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后银行账户的响应视图。
pub async fn party_bank_account_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePartyBankAccountRequest>,
) -> Result<PartyBankAccountView> {
    let view = PartyBankAccountService::new(state.db())
        .update_party_bank_account(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}
