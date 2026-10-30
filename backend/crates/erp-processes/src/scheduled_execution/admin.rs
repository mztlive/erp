//! XXL-JOB 2.4.1 注册与回报协议；不传播凭据和远端自由文本。
use std::time::Duration;

use async_trait::async_trait;
use config::XxlJobConfig;
use erp_support::entity::scheduled_run::ScheduledRun;
use reqwest::redirect::Policy;
use reqwest::{Client, RequestBuilder, StatusCode};
use serde_json::{Value, json};

use super::ScheduleCallback;

/// 调度中心客户端，所有请求具有固定超时且禁止重定向。
pub struct XxlAdmin {
    client: Client,
    config: XxlJobConfig,
}
impl XxlAdmin {
    /// 构造受控 HTTP 客户端。
    /// # 参数
    /// config 为启动时校验并固定的配置。
    /// # 返回
    /// 注册和回报客户端。
    /// # 错误
    /// TLS 或 HTTP 客户端构造失败时返回错误。
    pub fn new(config: XxlJobConfig) -> Result<Self, reqwest::Error> {
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(5))
                .connect_timeout(Duration::from_secs(3))
                .redirect(Policy::none())
                .build()?,
            config,
        })
    }

    /// 注册或注销内部执行器地址。
    /// # 参数
    /// remove 为 true 时注销。
    /// # 返回
    /// HTTP 和协议 code 同时成功时为 true。
    /// # 错误
    /// 网络或协议失败返回 false，不记录远端响应或 token。
    pub async fn registry(&self, remove: bool) -> bool {
        self.post(
            if remove { "registryRemove" } else { "registry" },
            &json!({
                "registryGroup":"EXECUTOR", "registryKey":self.config.app_name,
                "registryValue":self.config.executor_url.trim_end_matches('/'),
            }),
        )
        .await
    }

    /// 构造固定协议请求，集中约束路径和凭据传递位置。
    fn request(&self, action: &str, body: &Value) -> RequestBuilder {
        let url = format!("{}/api/{action}", self.config.admin_url.trim_end_matches('/'));
        self.client.post(url).header("XXL-JOB-ACCESS-TOKEN", &self.config.access_token).json(body)
    }

    /// 发送固定管理协议，并限制响应体大小。
    async fn post(&self, action: &str, body: &Value) -> bool {
        let Ok(mut response) = self.request(action, body).send().await else {
            return false;
        };
        if response.status() != StatusCode::OK {
            return false;
        }
        let mut bytes = Vec::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) if bytes.len().saturating_add(chunk.len()) <= 65536 => {
                    bytes.extend_from_slice(&chunk)
                },
                Ok(None) => break,
                _ => return false,
            }
        }
        serde_json::from_slice::<Value>(&bytes)
            .is_ok_and(|value| value.get("code").and_then(Value::as_i64) == Some(200))
    }
}
#[async_trait]
impl ScheduleCallback for XxlAdmin {
    async fn send(&self, run: &ScheduledRun) -> bool {
        self.post("callback", &callback_body(run)).await
    }
}

/// logDateTim 为 XXL-JOB 2.4.1 的协议字段，不得改为 logDateTime。
fn callback_body(run: &ScheduledRun) -> Value {
    json!([{
        "logId":run.request.log_id, "logDateTim":run.request.log_time,
        "handleCode":run.handle_code, "handleMsg":run.handle_msg,
    }])
}

#[cfg(test)]
mod tests {
    use erp_support::entity::scheduled_run::{ScheduledHandler, ScheduledRequest};

    use super::*;

    #[test]
    fn callback_request_matches_admin_241_wire_contract_without_network() {
        let admin = XxlAdmin::new(XxlJobConfig {
            admin_url: "http://admin.invalid/xxl-job-admin/".into(),
            access_token: "test-secret-123456".into(),
            app_name: "erp-local".into(),
            executor_url: "http://worker.invalid:10002".into(),
        })
        .unwrap();
        let mut run = ScheduledRun::accepted(ScheduledRequest {
            app: "erp-test".into(),
            job_id: 1,
            log_id: 9,
            log_time: 1000,
            handler: ScheduledHandler::Tracking,
        });
        run.handle_code = 500;
        run.handle_msg = "中断".into();
        let request = admin.request("callback", &callback_body(&run)).build().unwrap();
        assert_eq!(request.method(), "POST");
        assert_eq!(request.url().as_str(), "http://admin.invalid/xxl-job-admin/api/callback");
        assert_eq!(request.headers()["XXL-JOB-ACCESS-TOKEN"], "test-secret-123456");
        let payload: Value = serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
        assert_eq!(payload, json!([{"logId":9,"logDateTim":1000,"handleCode":500,"handleMsg":"中断"}]));
    }
}
