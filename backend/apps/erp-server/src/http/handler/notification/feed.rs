//! 订阅先于快照；变化驱动刷新，心跳只验证会话，不轮询业务计数。
use std::collections::BTreeSet;
use std::convert::Infallible;
use std::time::Duration;

use application_core::AuditActor;
use axum::Extension;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive};
use axum::response::{IntoResponse, Response, Sse};
use erp_notification::{RefreshSignal, RefreshTopic};
use futures_util::stream;
use serde::Serialize;
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};
use tokio::time::Instant;

use super::http_error;
use crate::http::errors::Error;
use crate::http::middleware::{bearer_token, revalidate_session};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::notification::todo_count;

#[derive(Debug, Serialize)]
struct NotificationSnapshot {
    unread_count: u64,
    todo_count: Option<i64>,
    topics: BTreeSet<RefreshTopic>,
}
#[derive(Debug, Serialize)]
struct RefreshFrame {
    topics: BTreeSet<RefreshTopic>,
}
struct Feed {
    state: HttpState,
    actor: AuditActor,
    token: String,
    receiver: Receiver<RefreshSignal>,
    deadline: Instant,
    heartbeat: Instant,
    first: bool,
    closed: bool,
}

/// 建立可重连的单连接通知流，身份只取认证上下文。
/// # 参数
/// state、actor 和 headers 为当前已认证请求。
/// # 返回
/// 首帧完整同步；后续按业务变化发送 snapshot 或 refresh，15 秒复验会话。
/// # 错误
/// 凭证缺失返回 401；订阅、查询或会话错误显式发送 error/auth 后关闭。
pub async fn events(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
) -> Response {
    let Some(token) = bearer_token(&headers) else {
        return ApiResponse::<()>::unauthorized().into_response();
    };
    let receiver = state.notification_feed.hub.subscribe();
    let now = Instant::now();
    let feed = Feed {
        state,
        actor,
        token: token.to_owned(),
        receiver,
        deadline: now + Duration::from_secs(60),
        heartbeat: now + Duration::from_secs(15),
        first: true,
        closed: false,
    };
    let events = stream::unfold(feed, |mut feed| async move {
        if feed.closed {
            return None;
        }
        let event = next_event(&mut feed).await?;
        Some((Ok::<_, Infallible>(event), feed))
    });
    let mut response = Sse::new(events).keep_alive(KeepAlive::default()).into_response();
    response.headers_mut().insert("X-Accel-Buffering", "no".parse().expect("静态合法响应头"));
    response
}

async fn next_event(feed: &mut Feed) -> Option<Event> {
    let topics = if feed.first {
        feed.first = false;
        RefreshTopic::all()
    } else {
        match changes(feed).await {
            Some(Ok(topics)) => topics,
            Some(Err(())) => return Some(failure(feed, "error")),
            None => return None,
        }
    };
    if let Err(error) = revalidate_session(&feed.state, &feed.token).await {
        return Some(failure(feed, if error.status == 401 { "auth" } else { "error" }));
    }
    if !feed.state.notification_feed.hub.is_available() {
        return Some(failure(feed, "error"));
    }
    feed.heartbeat = Instant::now() + Duration::from_secs(15);
    if topics.is_empty() {
        return Some(Event::default().comment("heartbeat"));
    }
    match frame(&feed.state, &feed.actor, topics).await {
        Ok(event) => Some(event),
        Err(error) => {
            tracing::warn!(account = feed.actor.account(), request_id = feed.actor.request_id(), %error, "通知同步失败");
            Some(failure(feed, "error"))
        },
    }
}

async fn changes(feed: &mut Feed) -> Option<Result<BTreeSet<RefreshTopic>, ()>> {
    let mut topics = BTreeSet::new();
    loop {
        tokio::select! {
            biased;
            _ = tokio::time::sleep_until(feed.deadline) => return None,
            _ = tokio::time::sleep_until(feed.heartbeat) => return Some(Ok(topics)),
            signal = feed.receiver.recv() => {
                match signal {
                    Ok(signal) if signal.collect(feed.actor.id(), &mut topics) => {},
                    Err(RecvError::Lagged(_)) => topics.extend(RefreshTopic::all()),
                    _ => return Some(Err(())),
                }
            }
        }
        if !topics.is_empty() {
            break;
        }
    }
    // 有界合并批量事务和进度变化；不因连续写入无限延迟推送。
    tokio::time::sleep(Duration::from_millis(200)).await;
    Some(drain(&mut feed.receiver, feed.actor.id(), topics))
}

