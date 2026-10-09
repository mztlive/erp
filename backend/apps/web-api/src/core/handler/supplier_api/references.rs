use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, Query, State};
use axum::http::HeaderValue;
use axum::http::header::CACHE_CONTROL;
use axum::response::{IntoResponse, Response};
use erp_supply::ports::supplier_reference_registry::{
    SupplierReferenceOption, SupplierReferenceOptionsQuery,
};

use crate::app_state::AppState;
use crate::core::errors::Error;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "查看供应商接口配置选项",
    resource = "supplier_api_connection",
    action = "view_reference_metadata"
)]
/// 列出当前后台连接可绑定的地址或密钥配置。
/// # 参数
/// 应用状态、操作人、连接 ID 和配置种类。
/// # 返回
/// 安全别名及短时绑定票据，不返回地址或密钥正文。
/// # 错误
/// 缺少绑定权限、连接不存在、参数无效或目录不可用时返回错误。
pub async fn options(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(query): Query<SupplierReferenceOptionsQuery>,
) -> Result<Response, Error> {
    let options =
        state.supplier_api_read_service().reference_options_for_actor(&id, query.kind, &actor).await?;
    Ok(options_response(options))
}

fn options_response(options: Vec<SupplierReferenceOption>) -> Response {
    let mut response = ApiResponse::ok_with_data(options).into_response();
    response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use tower::Service;

    use super::*;

    #[test]
    fn temporary_reference_options_must_not_be_http_cached() {
        let response = options_response(vec![]);
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-store");
    }

    #[tokio::test]
    async fn option_query_rejects_unknown_kind_and_extra_fields() {
        async fn parse(Query(_): Query<SupplierReferenceOptionsQuery>) -> StatusCode {
            StatusCode::OK
        }
        let mut router = Router::new().route("/", get(parse));
        for (query, expected) in [
            ("kind=endpoint", StatusCode::OK),
            ("kind=credential", StatusCode::OK),
            ("kind=unknown", StatusCode::BAD_REQUEST),
            ("kind=endpoint&supplier_id=other", StatusCode::BAD_REQUEST),
        ] {
            let response =
                router.call(Request::get(format!("/?{query}")).body(Body::empty()).unwrap()).await.unwrap();
            assert_eq!(response.status(), expected);
        }
    }
}
