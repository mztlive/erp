use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, Query, State};
use erp_core::common::time::Instant;
use erp_processes::connectors::dangaoshushu::{DangaoshushuReadQuery, SupplierReferenceTickets};
use persistence_core::NoTransaction;
use serde_json::Value;

use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "读取供应商协议目录资料",
    resource = "supplier_api_connection",
    action = "protocol_read"
)]
/// 读取蛋糕叔叔原始目录；先执行连接数据范围与固定供应商绑定校验。
/// # 参数
/// HTTP 状态、当前操作人、连接 ID 和固定只读查询。
/// # 返回
/// 原始供应商目录资料，不应用到正式商品/供给。
/// # 错误
/// 权限、数据范围、绑定、配置或供应商读取失败时返回对应错误。
pub async fn read(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(query): Query<DangaoshushuReadQuery>,
) -> Result<Value> {
    state.supplier_api_read_service().connection_detail_for_actor(&id, &actor).await?;
    let runtime =
        state.dangaoshushu_runtime().ok_or_else(|| Error::Unprocessable("蛋糕叔叔连接尚未配置".into()))?;
    let connection = state.supplier_api_service().load_connection(&id, &mut NoTransaction).await?;
    let value =
        runtime.read(&connection, &query).await.map_err(|failure| Error::Unprocessable(failure.summary))?;
    Ok(ApiResponse::ok_with_data(value))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "签发供应商技术绑定票据",
    resource = "supplier_api_connection",
    action = "manage_credential_reference"
)]
/// 签发端点和凭证绑定票据；复用连接范围并要求凭证管理权限。
/// # 参数
/// HTTP 状态、当前操作人和连接 ID。
/// # 返回
/// 五分钟有效的两个技术票据；不包含明文凭据。
/// # 错误
/// 权限、范围、配置、绑定或票据生成失败时返回对应错误。
pub async fn reference_tickets(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SupplierReferenceTickets> {
    state.supplier_api_read_service().connection_detail_for_actor(&id, &actor).await?;
    let runtime =
        state.dangaoshushu_runtime().ok_or_else(|| Error::Unprocessable("蛋糕叔叔连接尚未配置".into()))?;
    let connection = state.supplier_api_service().load_connection(&id, &mut NoTransaction).await?;
    let tickets = runtime
        .reference_tickets(&connection, Instant::now())
        .map_err(|failure| Error::Unprocessable(failure.summary))?;
    Ok(ApiResponse::ok_with_data(tickets))
}
