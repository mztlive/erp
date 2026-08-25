//! 域 D09 `supplier` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::supplier` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use axum::{
    extract::{Multipart, Path, Query, State},
    Extension, Json,
};
use entities::file_asset::SensitivityClass;
use services::audit::AuditActor;
use services::supplier::{
    profile::SupplierProfileService, PageView, RevealSupplierSensitiveRequest, SaveSupplierProfileRequest,
    SupplierDetailView, SupplierListParams, SupplierProfileMutationView, SupplierSensitiveRevealView,
    SupplierService, SupplierView,
};

use crate::{
    app_state::AppState,
    core::{
        errors::Result,
        handler::file_asset::{
            delete_pending_asset_objects, extract_command_with_asset_files, should_compensate_pending_assets,
            store_pending_asset_files,
        },
        response::ApiResponse,
    },
};

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "创建完整供应商资料",
    resource = "supplier",
    action = "create"
)]
/// 原子创建完整供应商资料。
pub async fn supplier_profile_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<SaveSupplierProfileRequest>,
) -> Result<SupplierProfileMutationView> {
    let view = SupplierProfileService::new(state.db(), state.sensitive_data())
        .create(req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "一次创建供应商资料及资质文件",
    resource = "supplier",
    action = "create"
)]
/// 一次接收供应商根命令与资质文件，并原子登记文件元数据和完整供应商资料。
pub async fn supplier_profile_create_with_assets(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    mut multipart: Multipart,
) -> Result<SupplierProfileMutationView> {
    let (req, files) = extract_command_with_asset_files::<SaveSupplierProfileRequest>(&mut multipart).await?;
    let pending = store_pending_asset_files(&state, files, supplier_asset_sensitivity).await?;
    let result = SupplierProfileService::new(state.db(), state.sensitive_data())
        .create_with_assets(req, pending.clone(), &actor)
        .await;
    match result {
        Ok(result) => {
            if !result.assets_committed {
                delete_pending_asset_objects(&state, &pending).await;
            }
            Ok(ApiResponse::ok_with_data(result.view))
        }
        Err(error) => {
            if should_compensate_pending_assets(&error) {
                delete_pending_asset_objects(&state, &pending).await;
            }
            Err(error.into())
        }
    }
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "修订完整供应商资料",
    resource = "supplier",
    action = "update"
)]
/// 原子修订完整供应商资料。
pub async fn supplier_profile_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SaveSupplierProfileRequest>,
) -> Result<SupplierProfileMutationView> {
    let view = SupplierProfileService::new(state.db(), state.sensitive_data())
        .update(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "一次修订供应商资料及资质文件",
    resource = "supplier",
    action = "update"
)]
/// 一次接收供应商修订根命令与资质文件，并原子登记文件元数据和全部资料变化。
pub async fn supplier_profile_update_with_assets(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> Result<SupplierProfileMutationView> {
    let (req, files) = extract_command_with_asset_files::<SaveSupplierProfileRequest>(&mut multipart).await?;
    let pending = store_pending_asset_files(&state, files, supplier_asset_sensitivity).await?;
    let result = SupplierProfileService::new(state.db(), state.sensitive_data())
        .update_with_assets(&id, req, pending.clone(), &actor)
        .await;
    match result {
        Ok(result) => {
            if !result.assets_committed {
                delete_pending_asset_objects(&state, &pending).await;
            }
            Ok(ApiResponse::ok_with_data(result.view))
        }
        Err(error) => {
            if should_compensate_pending_assets(&error) {
                delete_pending_asset_objects(&state, &pending).await;
            }
            Err(error.into())
        }
    }
}

/// 从受控临时引用派生供应商资质文件的最低敏感级别。
fn supplier_asset_sensitivity(reference: &str) -> SensitivityClass {
    if reference.contains(":legal_person_id:") {
        SensitivityClass::HighlySensitive
    } else {
        SensitivityClass::Sensitive
    }
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "查询供应商资料保存结果",
    resource = "supplier",
    action = "detail"
)]
/// 按幂等键查询已成功的供应商资料命令结果。
pub async fn supplier_profile_command_detail(
    State(state): State<AppState>,
    Path(idempotency_key): Path<String>,
) -> Result<Option<SupplierProfileMutationView>> {
    let view = SupplierProfileService::new(state.db(), state.sensitive_data())
        .command_result(&idempotency_key)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "短时查看供应商敏感字段",
    resource = "supplier_sensitive",
    action = "reveal"
)]
/// 按详情接口签发的短时令牌揭示单个敏感字段。
pub async fn supplier_sensitive_reveal(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<RevealSupplierSensitiveRequest>,
) -> Result<SupplierSensitiveRevealView> {
    let view = SupplierProfileService::new(state.db(), state.sensitive_data())
        .reveal_sensitive(req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "查询供应商列表",
    resource = "supplier",
    action = "list"
)]
/// 查询供应商列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`keyword`/`party_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn supplier_list(
    State(state): State<AppState>,
    Query(params): Query<SupplierListParams>,
) -> Result<PageView<SupplierView>> {
    let page = SupplierService::new(state.db()).supplier_list(&params).await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "查询供应商详情",
    resource = "supplier",
    action = "detail"
)]
/// 查询供应商详情（供应商 + 当前商务结算版本 + 主体编号）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 供应商角色 ID
///
/// # 返回
/// 返回供应商详情视图。
pub async fn supplier_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<SupplierDetailView> {
    let view = SupplierService::with_sensitive_data(state.db(), state.sensitive_data())
        .supplier_detail(&id)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商",
    group_desc = "供应商角色、商务结算版本、能力与资质管理",
    desc = "删除供应商",
    resource = "supplier",
    action = "delete"
)]
/// 软删除供应商角色。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 供应商角色 ID
///
/// # 返回
/// 返回统一成功信封。
pub async fn supplier_delete(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    SupplierService::new(state.db())
        .delete_supplier(&id, &actor)
        .await?;
    Ok(ApiResponse::ok())
}
