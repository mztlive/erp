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

use super::{ProcessRole, Result};
use crate::http::routes;
use crate::http::state::HttpState;
use crate::wiring::Application;
use crate::worker::Workers;

// 绑定监听端口后才启动相应角色的路由和后台任务。收到退出信号、HTTP 退出或常驻循环失败时，先停止新请求和任务领取，再统一排空。
pub(crate) async fn serve(state: Application, role: ProcessRole, port: u16) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    let readiness = Arc::new(AtomicBool::new(true));
    let workers = if role.runs_workers() { Some(Workers::start(state.clone()).await?) } else { None };
    let http_state = role.serves_api().then(|| HttpState::from_application(state));
    if let Some(state) = &http_state {
        state.notification_feed.start(state.application().clone(), state.tasks())?;
    }
    let app = application(http_state.as_ref(), workers.as_ref(), readiness.clone());
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
    if let Some(state) = &http_state {
        state.notification_feed.stop();
    }
    stop.send_replace(true);
    drain(http, http_finished, http_state, workers).await?;
    result
}

// 停机时并行排空 HTTP 和 Worker；存量 HTTP 结束后再等请求衍生任务和京东授权保存。总预算为 60 秒，超时返回进程错误，后续按原任务状态恢复或核实。
async fn drain(
    mut http: tokio::task::JoinHandle<std::io::Result<()>>,
    http_finished: bool,
    state: Option<HttpState>,
    workers: Option<Workers>,
) -> Result<()> {
    let background = async {
        if let Some(workers) = workers {
            workers.drain().await;
        }
    };
    let requests = async {
        let result = if http_finished { Ok(()) } else { http_result((&mut http).await) };
        if let Some(state) = state {
            tokio::join!(state.tasks().drain(), async {
                if state.jd_vop_authorization_tasks().shutdown().await.is_err() {
                    warn!("京东授权任务退出状态待核实，请查询原授权记录");
                }
            });
        }
        result
    };
    match tokio::time::timeout(Duration::from_secs(60), async {
        let (_, result) = tokio::join!(background, requests);
        result
    })
    .await
    {
        Ok(result) => result,
        Err(_) => {
            warn!("进程排空超过 60 秒，未完成任务保留原状态恢复或核实");
            Err("进程排空超时".into())
        },
    }
}

// 按进程职责选择路由：Web API 挂业务接口，Worker 只挂探针和已装配的 XXL 协议。
fn application(state: Option<&HttpState>, workers: Option<&Workers>, ready: Arc<AtomicBool>) -> Router {
    if let Some(state) = state {
        routes::create(state.clone())
    } else {
        probes(ready).merge(workers.map(Workers::scheduler_routes).unwrap_or_default())
    }
}

// 启动 Worker 时等待监督器发现常驻循环退出；没有 Worker 的 API 保持等待，不触发此退出分支。
async fn worker_failure(workers: Option<&Workers>) {
    match workers {
        Some(workers) => workers.failed().await,
        None => std::future::pending::<()>().await,
    }
}

// 把 HTTP 监听任务的 I/O 失败与 join 异常统一传给进程生命周期，避免把异常退出当作正常关闭。
fn http_result(result: std::result::Result<std::io::Result<()>, tokio::task::JoinError>) -> Result<()> {
    result??;
    Ok(())
}

// 装配 Worker 存活和就绪探针；进入排空时 `/ready` 返回 503，探针不代表 XXL 注册成功或业务无积压。
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

// 等待运行环境的停止信号，供服务进入统一的有界停机流程；Unix 环境兼容 SIGTERM 和 Ctrl-C。
async fn shutdown_signal() {
    #[cfg(unix)]
    wait_for_unix_shutdown().await;
    #[cfg(not(unix))]
    wait_for_ctrl_c().await;
    info!("Shutdown signal received");
}
// 等待跨平台 Ctrl-C；若处理器注册失败，记日志并结束等待，让进程进入安全关闭流程。
async fn wait_for_ctrl_c() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        warn!(%error, "Failed to install Ctrl-C signal handler");
    }
}

// 在 Unix 上同时等待 SIGTERM 与 Ctrl-C，以兼容容器停止和本地退出；SIGTERM 注册失败时继续等 Ctrl-C。
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
