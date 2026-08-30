//! 域 D08 `customer` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::customer` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use entities::{common::time::Instant, Permission};
use services::audit::AuditActor;
use services::customer::{
    assignment::CustomerAssignmentService, profile::CustomerProfileService, CreateCustomerRequest,
    CustomerAssignmentListParams, CustomerAssignmentRequest, CustomerAssignmentView,
    CustomerCenterReadService, CustomerCenterReceivableView, CustomerCenterRelatedView, CustomerDetailView,
    CustomerListParams, CustomerProfileDetailView, CustomerProfileMutationView, CustomerScope,
    CustomerSensitiveRevealView, CustomerService, CustomerView, PageView, RevealCustomerSensitiveRequest,
    SaveCustomerProfileRequest, UpdateCustomerRequest,
};

use crate::{
    app_state::AppState,
    core::{
        errors::{Error, Result},
        extractor::UserID,
        middleware::RbacSubject,
        response::ApiResponse,
    },
};

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "创建完整客户资料",
    resource = "customer",
    action = "create"
)]
/// 原子创建完整客户资料。
pub async fn customer_profile_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Extension(subject): Extension<RbacSubject>,
    Json(req): Json<SaveCustomerProfileRequest>,
) -> Result<CustomerProfileMutationView> {
    if req.contacts.is_some() {
        ensure_permission(&state, &subject, "party_contact:create").await?;
    }
    if req.addresses.is_some() {
        ensure_permission(&state, &subject, "party_address:create").await?;
    }
    if req.bank_accounts.is_some() {
        ensure_permission(&state, &subject, "party_bank_account:create").await?;
    }
    let view = CustomerProfileService::new(state.db(), state.sensitive_data())
        .create(req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "修订完整客户资料",
    resource = "customer",
    action = "update"
)]
/// 原子修订完整客户资料。
pub async fn customer_profile_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Extension(subject): Extension<RbacSubject>,
    Extension(UserID(user_id)): Extension<UserID>,
    Path(id): Path<String>,
    Json(req): Json<SaveCustomerProfileRequest>,
) -> Result<CustomerProfileMutationView> {
    ensure_customer_access(&state, &subject, &user_id, &id).await?;
    if req.contacts.is_some() {
        ensure_permission(&state, &subject, "party_contact:detail").await?;
        ensure_permission(&state, &subject, "party_contact:update").await?;
    }
    if req.addresses.is_some() {
        ensure_permission(&state, &subject, "party_address:detail").await?;
        ensure_permission(&state, &subject, "party_address:update").await?;
    }
    if req.bank_accounts.is_some() {
        ensure_permission(&state, &subject, "party_bank_account:detail").await?;
        ensure_permission(&state, &subject, "party_bank_account:update").await?;
    }
    let view = CustomerProfileService::new(state.db(), state.sensitive_data())
        .update(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "查询完整客户资料",
    resource = "customer",
    action = "detail"
)]
/// 查询完整客户资料，并按当前字段权限裁剪从属事实与允许动作。
pub async fn customer_profile_detail(
    State(state): State<AppState>,
    Extension(subject): Extension<RbacSubject>,
    Extension(UserID(user_id)): Extension<UserID>,
    Path(id): Path<String>,
) -> Result<CustomerProfileDetailView> {
    ensure_customer_access(&state, &subject, &user_id, &id).await?;
    let mut view = CustomerProfileService::new(state.db(), state.sensitive_data())
        .detail(&id)
        .await?;
    view.allowed_actions = allowed_actions(&state, &subject, view.account.status.is_active()).await?;
    let can_view_contact = has_permission(&state, &subject, "party_contact:detail").await?;
    let can_view_address = has_permission(&state, &subject, "party_address:detail").await?;
    let can_view_tax = has_permission(&state, &subject, "party_tax_profile:detail").await?;
    let can_view_bank = has_permission(&state, &subject, "party_bank_account:detail").await?;
    let can_reveal_contact =
        can_view_contact && has_permission(&state, &subject, "party_contact:reveal").await?;
    let can_reveal_address =
        can_view_address && has_permission(&state, &subject, "party_address:reveal").await?;
    let can_reveal_bank =
        can_view_bank && has_permission(&state, &subject, "party_bank_account:reveal").await?;
    view.sensitive_fields.retain(|field| match field.kind {
        services::party::SensitiveFieldKind::ContactMobile => can_reveal_contact,
        services::party::SensitiveFieldKind::Address => can_reveal_address,
        services::party::SensitiveFieldKind::BankAccountNumber => can_reveal_bank,
    });
    if !can_view_bank {
        view.bank_accounts.clear();
    }
    if !can_view_contact {
        view.contacts.clear();
    }
    if !can_view_address {
        view.addresses.clear();
    }
    if !can_view_tax {
        view.tax_profiles.clear();
    }
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "查询客户中心合同与销售摘要",
    resource = "customer",
    action = "detail"
)]
/// 查询客户中心合同/销售最近摘要与跨页指标。
pub async fn customer_center_related(
    State(state): State<AppState>,
    Extension(subject): Extension<RbacSubject>,
    Extension(UserID(user_id)): Extension<UserID>,
    Path(id): Path<String>,
) -> Result<CustomerCenterRelatedView> {
    ensure_customer_access(&state, &subject, &user_id, &id).await?;
    ensure_permission(&state, &subject, "contract:list").await?;
    ensure_permission(&state, &subject, "sales_order:list").await?;
    let view = CustomerCenterReadService::new(state.db()).related(&id).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "查询客户中心应收汇总",
    resource = "customer",
    action = "detail"
)]
/// 查询客户中心跨应收账户的定点金额汇总。
pub async fn customer_center_receivable(
    State(state): State<AppState>,
    Extension(subject): Extension<RbacSubject>,
    Extension(UserID(user_id)): Extension<UserID>,
    Path(id): Path<String>,
) -> Result<CustomerCenterReceivableView> {
    ensure_customer_access(&state, &subject, &user_id, &id).await?;
    ensure_permission(&state, &subject, "receivable_account:list").await?;
    let view = CustomerCenterReadService::new(state.db()).receivable(&id).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "查询客户资料保存结果",
    resource = "customer",
    action = "detail"
)]
/// 按幂等键查询已成功客户资料命令结果。
pub async fn customer_profile_command_detail(
    State(state): State<AppState>,
    Extension(subject): Extension<RbacSubject>,
    Extension(UserID(user_id)): Extension<UserID>,
    Path(idempotency_key): Path<String>,
) -> Result<Option<CustomerProfileMutationView>> {
    let view = CustomerProfileService::new(state.db(), state.sensitive_data())
        .command_result(&idempotency_key)
        .await?;
    if let Some(result) = &view {
        if result.initiated_by != user_id {
            ensure_customer_access(&state, &subject, &user_id, &result.customer_id).await?;
        }
    }
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "查看客户敏感字段",
    resource = "customer_sensitive",
    action = "reveal"
)]
/// 按短时令牌揭示单个敏感字段，并同时校验客户范围、字段详情与揭示权限。
pub async fn customer_sensitive_reveal(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Extension(subject): Extension<RbacSubject>,
    Extension(UserID(user_id)): Extension<UserID>,
    Json(req): Json<RevealCustomerSensitiveRequest>,
) -> Result<CustomerSensitiveRevealView> {
    let now =
        u64::try_from(Instant::now().unix_secs()).map_err(|_| Error::Internal("系统时间非法".to_string()))?;
    let scope = state
        .sensitive_data()
        .verify_reveal_token(&req.reveal_token, now)?;
    let (detail_permission, reveal_permission) = match scope.kind {
        services::party::SensitiveFieldKind::ContactMobile => {
            ("party_contact:detail", "party_contact:reveal")
        }
        services::party::SensitiveFieldKind::Address => ("party_address:detail", "party_address:reveal"),
        services::party::SensitiveFieldKind::BankAccountNumber => {
            ("party_bank_account:detail", "party_bank_account:reveal")
        }
    };
    ensure_customer_access(&state, &subject, &user_id, &scope.supplier_id).await?;
    ensure_permission(&state, &subject, detail_permission).await?;
    ensure_permission(&state, &subject, reveal_permission).await?;
    let view = CustomerProfileService::new(state.db(), state.sensitive_data())
        .reveal_sensitive(req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "查询客户列表",
    resource = "customer",
    action = "list"
)]
/// 查询客户列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`keyword`/`party_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn customer_list(
    State(state): State<AppState>,
    Extension(UserID(user_id)): Extension<UserID>,
    Query(params): Query<CustomerListParams>,
) -> Result<PageView<CustomerView>> {
    if params.scope == CustomerScope::AllAuthorized {
        return Err(Error::Forbidden(
            "全部有权客户必须通过专用授权范围查询".to_string(),
        ));
    }
    let page = CustomerService::new(state.db())
        .customer_list(&params, &user_id)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "查询全部有权客户",
    resource = "customer_scope",
    action = "detail"
)]
/// 查询不受个人归属限制的全部有权客户。
pub async fn customer_all_authorized_list(
    State(state): State<AppState>,
    Extension(subject): Extension<RbacSubject>,
    Extension(UserID(user_id)): Extension<UserID>,
    Query(mut params): Query<CustomerListParams>,
) -> Result<PageView<CustomerView>> {
    ensure_permission(&state, &subject, "customer:list").await?;
    params.scope = CustomerScope::AllAuthorized;
    let page = CustomerService::new(state.db())
        .customer_list(&params, &user_id)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "创建客户",
    resource = "customer",
    action = "create"
)]
/// 创建客户（同事务建立客户角色 + 首条 OWNER 归属）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建客户角色的响应视图。
pub async fn customer_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateCustomerRequest>,
) -> Result<CustomerView> {
    let view = CustomerService::new(state.db())
        .create_customer(req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "查询客户详情",
    resource = "customer",
    action = "detail"
)]
/// 查询客户详情（客户 + 主体身份 + 当前生效 OWNER）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 客户角色 ID
///
/// # 返回
/// 返回客户详情视图。
pub async fn customer_detail(
    State(state): State<AppState>,
    Extension(subject): Extension<RbacSubject>,
    Extension(UserID(user_id)): Extension<UserID>,
    Path(id): Path<String>,
) -> Result<CustomerDetailView> {
    ensure_customer_access(&state, &subject, &user_id, &id).await?;
    let view = CustomerService::new(state.db()).customer_detail(&id).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "更新客户",
    resource = "customer",
    action = "update"
)]
/// 更新客户角色（乐观锁）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 客户角色 ID
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后客户角色的响应视图。
pub async fn customer_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Extension(subject): Extension<RbacSubject>,
    Extension(UserID(user_id)): Extension<UserID>,
    Path(id): Path<String>,
    Json(req): Json<UpdateCustomerRequest>,
) -> Result<CustomerView> {
    ensure_customer_access(&state, &subject, &user_id, &id).await?;
    let view = CustomerService::new(state.db())
        .update_customer(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "删除客户",
    resource = "customer",
    action = "delete"
)]
/// 软删除客户角色。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 客户角色 ID
///
/// # 返回
/// 返回统一成功信封。
pub async fn customer_delete(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Extension(subject): Extension<RbacSubject>,
    Extension(UserID(user_id)): Extension<UserID>,
    Path(id): Path<String>,
) -> Result<()> {
    ensure_customer_access(&state, &subject, &user_id, &id).await?;
    CustomerService::new(state.db())
        .delete_customer(&id, &actor)
        .await?;
    Ok(ApiResponse::ok())
}

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "查询客户归属列表",
    resource = "customer_assignment",
    action = "list"
)]
/// 查询客户归属列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 客户角色 ID
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn customer_assignment_list(
    State(state): State<AppState>,
    Extension(subject): Extension<RbacSubject>,
    Extension(UserID(user_id)): Extension<UserID>,
    Path(id): Path<String>,
    Query(params): Query<CustomerAssignmentListParams>,
) -> Result<PageView<CustomerAssignmentView>> {
    ensure_customer_access(&state, &subject, &user_id, &id).await?;
    let page = CustomerAssignmentService::new(state.db())
        .customer_assignment_list(&id, &params)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "客户",
    group_desc = "客户角色与归属管理",
    desc = "调整客户归属",
    resource = "customer_assignment",
    action = "create"
)]
/// 调整客户归属（Assign 建立新归属并结束重叠旧归属；End 提前结束有效期）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 客户角色 ID
/// * `req` - 归属变更请求
///
/// # 返回
/// 返回本次变更涉及的归属行。
pub async fn customer_assignment_apply(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Extension(subject): Extension<RbacSubject>,
    Extension(UserID(user_id)): Extension<UserID>,
    Path(id): Path<String>,
    Json(req): Json<CustomerAssignmentRequest>,
) -> Result<Vec<CustomerAssignmentView>> {
    ensure_customer_access(&state, &subject, &user_id, &id).await?;
    let views = CustomerAssignmentService::new(state.db())
        .apply_assignment(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(views))
}

