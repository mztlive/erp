//! 共享事务模板：业务写入与审计日志写入跨集合原子提交（D01 样板写法）。
//!
//! 仅限本域内部调用（`pub(super)`），不对外暴露。

use std::future::Future;
use std::pin::Pin;

use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::IntegrationResolutionProcess;
use crate::Result;

impl IntegrationResolutionProcess {
    // 集成处置跨集合写命令业务事实和成功审计时，共用此事务；闭包不得进行外部 HTTP 或文件 I/O。
    // 返回闭包结果或原事务错误，提交结果未知保留 `OutcomeUnknown`。
    pub(super) async fn run_audited<R, F>(&self, f: F) -> Result<R>
    where
        R: Send,
        F: for<'a> FnOnce(
                &'a Database,
                &'a mut dyn Executor,
            ) -> Pin<Box<dyn Future<Output = Result<R>> + Send + 'a>>
            + Send
            + 'static,
    {
        let db = self.db.clone();
        let client = db.client().clone();
        client.with_transaction(move |executor| Box::pin(async move { f(&db, executor).await })).await
    }
}
