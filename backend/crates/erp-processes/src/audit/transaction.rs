//! Service 共享事务执行模板。
//!
//! 本模块只统一“业务写入 + 成功审计”的原子提交机械逻辑；领域校验、业务动作
//! 名称和资源身份仍由各领域 Service 决定。

use std::future::Future;
use std::pin::Pin;

use erp_audit::{AuditLog, BusinessEventContext, attempt_context, prepare_business_log};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::attempt::{MongoAuditAttemptSink, finish_attempt};
use super::execution::{AuditedCommand, MongoAuditEventSink, execute_audited, execute_prepared};
use crate::Result;

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
pub async fn run_audited<R, F>(db: &Database, audit: AuditLog, write: F) -> Result<R>
where
    R: Send + 'static,
    F: for<'a> FnOnce(
            &'a Database,
            &'a mut dyn Executor,
        ) -> Pin<Box<dyn Future<Output = Result<R>> + Send + 'a>>
        + Send
        + 'static,
{
    let audit = prepare_business_log(&audit)?;
    let context = attempt_context(&audit)?;
    let attempts_db = db.clone();
    let db = db.clone();
    let client = db.client().clone();
    let result = client
        .with_transaction(move |executor| {
            Box::pin(async move {
                let write_db = db.clone();
                execute_prepared(&audit, &MongoAuditEventSink::new(&db), executor, move |executor| {
                    Box::pin(async move { write(&write_db, executor).await })
                })
                .await
            })
        })
        .await;
    finish_attempt(result, &context, &MongoAuditAttemptSink::new(&attempts_db)).await
}

/// 为类型化命令建立唯一事务并记录执行后的安全事实。
///
/// 已有调用方事务时使用 `execute_audited`，不得再次调用本入口。
///
/// # 参数
/// * `db` - 当前业务用例数据库。
/// * `context` - 事务开始前已验证的身份、动作及关联快照。
/// * `command` - 只执行当前数据库业务规则和写入的命令。
///
/// # 返回
/// 返回首次执行或领域恢复的原命令结果。
///
/// # 错误
/// 业务、结果校验、审计或提交失败时沿用原错误，未知提交不得自动重放。
pub async fn run_audited_event<C>(
    db: &Database,
    context: BusinessEventContext,
    command: C,
) -> Result<C::Output>
where
    C: AuditedCommand + 'static,
    C::Output: 'static,
{
    let attempt_context = context.clone();
    let attempts_db = db.clone();
    let db = db.clone();
    let client = db.client().clone();
    let result = client
        .with_transaction(move |executor| {
            Box::pin(async move {
                execute_audited(&context, &MongoAuditEventSink::new(&db), executor, &command).await
            })
        })
        .await;
    finish_attempt(result, &attempt_context, &MongoAuditAttemptSink::new(&attempts_db)).await
}
