//! 域 D08 `customer` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `erp_customer` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use application_core::AuditActor;
use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use erp_core::common::time::Instant;
use erp_customer::{
    CreateCustomerRequest, CustomerAssignmentListParams, CustomerAssignmentRequest, CustomerAssignmentView,
    CustomerDetailView, CustomerListParams, CustomerListView, CustomerProfileDetailView,
    CustomerProfileMutationView, CustomerScope, CustomerSensitiveRevealView, CustomerView, PageView,
    RevealCustomerSensitiveRequest, SaveCustomerProfileRequest, SensitiveFieldKind, UpdateCustomerRequest,
};
use erp_identity::Permission;
use erp_read_models::{CustomerCenterReadService, CustomerCenterReceivableView, CustomerCenterRelatedView};

use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::middleware::RbacSubject;
use crate::core::response::ApiResponse;

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
    erp_processes::adapters::customer_access(state.db(), state.rbac()).ensure_create(&actor).await?;
    let view = state.customer_profile_service().create(req, &actor).await?;
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
    Path(id): Path<String>,
    Json(req): Json<SaveCustomerProfileRequest>,
) -> Result<CustomerProfileMutationView> {
    ensure_customer_access(&state, &actor, "update", &id).await?;
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
    let view = state.customer_profile_service().update(&id, req, &actor).await?;
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
    Extension(actor): Extension<AuditActor>,
    Extension(subject): Extension<RbacSubject>,
    Path(id): Path<String>,
) -> Result<CustomerProfileDetailView> {
    ensure_customer_access(&state, &actor, "detail", &id).await?;
    let mut view = state.customer_profile_service().detail(&id).await?;
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
        SensitiveFieldKind::ContactMobile => can_reveal_contact,
        SensitiveFieldKind::Address => can_reveal_address,
        SensitiveFieldKind::BankAccountNumber => can_reveal_bank,
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
    Extension(actor): Extension<AuditActor>,
    Extension(subject): Extension<RbacSubject>,
    Path(id): Path<String>,
) -> Result<CustomerCenterRelatedView> {
    ensure_customer_access(&state, &actor, "detail", &id).await?;
    ensure_permission(&state, &subject, "contract:list").await?;
    ensure_permission(&state, &subject, "sales_order:list").await?;
    let view = CustomerCenterReadService::new(state.db()).related(&id, &actor).await?;
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
    Extension(actor): Extension<AuditActor>,
    Extension(subject): Extension<RbacSubject>,
    Path(id): Path<String>,
) -> Result<CustomerCenterReceivableView> {
    ensure_customer_access(&state, &actor, "detail", &id).await?;
    ensure_permission(&state, &subject, "receivable_account:list").await?;
    let view = CustomerCenterReadService::new(state.db()).receivable(&id, &actor).await?;
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
    Extension(actor): Extension<AuditActor>,
    Path(idempotency_key): Path<String>,
) -> Result<Option<CustomerProfileMutationView>> {
    let view = state.customer_profile_service().command_result(&idempotency_key).await?;
    if let Some(result) = &view
        && result.initiated_by != actor.id()
    {
        ensure_customer_access(&state, &actor, "detail", &result.customer_id).await?;
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
    Json(req): Json<RevealCustomerSensitiveRequest>,
) -> Result<CustomerSensitiveRevealView> {
    let now =
        u64::try_from(Instant::now().unix_secs()).map_err(|_| Error::Internal("系统时间非法".to_string()))?;
    let scope = state.sensitive_data().verify_reveal_token(&req.reveal_token, now)?;
    let (detail_permission, reveal_permission) = match scope.kind {
        erp_party::SensitiveFieldKind::ContactMobile => ("party_contact:detail", "party_contact:reveal"),
        erp_party::SensitiveFieldKind::Address => ("party_address:detail", "party_address:reveal"),
        erp_party::SensitiveFieldKind::BankAccountNumber => {
            ("party_bank_account:detail", "party_bank_account:reveal")
        },
    };
    ensure_customer_access(&state, &actor, "detail", &scope.supplier_id).await?;
    ensure_permission(&state, &subject, detail_permission).await?;
    ensure_permission(&state, &subject, reveal_permission).await?;
    let view = state.customer_profile_service().reveal_sensitive(req, &actor).await?;
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
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<CustomerListParams>,
) -> Result<CustomerListView> {
    reject_all_authorized_on_regular_list(params.scope)?;
    let page = state.customer_service().customer_list(&params, &actor).await?;
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
    Extension(actor): Extension<AuditActor>,
    Extension(subject): Extension<RbacSubject>,
    Query(mut params): Query<CustomerListParams>,
) -> Result<CustomerListView> {
    ensure_permission(&state, &subject, "customer:list").await?;
    params.scope = CustomerScope::AllAuthorized;
    let page = state.customer_service().customer_list(&params, &actor).await?;
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
    let view = state.customer_service().create_customer(req, &actor).await?;
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
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<CustomerDetailView> {
    let view = state.customer_service().customer_detail(&id, &actor).await?;
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
    Path(id): Path<String>,
    Json(req): Json<UpdateCustomerRequest>,
) -> Result<CustomerView> {
    let view = state.customer_service().update_customer(&id, req, &actor).await?;
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
    Path(id): Path<String>,
) -> Result<()> {
    ensure_customer_access(&state, &actor, "delete", &id).await?;
    erp_processes::delete_customer(state.db(), state.rbac(), id, actor).await?;
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
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(params): Query<CustomerAssignmentListParams>,
) -> Result<PageView<CustomerAssignmentView>> {
    ensure_customer_access(&state, &actor, "detail", &id).await?;
    let page = state.customer_assignment_service().customer_assignment_list(&id, &params).await?;
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
    Path(id): Path<String>,
    Json(req): Json<CustomerAssignmentRequest>,
) -> Result<Vec<CustomerAssignmentView>> {
    ensure_customer_access(&state, &actor, "update", &id).await?;
    let views = state.customer_assignment_service().apply_assignment(&id, req, &actor).await?;
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
        ("REVEAL_CONTACT", "party_contact:reveal", Some("party_contact:detail"), false),
        ("REVEAL_ADDRESS", "party_address:reveal", Some("party_address:detail"), false),
        ("REVEAL_BANK_ACCOUNT", "party_bank_account:reveal", Some("party_bank_account:detail"), false),
    ];
    let mut actions = Vec::new();
    for (action, permission, prerequisite, requires_active) in candidates {
        if requires_active && !customer_active {
            continue;
        }
        if let Some(prerequisite) = prerequisite
            && !has_permission(state, subject, prerequisite).await?
        {
            continue;
        }
        if has_permission(state, subject, permission).await? {
            actions.push(action.to_string());
        }
    }
    Ok(actions)
}

/// 判断当前 RBAC 主体是否覆盖给定权限。
pub(crate) async fn has_permission(
    state: &AppState,
    subject: &RbacSubject,
    permission: &str,
) -> std::result::Result<bool, Error> {
    let permission = Permission::parse(permission)?;
    state.rbac().enforce(&subject.0, &permission).await.map_err(Into::into)
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

/// 常规列表拒绝全量授权口径，避免只凭 `customer:list` 绕过专用入口。
///
/// # 参数
/// * `scope` - 请求中的目录范围标签
///
/// # 返回
/// 非 `all_authorized` 时成功。
///
/// # 错误
/// 常规入口携带全量授权时返回校验错误。
///
/// # 关键业务约束
/// 全部有权客户必须走 `customer_scope:detail` 专用入口。
fn reject_all_authorized_on_regular_list(scope: CustomerScope) -> std::result::Result<(), Error> {
    if scope == CustomerScope::AllAuthorized {
        return Err(Error::BadRequest("全部有权客户请使用专用查询入口".to_string()));
    }
    Ok(())
}

/// 按资源动作重验客户对象范围；缺动作拒绝，缺范围不得补公司。
pub(crate) async fn ensure_customer_access(
    state: &AppState,
    actor: &AuditActor,
    action: &str,
    customer_id: &str,
) -> std::result::Result<(), Error> {
    erp_processes::adapters::customer_access(state.db(), state.rbac())
        .require(actor, action, customer_id)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use axum::http::Uri;

    use super::*;

    /// 客户 Query 解码必须消费范围版本与组织筛选，并拒绝旧姓名参数。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 未知参数不得被忽略后返回全部客户。
    #[test]
    fn customer_scope_version_and_org_filters_decode_from_url() {
        let uri: Uri = "/?page=2&page_size=25&scope_version=v1&owner_user_ids=a,b&org_unit_ids=org-1&include_descendants=true"
            .parse()
            .unwrap();
        let Query(params) = Query::<CustomerListParams>::try_from_uri(&uri).unwrap();
        assert_eq!(params.page, Some(2));
        assert_eq!(params.scope_version.as_deref(), Some("v1"));
        assert!(params.owner_user_ids.is_some());
        assert_eq!(params.org_unit_ids.unwrap().as_slice(), &["org-1".to_string()]);
        assert_eq!(params.include_descendants, Some(true));
        let legacy: Uri = "/?owner=张三".parse().unwrap();
        assert!(Query::<CustomerListParams>::try_from_uri(&legacy).is_err());
    }

    /// 常规列表不得接受 all_authorized，专用入口才覆盖该口径。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 组织范围内账号不能靠常规列表看到全部有权客户。
    #[test]
    fn regular_list_rejects_all_authorized_scope() {
        assert!(reject_all_authorized_on_regular_list(CustomerScope::Mine).is_ok());
        assert!(reject_all_authorized_on_regular_list(CustomerScope::Collaborating).is_ok());
        assert!(reject_all_authorized_on_regular_list(CustomerScope::Assigned).is_ok());
        assert!(reject_all_authorized_on_regular_list(CustomerScope::AllAuthorized).is_err());
    }
}
