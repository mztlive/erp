//! 新建连接的接口类型目录；仅包含当前程序已实现的协议。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::State;
use erp_supply::dto::supplier_api::SupplierConnectorTypeOption;

use crate::http::errors::Result;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 在创建连接资格内列出当前已实现的接口类型。
/// 稳定协议代码及中文名称，不表示对应部署已配置凭据。
/// 身份、创建资格或授权读取失败时返回统一错误。
pub async fn options(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
) -> Result<Vec<SupplierConnectorTypeOption>> {
    let options = state.application().supplier_api_read_service().connector_types(&actor).await?;
    Ok(ApiResponse::ok_with_data(options))
}
