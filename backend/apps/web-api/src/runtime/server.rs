//! HTTP 与 Worker 探针服务；共享停止通知，统一有界排空。
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;
use tokio::sync::watch;
use tracing::{info, warn};

use super::workers::Workers;
use super::{ProcessRole, Result};
use crate::app_state::AppState;
use crate::core::routes;

/// 绑定成功后才启动后台循环；停止时先关闭领取，再排空 HTTP 和任务。
pub(super) async fn serve(state: AppState, role: ProcessRole, port: u16) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    let readiness = Arc::new(AtomicBool::new(true));
    let workers = if role.runs_workers() { Some(Workers::start(state.clone()).await?) } else { None };
    let app = application(&state, role, workers.as_ref(), readiness.clone());
    let (stop, mut receive) = watch::channel(false);
    let http = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async move {
            let _ = receive.changed().await;
        });
    let mut http = tokio::spawn(async move { http.await });
    info!(?role, port, "进程已启动");
    let (result, http_finished) = tokio::select! {
        _ = shutdown_signal() => (Ok(()), false),
        _ = worker_failure(workers.as_ref()) => (Err("常驻后台循环意外退出".into()), false),
        result = &mut http => (http_result(result), true),
    };
    readiness.store(false, Ordering::Release);
    if let Some(workers) = &workers {
        workers.request_stop();
    }
    stop.send_replace(true);
    let drained = tokio::time::timeout(Duration::from_secs(60), async {
        let background = async {
            if let Some(workers) = workers {
                workers.drain().await;
            }
        };
        let requests = async {
            if !http_finished {
                http_result((&mut http).await)?;
            }
            if state.jd_vop_authorization_tasks().shutdown().await.is_err() {
                warn!("京东授权任务退出状态待核实，请查询原授权记录");
            }
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        };
        let (_, requests) = tokio::join!(background, requests);
        requests
    })
    .await;
    match drained {
        Ok(drained) => {
            drained?;
            result
        },
        Err(_) => {
            warn!("进程排空超过 60 秒，未完成任务保留原状态恢复或核实");
            Err("进程排空超时".into())
        },
    }
}

/// API 只提供业务路由，Worker 只提供探针和已启用的执行器协议。
fn application(
    state: &AppState,
    role: ProcessRole,
    workers: Option<&Workers>,
    ready: Arc<AtomicBool>,
) -> Router {
    if role.serves_api() {
        routes::create(state.clone())
    } else {
        probes(ready).merge(workers.map(Workers::scheduler_routes).unwrap_or_default())
    }
}

/// 不启动后台循环的 API 只等待进程信号。
async fn worker_failure(workers: Option<&Workers>) {
    match workers {
        Some(workers) => workers.failed().await,
        None => std::future::pending::<()>().await,
    }
}

/// 传播 HTTP 服务或监督任务异常。
fn http_result(result: std::result::Result<std::io::Result<()>, tokio::task::JoinError>) -> Result<()> {
    result??;
    Ok(())
}

/// Worker 不装配业务路由；ready 在排空阶段关闭。
fn probes(ready: Arc<AtomicBool>) -> Router {
    Router::new().route("/health", get(|| async { StatusCode::OK })).route(
        "/ready",
        get(move || {
            let ready = ready.clone();
            async move {
                if ready.load(Ordering::Acquire) { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE }
            }
        }),
    )
}

/// 同时响应容器 SIGTERM 和本地 Ctrl-C。
async fn shutdown_signal() {
    #[cfg(unix)]
    wait_for_unix_shutdown().await;
    #[cfg(not(unix))]
    wait_for_ctrl_c().await;
    info!("Shutdown signal received");
}
/// 等待跨平台 Ctrl-C 信号；注册失败时记录错误并触发安全关闭。
async fn wait_for_ctrl_c() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        warn!(%error, "Failed to install Ctrl-C signal handler");
    }
}

/// 在 Unix 环境同时等待 SIGTERM 与 Ctrl-C，覆盖容器标准停止流程。
#[cfg(unix)]
async fn wait_for_unix_shutdown() {
    let terminate = async {
        let Ok(mut signal) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) else {
            warn!("Failed to install SIGTERM signal handler");
            std::future::pending::<()>().await;
            return;
        };
        signal.recv().await;
    };

    tokio::select! {
        _ = wait_for_ctrl_c() => {}
        _ = terminate => {}
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn worker_only_serves_probes_and_closes_readiness_on_shutdown() {
        let ready = Arc::new(AtomicBool::new(true));
        let app = probes(ready.clone());
        for (path, expected) in [
            ("/health", StatusCode::OK),
            ("/ready", StatusCode::OK),
            ("/admin/background-jobs", StatusCode::NOT_FOUND),
            ("/login", StatusCode::NOT_FOUND),
        ] {
            let response =
                app.clone().oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
            assert_eq!(response.status(), expected);
        }
        ready.store(false, Ordering::Release);
        let response =
            app.oneshot(Request::builder().uri("/ready").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