fn drain(
    receiver: &mut Receiver<RefreshSignal>,
    account: &str,
    mut topics: BTreeSet<RefreshTopic>,
) -> Result<BTreeSet<RefreshTopic>, ()> {
    for _ in 0..256 {
        match receiver.try_recv() {
            Ok(signal) if signal.collect(account, &mut topics) => {},
            Err(TryRecvError::Lagged(_)) => topics.extend(RefreshTopic::all()),
            Err(TryRecvError::Empty) => break,
            _ => return Err(()),
        }
    }
    Ok(topics)
}
fn failure(feed: &mut Feed, event: &str) -> Event {
    feed.closed = true;
    Event::default().event(event).data("unavailable")
}
fn needs_snapshot(topics: &BTreeSet<RefreshTopic>) -> bool {
    [RefreshTopic::Inbox, RefreshTopic::Tasks, RefreshTopic::Authorization]
        .iter()
        .any(|topic| topics.contains(topic))
}
async fn frame(
    state: &HttpState,
    actor: &AuditActor,
    topics: BTreeSet<RefreshTopic>,
) -> Result<Event, Error> {
    if !needs_snapshot(&topics) {
        return Ok(Event::default()
            .event("refresh")
            .json_data(RefreshFrame { topics })
            .expect("固定主题可序列化"));
    }
    let unread_count =
        state.application().notifications().unread_count(actor.id()).await.map_err(http_error)?;
    let todo_count = todo_count(state.application(), actor).await?;
    Ok(Event::default()
        .event("snapshot")
        .json_data(NotificationSnapshot { unread_count, todo_count, topics })
        .expect("简单计数可序列化"))
}
#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use erp_notification::RefreshHub;

    use super::*;
    #[tokio::test]
    async fn snapshot_preserves_unknown_and_refreshes_unchanged_counts() {
        let event = Event::default()
            .event("snapshot")
            .json_data(NotificationSnapshot {
                unread_count: 3,
                todo_count: None,
                topics: [RefreshTopic::Tasks].into(),
            })
            .unwrap();
        let response = Sse::new(stream::iter([Ok::<_, Infallible>(event)])).into_response();
        let body = String::from_utf8(to_bytes(response.into_body(), 4096).await.unwrap().to_vec()).unwrap();
        let raw = body.lines().find_map(|line| line.strip_prefix("data: ")).unwrap();
        let data: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert!(data["todo_count"].is_null());
        assert_eq!(data["topics"], serde_json::json!(["tasks"]));
    }
    #[test]
    fn coalesces_bursts_and_keeps_personal_events_separate() {
        let hub = RefreshHub::default();
        let mut receiver = hub.subscribe();
        for _ in 0..20 {
            hub.publish(RefreshSignal::Changed { topic: RefreshTopic::Jobs, recipient: None });
        }
        hub.publish(RefreshSignal::Changed { topic: RefreshTopic::Inbox, recipient: Some("other".into()) });
        let topics = drain(&mut receiver, "me", BTreeSet::new()).unwrap();
        assert_eq!(topics, [RefreshTopic::Jobs].into());
        assert!(!needs_snapshot(&topics));
        hub.publish(RefreshSignal::Resync);
        assert_eq!(drain(&mut receiver, "me", BTreeSet::new()).unwrap(), RefreshTopic::all());
        hub.publish(RefreshSignal::Unavailable);
        assert!(drain(&mut receiver, "me", BTreeSet::new()).is_err());
    }
    #[test]
    fn lagged_queue_refreshes_all_topics_and_closed_queue_fails() {
        let hub = RefreshHub::default();
        let mut receiver = hub.subscribe();
        for _ in 0..300 {
            hub.publish(RefreshSignal::Changed { topic: RefreshTopic::Jobs, recipient: None });
        }
        assert_eq!(drain(&mut receiver, "me", BTreeSet::new()).unwrap(), RefreshTopic::all());
        while receiver.try_recv().is_ok() {}
        drop(hub);
        assert!(drain(&mut receiver, "me", BTreeSet::new()).is_err());
    }
}