/// 计算客户对象中心允许动作；RBAC 与客户停用状态在可信服务端合并。
async fn allowed_actions(
    state: &AppState,
    subject: &RbacSubject,
    customer_active: bool,
) -> std::result::Result<Vec<String>, Error> {
    let candidates = [
        ("EDIT_CUSTOMER", "customer:update", None, false),
        ("UPLOAD_CONTRACT_PDF", "contract:create", None, true),
        ("CREATE_SALES_ORDER", "sales_order:create", None, true),
        ("OPEN_RECEIVABLE", "receivable_account:detail", None, false),
        ("MANAGE_ASSIGNMENTS", "customer_assignment:create", None, false),
        (
            "REVEAL_CONTACT",
            "party_contact:reveal",
            Some("party_contact:detail"),
            false,
        ),
        (
            "REVEAL_ADDRESS",
            "party_address:reveal",
            Some("party_address:detail"),
            false,
        ),
        (
            "REVEAL_BANK_ACCOUNT",
            "party_bank_account:reveal",
            Some("party_bank_account:detail"),
            false,
        ),
    ];
    let mut actions = Vec::new();
    for (action, permission, prerequisite, requires_active) in candidates {
        if requires_active && !customer_active {
            continue;
        }
        if let Some(prerequisite) = prerequisite {
            if !has_permission(state, subject, prerequisite).await? {
                continue;
            }
        }
        if has_permission(state, subject, permission).await? {
            actions.push(action.to_string());
        }
    }
    Ok(actions)
}

