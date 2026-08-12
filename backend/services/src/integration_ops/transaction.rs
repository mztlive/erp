//! 共享事务模板：业务写入与审计日志写入跨集合原子提交（D01 样板写法）。
//!
//! 仅限本域内部调用（`pub(super)`），不对外暴露。

use std::{future::Future, pin::Pin};

use database::Transactional;

use super::IntegrationOpsService;
use crate::errors::Result;

impl IntegrationOpsService {
    /// 在事务中执行业务写入与审计日志写入（跨集合原子提交，D01 样板写法）。
    ///
    /// # 参数
    /// * `f` - 事务闭包（业务写入 + 审计写入；禁止外部 HTTP/文件 IO）
    ///
    /// # 返回
    /// 返回事务结果（闭包返回值）。
    ///
    /// # 错误
    /// 事务内错误透出；提交结果未知映射为 `OutcomeUnknown`。
    pub(super) async fn run_audited<R, F>(&self, f: F) -> Result<R>
    where
        R: Send,
        F: for<'a> FnOnce(
                &'a mongodb::Database,
                &'a mut mongodb::ClientSession,
            ) -> Pin<Box<dyn Future<Output = Result<R>> + Send + 'a>>
            + Send
            + 'static,
    {
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| Box::pin(async move { f(&db, session).await }))
            .await
    }
}
