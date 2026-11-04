//! 个人收件箱及 SSE 适配；收件人始终取认证上下文。
use std::convert::Infallible;
use std::time::{Duration, Instant};

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive};
use axum::response::{IntoResponse, Response, Sse};
use erp_notification::{Error as NotificationError, InboxPage, InboxQuery};
use futures_util::stream;
use serde::Serialize;

use crate::http::errors::{Error, Result};
use crate::http::extractor::Query;
use crate::http::middleware::{bearer_token, revalidate_session};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::notification::todo_count;

/// 读取本人最近消息。
/// # 参数
/// state 为共享状态，actor 为会话账号。
/// # 返回
/// 个人消息和未读数。
/// # 错误
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
/// # 参数
/// state 为共享状态，actor 为会话账号，id 为消息 ID。
/// # 返回
/// 空成功响应。
/// # 错误
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

#[derive(Debug, Serialize)]
struct NotificationSnapshot {
    unread_count: u64,
    todo_count: Option<i64>,
}

struct Feed {
    state: HttpState,
    actor: AuditActor,
    token: String,
    started: Instant,
    first: bool,
    closed: bool,
}

/// 建立 60 秒可重连 SSE；每轮复验会话及任务权限。
/// # 参数
/// state、actor 和 headers 为当前已认证请求。
/// # 返回
/// 首帧及后续 snapshot；错误显式发出 error/auth 事件后关闭。
/// # 错误
/// 缺少 Bearer 凭证返回 401。流内错误不伪装为零计数。
pub async fn events(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
) -> Response {
    let Some(token) = bearer_token(&headers) else {
        return ApiResponse::<()>::unauthorized().into_response();
    };
    let feed =
        Feed { state, actor, token: token.to_owned(), started: Instant::now(), first: true, closed: false };
    let events = stream::unfold(feed, |mut feed| async move {
        if feed.closed || feed.started.elapsed() >= Duration::from_secs(60) {
            return None;
        }
        if !feed.first {
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
        feed.first = false;
        let event = next_event(&mut feed).await;
        Some((Ok::<_, Infallible>(event), feed))
    });
    let mut response = Sse::new(events).keep_alive(KeepAlive::default()).into_response();
    response.headers_mut().insert("X-Accel-Buffering", "no".parse().expect("静态合法响应头"));
    response
}

async fn next_event(feed: &mut Feed) -> Event {
    if let Err(error) = revalidate_session(&feed.state, &feed.token).await {
        feed.closed = true;
        return Event::default()
            .event(if error.status == 401 { "auth" } else { "error" })
            .data("unavailable");
    }
    match snapshot(&feed.state, &feed.actor).await {
        Ok(snapshot) => Event::default().event("snapshot").json_data(snapshot).expect("简单计数可序列化"),
        Err(error) => {
            tracing::warn!(account = feed.actor.account(), request_id = feed.actor.request_id(), error = %error, "通知实时计数查询失败");
            feed.closed = true;
            Event::default().event("error").data("unavailable")
        },
    }
}

async fn snapshot(state: &HttpState, actor: &AuditActor) -> std::result::Result<NotificationSnapshot, Error> {
    let unread_count =
        state.application().notifications().unread_count(actor.id()).await.map_err(http_error)?;
    let todo_count = todo_count(state.application(), actor).await?;
    Ok(NotificationSnapshot { unread_count, todo_count })
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use axum::http::StatusCode;

    use super::*;

    #[tokio::test]
    async fn notification_snapshot_uses_named_sse_event_and_preserves_unknown_count() {
        let event = Event::default()
            .event("snapshot")
            .json_data(NotificationSnapshot { unread_count: 3, todo_count: None })
            .unwrap();
        let response = Sse::new(stream::iter([Ok::<_, Infallible>(event)])).into_response();
        assert_eq!(response.headers()["content-type"], "text/event-stream");
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("event: snapshot"));
        let raw = body.lines().find_map(|line| line.strip_prefix("data: ")).unwrap();
        let data: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert_eq!(data["unread_count"], 3);
        assert!(data["todo_count"].is_null());
    }

    #[test]
    fn missing_or_foreign_messages_use_not_found_and_invalid_cursor_is_bad_request() {
        assert_eq!(http_error(NotificationError::NotFound).into_response().status(), StatusCode::NOT_FOUND);
        assert_eq!(
            http_error(NotificationError::Invalid("分页条件无效，请重新打开收件箱")).into_response().status(),
            StatusCode::BAD_REQUEST
        );
    }
}
