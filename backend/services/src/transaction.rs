//! Service 共享事务执行模板。
//!
//! 本模块只统一“业务写入 + 成功审计”的原子提交机械逻辑；领域校验、业务动作
//! 名称和资源身份仍由各领域 Service 决定。

use std::{future::Future, pin::Pin};

use database::{AccessControlExt, Transactional};
use entities::AuditLog;
use mongodb::{ClientSession, Database};

use crate::errors::Result;

/// 在单个 MongoDB 事务中执行业务写入并追加成功审计。
///
/// # 参数
/// * `db` - 数据库实例
/// * `audit` - 事务开始前已完成校验的成功审计记录
/// * `write` - 只执行数据库业务写入的闭包；禁止外部 HTTP 或文件 I/O
///
/// # 返回
/// 返回业务写入闭包的结果。业务写入和审计均成功后才允许事务提交。
///
/// # 错误
/// 业务写入、审计写入或事务提交失败时返回错误并回滚全部写入；提交结果无法
/// 确认时沿用统一 `OutcomeUnknown` 映射，调用方不得盲目重放。
pub(crate) async fn run_audited<R, F>(db: &Database, audit: AuditLog, write: F) -> Result<R>
where
    R: Send + 'static,
    F: for<'a> FnOnce(
            &'a Database,
            &'a mut ClientSession,
        ) -> Pin<Box<dyn Future<Output = Result<R>> + Send + 'a>>
        + Send
        + 'static,
{
    let db = db.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                let result = write(&db, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok(result)
            })
        })
        .await
}
