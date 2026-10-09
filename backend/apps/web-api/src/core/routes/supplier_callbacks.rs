use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Extension, Router};
use erp_core::common::time::Instant;
use erp_processes::connectors::dangaoshushu::reception;
use erp_supply::ports::connector::callback::CallbackRequest;

use crate::app_state::AppState;
use crate::core::errors::Error;
use crate::core::rate_limit::RateLimiter;

const MAX_CALLBACK_BYTES: usize = 256 * 1024;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/callbacks/dangaoshushu/{connection_id}/{kind}", post(receive))
        .layer(DefaultBodyLimit::max(MAX_CALLBACK_BYTES))
        .layer(Extension(RateLimiter::new(300, 600, Duration::from_secs(60), 8)))
}

async fn receive(
    State(state): State<AppState>,
    Extension(limiter): Extension<RateLimiter>,
    Path((connection_id, kind)): Path<(String, String)>,
    body: Bytes,
) -> Result<Response, Error> {
    let runtime =
        state.dangaoshushu_runtime().ok_or_else(|| Error::NotFound("供应商推送接入未启用".into()))?;
    let _permit = limiter.admit("dangaoshushu")?;
    let path = format!("/callbacks/dangaoshushu/{connection_id}/{kind}");
    let request = CallbackRequest {
        method: "POST",
        path_and_query: &path,
        headers: &[],
        body: &body,
        received_at: Instant::now(),
    };
    let reply =
        reception::receive(&state.db(), &runtime, &state.sensitive_data(), &connection_id, &request).await?;
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
}
