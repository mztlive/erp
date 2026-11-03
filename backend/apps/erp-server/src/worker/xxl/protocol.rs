//! XXL-JOB 2.4.1 请求约束和 ReturnT 响应。
use erp_support::entity::scheduled_run::{ScheduledHandler, ScheduledRequest};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 所有协议错误都以固定摘要返回，禁止回显脚本或凭据。
#[derive(Serialize)]
pub(crate) struct Reply {
    pub code: i32,
    pub msg: Option<String>,
    pub content: Option<Value>,
}
impl Reply {
    /// 构造成功回执。
    /// # 参数
    /// content 为可选协议响应。
    /// # 返回
    /// code=200 的 ReturnT。
    /// # 错误
    /// 无。
    pub fn ok(content: Option<Value>) -> Self {
        Self { code: 200, msg: None, content }
    }
    /// 构造失败回执。
    /// # 参数
    /// message 为固定脱敏摘要。
    /// # 返回
    /// code=500 的 ReturnT。
    /// # 错误
    /// 无。
    pub fn fail(message: &str) -> Self {
        Self { code: 500, msg: Some(message.into()), content: None }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Trigger {
    job_id: i32,
    executor_handler: ScheduledHandler,
    #[serde(default)]
    executor_params: String,
    executor_block_strategy: String,
    executor_timeout: i32,
    log_id: i64,
    log_date_time: i64,
    glue_type: String,
    #[serde(default)]
    glue_source: String,
    broadcast_index: i32,
    broadcast_total: i32,
}
impl Trigger {
    /// 验证有限协议子集；不支持的运行模式必须拒绝。
    /// # 参数
    /// app 为执行器固定应用名。
    /// # 返回
    /// 可信的调度身份。
    /// # 错误
    /// 身份不匹配或运行方式不受支持时拒绝。
    pub fn request(self, app: &str) -> Result<ScheduledRequest, &'static str> {
        if self.glue_type != "BEAN"
            || !self.glue_source.is_empty()
            || !self.executor_params.trim().is_empty()
            || self.executor_block_strategy != "DISCARD_LATER"
            || self.executor_timeout != 0
            || self.broadcast_index != 0
            || self.broadcast_total != 1
        {
            return Err("仅支持 BEAN、空参数、DISCARD_LATER、超时0、非分片执行");
        }
        if self.job_id <= 0 {
            return Err("jobId 必须为正整数");
        }
        Ok(ScheduledRequest {
            app: app.to_string(),
            job_id: self.job_id,
            log_id: self.log_id,
            log_time: self.log_date_time,
            handler: self.executor_handler,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobParam {
    pub job_id: i32,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LogParam {
    pub log_id: i64,
    pub log_date_tim: i64,
    pub from_line_num: i32,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn trigger() -> Value {
        json!({"jobId":1,"executorHandler":"supplier_tracking_scan","executorParams":"",
            "executorBlockStrategy":"DISCARD_LATER","executorTimeout":0,"logId":7,
            "logDateTime":1000,"glueType":"BEAN","glueSource":"","glueUpdatetime":0,
            "broadcastIndex":0,"broadcastTotal":1})
    }
    #[test]
    fn protocol_preserves_identity_and_rejects_unsupported_execution() {
        let request = serde_json::from_value::<Trigger>(trigger()).unwrap().request("erp-test").unwrap();
        assert_eq!(request.key(), "erp-test:7");
        assert_eq!(request.log_time, 1000);
        for (id, handler) in [(123, "supplier_tracking_scan"), (456, "supplier_catalog_schedule")] {
            let mut input = trigger();
            input["jobId"] = json!(id);
            input["executorHandler"] = json!(handler);
            let accepted = serde_json::from_value::<Trigger>(input).unwrap().request("erp-test").unwrap();
            assert_eq!(accepted.job_id, id);
            assert_eq!(serde_json::to_value(accepted.handler).unwrap(), json!(handler));
        }

        for (field, value) in [
            ("executorBlockStrategy", json!("COVER_EARLY")),
            ("executorBlockStrategy", json!("SERIAL_EXECUTION")),
            ("executorTimeout", json!(30)),
            ("executorParams", json!("{}")),
            ("glueType", json!("GLUE_SHELL")),
            ("glueSource", json!("script")),
            ("broadcastTotal", json!(2)),
            ("broadcastIndex", json!(1)),
            ("jobId", json!(0)),
        ] {
            let mut input = trigger();
            input[field] = value;
            assert!(
                serde_json::from_value::<Trigger>(input).unwrap().request("erp-test").is_err(),
                "{field}"
            );
        }
        let mut input = trigger();
        input["executorHandler"] = json!("arbitrary");
        assert!(serde_json::from_value::<Trigger>(input).is_err());
    }
}
