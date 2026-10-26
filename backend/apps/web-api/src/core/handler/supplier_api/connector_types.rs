//! 新建连接的接口类型目录；仅包含当前程序已实现的协议。
use erp_supply::dto::supplier_api::SupplierConnectorTypeOption;
use erp_supply::entity::supplier_api::SupplierConnectorType;

use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "创建供应商 API 连接",
    resource = "supplier_api_connection",
    action = "create"
)]
/// 列出新建连接可选择的已实现接口类型。
/// # 参数
/// 无；路由使用创建连接权限。
/// # 返回
/// 稳定协议代码及中文名称，不表示对应部署已配置凭据。
/// # 错误
/// 不返回业务错误；鉴权拒绝由路由中间件处理。
pub async fn options() -> ApiResponse<Vec<SupplierConnectorTypeOption>> {
    ApiResponse::ok_with_data(
        SupplierConnectorType::all().into_iter().map(SupplierConnectorTypeOption::from).collect(),
    )
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use serde_json::{Value, from_slice};
    use tower::Service;

    use super::*;

    #[tokio::test]
    async fn interface_options_return_supported_codes_and_labels() {
        let mut router = Router::new().route("/", get(options));
        let response = router.call(Request::get("/").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 8192).await.unwrap();
        let value: Value = from_slice(&body).unwrap();
        assert_eq!(value["data"][0]["code"], "dangaoshushu");
        assert_eq!(value["data"][0]["label"], "蛋糕叔叔 API");
    }
}
