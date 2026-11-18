//! 个人收件箱及 SSE 适配；收件人始终取认证上下文。

use application_core::AuditActor;
use axum::Extension;
#[cfg(test)]
use axum::response::IntoResponse;

mod feed;
use axum::extract::{Path, State};
use erp_notification::{Error as NotificationError, InboxPage, InboxQuery};
pub use feed::events;

use crate::http::errors::{Error, Result};
use crate::http::extractor::Query;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 读取本人最近消息。
/// 个人消息和未读数。
/// 仓储失败返回统一 HTTP 错误。
pub async fn inbox(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<InboxQuery>,
) -> Result<InboxPage> {
    let page = state.application().notifications().inbox(actor.id(), query).await.map_err(http_error)?;
    Ok(ApiResponse::ok_with_data(page))
}

/// 标记本人消息已读。
/// 空成功响应。
/// 不存在或非本人消息均为 404。
pub async fn mark_read(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    state.application().notifications().mark_read(actor.id(), &id).await.map_err(http_error)?;
    Ok(ApiResponse::ok_with_data(()))
}

fn http_error(error: NotificationError) -> Error {
    match error {
        NotificationError::NotFound => Error::NotFound("消息不存在，请刷新收件箱".into()),
        NotificationError::Invalid(message) => Error::BadRequest(message.into()),
        other => Error::Internal(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use super::*;
    #[test]
    fn missing_or_foreign_messages_use_not_found_and_invalid_cursor_is_bad_request() {
        assert_eq!(http_error(NotificationError::NotFound).into_response().status(), StatusCode::NOT_FOUND);
        assert_eq!(
            http_error(NotificationError::Invalid("分页条件无效，请重新打开收件箱")).into_response().status(),
            StatusCode::BAD_REQUEST
        );
    }
}
