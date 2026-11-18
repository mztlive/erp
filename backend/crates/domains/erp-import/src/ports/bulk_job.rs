//! 由支撑域持有的后台任务身份事实消费端口。

use async_trait::async_trait;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// 导入批次视图使用的最小后台任务身份事实。
#[async_trait]
pub trait BulkJobFactsPort: Send + Sync {
    /// 用批次号作为任务请求身份，在调用方原执行器上查关联后台任务。无关联返回 `None`，查询失败保留原错误，供批次详情区分未关联与不可查。
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
    // 未接线时不为任何批次编造任务身份，统一返回 [`Error::Internal`] 并指出导入后台任务端口不可用。
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