/// 判断当前 RBAC 主体是否覆盖给定权限。
async fn has_permission(
    state: &AppState,
    subject: &RbacSubject,
    permission: &str,
) -> std::result::Result<bool, Error> {
    let permission = Permission::parse(permission)?;
    state
        .rbac()
        .enforce(&subject.0, &permission)
        .await
        .map_err(Into::into)
}

/// 强制当前 RBAC 主体覆盖字段级权限。
async fn ensure_permission(
    state: &AppState,
    subject: &RbacSubject,
    permission: &str,
) -> std::result::Result<(), Error> {
    if has_permission(state, subject, permission).await? {
        return Ok(());
    }
    Err(Error::Forbidden("当前角色无权维护或查看该字段".to_string()))
}

/// 强制当前用户具备全量客户范围，或命中该客户的当前有效归属。
pub(crate) async fn ensure_customer_access(
    state: &AppState,
    subject: &RbacSubject,
    user_id: &str,
    customer_id: &str,
) -> std::result::Result<(), Error> {
    if has_permission(state, subject, "customer_scope:detail").await? {
        return Ok(());
    }
    if CustomerService::new(state.db())
        .customer_is_assigned_to(customer_id, user_id)
        .await?
    {
        return Ok(());
    }
    Err(Error::Forbidden("当前用户不在该客户的有效归属范围内".to_string()))
}
