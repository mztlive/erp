//! 独立执行器协议；鉴权先于请求体解析及持久化。
use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use chrono::Utc;
use erp_support::entity::scheduled_run::ScheduledState;
use serde_json::json;
use subtle::ConstantTimeEq;

use super::XxlExecutor;
use super::protocol::{JobParam, LogParam, Reply, Trigger};

/// 仅 Worker 装配这些协议路由。
/// # 参数
/// executor 为固定启动配置的共享执行器。
/// # 返回
/// 带 token 校验和请求体限制的协议路由。
/// # 错误
/// 无。
pub(super) fn router(executor: Arc<XxlExecutor>) -> Router {
    Router::new()
        .route("/beat", post(beat))
        .route("/idleBeat", post(idle))
        .route("/run", post(run))
        .route("/kill", post(kill))
        .route("/log", post(log))
        .layer(DefaultBodyLimit::max(16384))
        .layer(middleware::from_fn_with_state(executor.clone(), authenticate))
        .with_state(executor)
}

/// 不回显请求头；停止后所有协议拒绝新请求。
async fn authenticate(State(executor): State<Arc<XxlExecutor>>, request: Request, next: Next) -> Response {
    let token = request.headers().get("XXL-JOB-ACCESS-TOKEN").map(|v| v.as_bytes()).unwrap_or_default();
    if !bool::from(token.ct_eq(executor.config.access_token.as_bytes())) {
        return Json(Reply::fail("执行器认证失败")).into_response();
    }
    if executor.stopping.load(Ordering::Acquire) {
        return Json(Reply::fail("执行器正在退出")).into_response();
    }
    next.run(request).await
}

/// 认证后的协议存活检查。
async fn beat() -> Json<Reply> {
    Json(Reply::ok(None))
}

/// 接收成功仅表示回执已持久化。
async fn run(
    State(executor): State<Arc<XxlExecutor>>,
    body: Result<Json<Trigger>, JsonRejection>,
) -> Json<Reply> {
    let Ok(Json(trigger)) = body else {
        return Json(Reply::fail("调度请求格式错误或超出大小限制"));
    };
    let request = match trigger.request(&executor.config.app_name) {
        Ok(request) => request,
        Err(message) => return Json(Reply::fail(message)),
    };
    match executor.process.accept(request, Utc::now().timestamp_millis()).await {
        Ok(()) => Json(Reply::ok(None)),
        Err(error) => Json(Reply::fail(&error.to_string())),
    }
}

/// 查询固定任务槽位，数据库不可用不能伪报空闲。
async fn idle(
    State(executor): State<Arc<XxlExecutor>>,
    body: Result<Json<JobParam>, JsonRejection>,
) -> Json<Reply> {
    let Ok(Json(job)) = body else { return Json(Reply::fail("任务参数错误")) };
    if job.job_id <= 0 {
        return Json(Reply::fail("jobId 必须为正整数"));
    }
    match executor.process.busy(job.job_id).await {
        Ok(false) => Json(Reply::ok(None)),
        Ok(true) => Json(Reply::fail("任务正在执行或等待执行")),
        Err(_) => Json(Reply::fail("任务状态暂不可确认")),
    }
}

/// 不强制中断业务写入，不伪报取消成功。
async fn kill() -> Json<Reply> {
    Json(Reply::fail("不支持强制终止，请等待当前轮次完成并查询 ERP 原业务记录"))
}

