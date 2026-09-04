//! 入站消息结果回写的规范化 Prepared DTO（INT-E18）。
//!
//! wire 契约保持 `processed` / `failed` 字符串取值；本模块经 `prepare`
//! 一次性完成边界校验与形状收紧，持有 tagged 决定。Service 只消费 tagged
//! 结果，不再拼装初始状态与 outcome 形状。时间由调用方注入，DTO 不读取时钟。

use entities::common::time::Instant;
use entities::integration_ops::ErrorClass;
use validator::Validate;

use super::inbox_message::{WriteBackInboxResultRequest, WriteBackOutcome};
use crate::errors::{Error, Result};

/// 结果回写的 tagged 规范化决定。
///
/// `processed` 仅携带处理完成时间；`failed` 携带错误分类、脱敏摘要与尝试时间。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreparedWriteBackOutcome {
    /// 已处理（含处理完成时间）。
    Processed {
        /// 处理完成时间（调用方注入）。
        processed_at: Instant,
    },
    /// 失败（转入错误任务）。
    Failed {
        /// 错误分类（必填）。
        error_class: ErrorClass,
        /// 脱敏的尝试结果摘要。
        attempt_summary: Option<String>,
        /// 尝试时间（请求携带或调用方注入）。
        attempt_at: Instant,
    },
}

impl PreparedWriteBackOutcome {
    /// 从回写请求生成规范化决定（时间由调用方注入）。
    ///
    /// # 参数
    /// * `request` - 已反序列化的回写请求
    /// * `now` - 调用方当前时间，用于缺省处理完成时间与尝试时间
    ///
    /// # 返回
    /// 返回 tagged 的已处理或失败决定。
    ///
    /// # 错误
    /// 请求边界校验失败、`processed` 携带错误分类或尝试摘要、`failed`
    /// 缺少错误分类时返回 `ValidationError`。
    ///
    /// # 约束
    /// 不读取全局时钟；`processed` 禁止 error 信息是对旧静默忽略的收紧。
    pub fn prepare(request: &WriteBackInboxResultRequest, now: Instant) -> Result<Self> {
        request.validate()?;
        match request.outcome {
            WriteBackOutcome::Processed => {
                if request.error_class.is_some() || request.attempt_summary.is_some() {
                    return Err(Error::ValidationError(
                        "已处理回写不得携带错误分类或尝试摘要".to_string(),
                    ));
                }
                Ok(Self::Processed {
                    processed_at: request.processed_at.map(Instant::from_unix_secs).unwrap_or(now),
                })
            }
            WriteBackOutcome::Failed => {
                let error_class = request
                    .error_class
                    .ok_or_else(|| Error::ValidationError("标记失败必须提供错误分类".to_string()))?;
                Ok(Self::Failed {
                    error_class,
                    attempt_summary: request.attempt_summary.clone(),
                    attempt_at: request.processed_at.map(Instant::from_unix_secs).unwrap_or(now),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::PreparedWriteBackOutcome;
    use super::WriteBackInboxResultRequest;
    use entities::common::time::Instant;
    use entities::integration_ops::ErrorClass;

    const NOW: i64 = 1_700_000_000;

    fn request(value: serde_json::Value) -> WriteBackInboxResultRequest {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn processed_holds_time_and_rejects_error_payload() {
        let prepared = PreparedWriteBackOutcome::prepare(
            &request(json!({"version": 1, "outcome": "processed", "processed_at": 1_700_000_100})),
            Instant::from_unix_secs(NOW),
        )
        .unwrap();
        assert!(matches!(
            prepared,
            PreparedWriteBackOutcome::Processed { processed_at } if processed_at.unix_secs() == 1_700_000_100
        ));

        let defaulted = PreparedWriteBackOutcome::prepare(
            &request(json!({"version": 1, "outcome": "processed"})),
            Instant::from_unix_secs(NOW),
        )
        .unwrap();
        assert!(matches!(
            defaulted,
            PreparedWriteBackOutcome::Processed { processed_at } if processed_at.unix_secs() == NOW
        ));

        assert!(PreparedWriteBackOutcome::prepare(
            &request(json!({"version": 1, "outcome": "processed", "error_class": "transient_failure"})),
            Instant::from_unix_secs(NOW),
        )
        .is_err());
        assert!(PreparedWriteBackOutcome::prepare(
            &request(json!({"version": 1, "outcome": "processed", "attempt_summary": "late"})),
            Instant::from_unix_secs(NOW),
        )
        .is_err());
    }

    #[test]
    fn failed_requires_error_class() {
        let prepared = PreparedWriteBackOutcome::prepare(
            &request(json!({"version": 2, "outcome": "failed", "error_class": "rate_limited"})),
            Instant::from_unix_secs(NOW),
        )
        .unwrap();
        assert!(matches!(
            prepared,
            PreparedWriteBackOutcome::Failed { error_class: ErrorClass::RateLimited, attempt_at, .. }
            if attempt_at.unix_secs() == NOW
        ));

        assert!(PreparedWriteBackOutcome::prepare(
            &request(json!({"version": 2, "outcome": "failed"})),
            Instant::from_unix_secs(NOW),
        )
        .is_err());
    }
}
