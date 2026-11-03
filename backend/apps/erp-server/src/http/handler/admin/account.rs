use application_core::AuditActor;
use axum::extract::{Extension, Path, State};
use erp_identity::{AdminItem, AdminService, CreateAdminParams, UpdateAdminParams, UpdateAdminRoleParams};

use crate::http::errors::Result;
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

pub async fn create_admin(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateAdminParams>,
) -> Result<()> {
    AdminService::new(state.application().db(), state.application().rbac()).create_admin(req, actor).await?;

    Ok(ApiResponse::<()>::ok())
}

pub async fn list_admins(State(state): State<HttpState>) -> Result<Vec<AdminItem>> {
    let items = AdminService::new(state.application().db(), state.application().rbac()).admin_list().await?;
    Ok(ApiResponse::ok_with_data(items))
}

pub async fn update_admin(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(mut req): Json<UpdateAdminParams>,
) -> Result<()> {
    req.id = id;
    AdminService::new(state.application().db(), state.application().rbac()).update_admin(req, actor).await?;

    Ok(ApiResponse::<()>::ok())
}

pub async fn delete_admin(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    AdminService::new(state.application().db(), state.application().rbac()).delete_admin(id, actor).await?;

    Ok(ApiResponse::<()>::ok())
}

pub async fn update_admin_role(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(mut req): Json<UpdateAdminRoleParams>,
) -> Result<()> {
    req.id = id;
    AdminService::new(state.application().db(), state.application().rbac())
        .update_admin_role(req, actor)
        .await?;

    Ok(ApiResponse::<()>::ok())
}