/// 仅返回两行脱敏状态摘要，支持调度中心增量读取。
async fn log(
    State(executor): State<Arc<XxlExecutor>>,
    body: Result<Json<LogParam>, JsonRejection>,
) -> Json<Reply> {
    let Ok(Json(query)) = body else { return Json(Reply::fail("日志参数错误")) };
    if query.from_line_num < 1 || query.log_id <= 0 {
        return Json(Reply::fail("日志参数错误"));
    }
    let run = match executor.process.receipt(query.log_id).await {
        Ok(Some(run)) if run.request.log_time == query.log_date_tim => run,
        _ => return Json(Reply::fail("原调度回执不存在或暂不可查询")),
    };
    let done = run.state == ScheduledState::Completed;
    let to = if done { 2 } else { 1 };
    let mut lines = Vec::new();
    if query.from_line_num <= 1 {
        lines.push("调度请求已持久化，业务结果以 ERP 原记录为准".to_string());
    }
    if done && query.from_line_num <= 2 {
        lines.push(run.handle_msg);
    }
    Json(Reply::ok(Some(json!({"fromLineNum":query.from_line_num,"toLineNum":to,
        "logContent":lines.join("\n"),"isEnd":done}))))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::AtomicBool;

    use async_trait::async_trait;
    use axum::body::{Body, to_bytes};
    use axum::http::Request as HttpRequest;
    use config::XxlJobConfig;
    use erp_processes::scheduled_execution::{
        ScheduleCallback, ScheduledExecutionProcess, ScheduledWork, XxlAdmin,
    };
    use erp_support::entity::scheduled_run::{ScheduledHandler, ScheduledRun};
    use erp_support::ports::scheduled_runs::ScheduledRunStore;
    use persistence_core::{Error, Result as StoreResult};
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;

    #[derive(Default)]
    struct Store(Mutex<Option<ScheduledRun>>);
    #[async_trait]
    impl ScheduledRunStore for Store {
        async fn find(&self, key: &str) -> StoreResult<Option<ScheduledRun>> {
            Ok(self.0.lock().unwrap().clone().filter(|r| r.base.id == key))
        }
        async fn insert(&self, run: &ScheduledRun) -> StoreResult<()> {
            let mut slot = self.0.lock().unwrap();
            if slot.is_some() {
                return Err(Error::OptimisticLockingError);
            }
            *slot = Some(run.clone());
            Ok(())
        }
        async fn job_busy(&self, app: &str, job_id: i32) -> StoreResult<bool> {
            Ok(self.0.lock().unwrap().as_ref().is_some_and(|r| {
                r.request.app == app && r.request.job_id == job_id && r.state != ScheduledState::Completed
            }))
        }
        async fn claim(&self, _: &str, _: &str) -> StoreResult<Option<ScheduledRun>> {
            panic!("HTTP must not run business")
        }
        async fn finish(&self, _: &ScheduledRun, _: i32, _: &str) -> StoreResult<()> {
            panic!("HTTP must not finish business")
        }
        async fn recover(&self, _: &str, _: &str) -> StoreResult<()> {
            Ok(())
        }
        async fn callbacks(&self, _: &str, _: i64) -> StoreResult<Vec<ScheduledRun>> {
            Ok(vec![])
        }
        async fn callback_result(&self, _: &ScheduledRun, _: bool, _: i64) -> StoreResult<()> {
            Ok(())
        }
    }
    struct Work;
    #[async_trait]
    impl ScheduledWork for Work {
        async fn run(&self, _: ScheduledHandler) -> Result<usize, &'static str> {
            panic!("HTTP must not execute business")
        }
    }
    struct Callback;
    #[async_trait]
    impl ScheduleCallback for Callback {
        async fn send(&self, _: &ScheduledRun) -> bool {
            panic!("HTTP must not send callback")
        }
    }
    fn executor() -> Arc<XxlExecutor> {
        let config = XxlJobConfig {
            access_token: "test-token-123456".into(),
            app_name: "erp-test".into(),
            admin_url: "http://unused.invalid/xxl-job-admin".into(),
            executor_url: "http://unused.invalid:10001".into(),
        };
        let process = Arc::new(ScheduledExecutionProcess::new(
            config.app_name.clone(),
            "owner".into(),
            Arc::new(Store::default()),
            Arc::new(Work),
            Arc::new(Callback),
        ));
        Arc::new(XxlExecutor {
            admin: Arc::new(XxlAdmin::new(config.clone()).unwrap()),
            config,
            process,
            stopping: AtomicBool::new(false),
        })
    }
    async fn request(app: Router, path: &str, token: &str, body: String) -> Value {
        let response = app
            .oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri(path)
                    .header("Content-Type", "application/json")
                    .header("XXL-JOB-ACCESS-TOKEN", token)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap()
    }
    #[tokio::test]
    async fn routes_authenticate_before_parsing_and_accept_without_executing() {
        let executor = executor();
        let app = router(executor.clone());
        let token = "test-token-123456";
        assert_eq!(request(app.clone(), "/run", "wrong", "not-json".into()).await["msg"], "执行器认证失败");
        assert_eq!(request(app.clone(), "/run", token, "not-json".into()).await["code"], 500);
        assert_eq!(request(app.clone(), "/run", token, "x".repeat(17000)).await["code"], 500);
        assert_eq!(
            request(app.clone(), "/idleBeat", token, json!({"jobId":123}).to_string()).await["code"],
            200
        );
        assert_eq!(
            request(app.clone(), "/idleBeat", token, json!({"jobId":0}).to_string()).await["code"],
            500
        );
        let now = Utc::now().timestamp_millis();
        let body = json!({"jobId":123,"executorHandler":"supplier_tracking_scan","executorParams":"",
            "executorBlockStrategy":"DISCARD_LATER","executorTimeout":0,"logId":10,"logDateTime":now,
            "glueType":"BEAN","broadcastIndex":0,"broadcastTotal":1})
        .to_string();
        assert_eq!(request(app.clone(), "/run", token, body.clone()).await["code"], 200);
        assert_eq!(request(app.clone(), "/run", token, body).await["code"], 200);
        assert_eq!(
            request(app.clone(), "/idleBeat", token, json!({"jobId":123}).to_string()).await["code"],
            500
        );
        assert_eq!(
            request(app.clone(), "/idleBeat", token, json!({"jobId":456}).to_string()).await["code"],
            200
        );
        let log = request(
            app.clone(),
            "/log",
            token,
            json!({"logId":10,"logDateTim":now,"fromLineNum":1}).to_string(),
        )
        .await;
        assert_eq!(log["code"], 200);
        assert_eq!(log["content"]["isEnd"], false);
        assert_eq!(request(app.clone(), "/kill", token, json!({"jobId":123}).to_string()).await["code"], 500);
        executor.request_stop();
        assert_eq!(request(app, "/beat", token, String::new()).await["code"], 500);
    }
    #[tokio::test]
    async fn stopped_executor_never_claims_another_job() {
        let executor = executor();
        let (_stop, receive) = tokio::sync::watch::channel(true);
        tokio::time::timeout(std::time::Duration::from_secs(1), executor.execute(receive)).await.unwrap();
        // Store::claim panics if shutdown still reaches the persistence port.
    }
}
