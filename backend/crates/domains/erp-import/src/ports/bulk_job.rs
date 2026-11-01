//! 由支撑域持有的后台任务身份事实消费端口。

use async_trait::async_trait;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// 导入批次视图使用的最小后台任务身份事实。
#[async_trait]
pub trait BulkJobFactsPort: Send + Sync {
    /// 按导入批次请求身份查找后台任务 ID。
    ///
    /// # 参数
    /// * `request_id` - 用作任务请求身份的导入批次号。
    /// * `executor` - 调用方选择的数据访问执行器。
    ///
    /// # 返回
    /// 找到时返回后台任务 ID；没有对应任务时返回 `None`。
    ///
    /// # 错误
    /// 任务身份查询失败时返回对应错误。
    async fn background_job_id_by_request_id(
        &self,
        request_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>>;
}

/// 组合根尚未注入适配器时使用的失败关闭后台任务端口。
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedBulkJobFacts;

#[async_trait]
impl BulkJobFactsPort for FailClosedBulkJobFacts {
    /// 拒绝查询且不编造任务身份。
    ///
    /// # 参数
    /// * `_request_id` - 未使用；任意批次号都失败关闭。
    /// * `_executor` - 未使用。
    ///
    /// # 返回
    /// 不返回任务 ID。
    ///
    /// # 错误
    /// 总是返回 [`Error::Internal`]，文案为导入后台任务端口未接线。
    async fn background_job_id_by_request_id(
        &self,
        _request_id: &str,
        _executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        Err(Error::Internal("导入后台任务端口未接线".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use persistence_core::NoTransaction;

    use super::{BulkJobFactsPort, FailClosedBulkJobFacts};

    #[tokio::test]
    async fn fail_closed_bulk_job_port_does_not_invent_job_identity() {
        let mut executor = NoTransaction;
        let error = FailClosedBulkJobFacts
            .background_job_id_by_request_id("IMP-1", &mut executor)
            .await
            .expect_err("unwired port must fail closed");
        assert!(error.to_string().contains("未接线"));
    }
}
