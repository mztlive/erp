use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, OriginalUri, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{MethodRouter, post};
use axum::{Extension, Router};
use erp_core::common::time::Instant;
use erp_supply::entity::supplier_api::SupplierConnectorType;
use erp_supply::ports::connector::callback::CallbackRequest;

use crate::app_state::AppState;
use crate::core::errors::Error;
use crate::core::rate_limit::RateLimiter;

const MAX_CALLBACK_BYTES: usize = 256 * 1024;

pub(super) fn routes() -> Router<AppState> {
    callback_router(post(receive))
}

fn callback_router<S: Clone + Send + Sync + 'static>(endpoint: MethodRouter<S>) -> Router<S> {
    Router::new()
        .route("/callbacks/{protocol}/{connection_id}/{kind}", endpoint)
        .layer(DefaultBodyLimit::max(MAX_CALLBACK_BYTES))
        .layer(Extension(RateLimiter::new(300, 600, Duration::from_secs(60), 8)))
}

async fn receive(
    State(state): State<AppState>,
    Extension(limiter): Extension<RateLimiter>,
    Path((protocol, connection_id, _kind)): Path<(SupplierConnectorType, String, String)>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Error> {
    let _permit = limiter.admit(protocol.label())?;
    let path = uri.path_and_query().map_or(uri.path(), |value| value.as_str());
    let headers =
        headers.iter().map(|(name, value)| (name.to_string(), value.as_bytes().to_vec())).collect::<Vec<_>>();
    let request = CallbackRequest {
        method: "POST",
        path_and_query: path,
        headers: &headers,
        body: &body,
        received_at: Instant::now(),
    };
    let reply = state.receive_supplier_callback(protocol, &connection_id, &request).await?;
    let status =
        StatusCode::from_u16(reply.status).map_err(|_| Error::Internal("供应商应答状态无效".into()))?;
    Ok((status, [(header::CONTENT_TYPE, reply.content_type)], reply.body).into_response())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use tower::Service;

    use super::*;
    #[tokio::test]
    async fn callback_rejects_oversized_body_before_handler() {
        async fn accept(_: Bytes) -> StatusCode {
            StatusCode::OK
        }
        let mut router =
            Router::new().route("/", post(accept)).layer(DefaultBodyLimit::max(MAX_CALLBACK_BYTES));
        let response = router
            .call(Request::post("/").body(Body::from(vec![b'x'; MAX_CALLBACK_BYTES + 1])).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
    #[tokio::test]
    async fn callback_route_preserves_protocol_original_query_headers_and_bytes() {
        async fn inspect(
            Path((protocol, connection, kind)): Path<(SupplierConnectorType, String, String)>,
            OriginalUri(uri): OriginalUri,
            headers: HeaderMap,
            body: Bytes,
        ) -> Bytes {
            assert_eq!(protocol, SupplierConnectorType::Dangaoshushu);
            assert_eq!((connection.as_str(), kind.as_str()), ("c", "price"));
            assert_eq!(uri.path_and_query().unwrap().as_str(), "/callbacks/dangaoshushu/c/price?a=%2F");
            assert_eq!(headers["x-signature"], "original");
            assert_eq!(body.as_ref(), b"{  \"value\": 1 }");
            body
        }
        let mut router = callback_router(post(inspect));
        let response = router
            .call(
                Request::post("/callbacks/dangaoshushu/c/price?a=%2F")
                    .header("x-signature", "original")
                    .body(Body::from("{  \"value\": 1 }"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = router
            .call(Request::post("/callbacks/unknown/c/price").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
