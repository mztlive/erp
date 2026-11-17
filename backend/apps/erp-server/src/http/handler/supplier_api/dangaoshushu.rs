use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_connector_dangaoshushu::{DangaoshushuReadQuery, SupplierReferenceTickets};
use erp_core::common::time::Instant;
use serde_json::Value;

use crate::http::errors::{Error, Result};
use crate::http::extractor::Query;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 读取蛋糕叔叔原始目录；先执行连接数据范围与固定供应商绑定校验。
/// # 参数
/// HTTP 状态、当前操作人、连接 ID 和固定只读查询。
/// # 返回
/// 原始供应商目录资料，不应用到正式商品/供给。
/// # 错误
/// 权限、数据范围、绑定、配置或供应商读取失败时返回对应错误。
pub async fn read(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(query): Query<DangaoshushuReadQuery>,
) -> Result<Value> {
    let connection = state.application().supplier_api_read_service().protocol_connection(&id, &actor).await?;
    let runtime = state
        .application()
        .dangaoshushu_runtime()
        .ok_or_else(|| Error::Unprocessable("蛋糕叔叔连接尚未配置".into()))?;
    let value =
        runtime.read(&connection, &query).await.map_err(|failure| Error::Unprocessable(failure.summary))?;
    Ok(ApiResponse::ok_with_data(value))
}

/// 签发端点和凭证绑定票据；复用连接范围并要求凭证管理权限。
/// # 参数
/// HTTP 状态、当前操作人和连接 ID。
/// # 返回
/// 五分钟有效的两个技术票据；不包含明文凭据。
/// # 错误
/// 权限、范围、配置、绑定或票据生成失败时返回对应错误。
pub async fn reference_tickets(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SupplierReferenceTickets> {
    let connection =
        state.application().supplier_api_read_service().reference_ticket_connection(&id, &actor).await?;
    let runtime = state
        .application()
        .dangaoshushu_runtime()
        .ok_or_else(|| Error::Unprocessable("蛋糕叔叔连接尚未配置".into()))?;
    let tickets = runtime
        .reference_tickets(&connection, Instant::now())
        .map_err(|failure| Error::Unprocessable(failure.summary))?;
    Ok(ApiResponse::ok_with_data(tickets))
}
