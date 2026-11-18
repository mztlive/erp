//! Service 共享事务执行模板。
//!
//! 本模块只统一“业务写入 + 成功审计”的原子提交机械逻辑；领域校验、业务动作
//! 名称和资源身份仍由各领域 Service 决定。

use std::future::Future;
use std::pin::Pin;

use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::AuditFailure;
use super::attempt::{MongoAuditAttemptSink, finish_attempt};
use super::command::{AuditedCommand, MongoAuditEventSink, execute_audited, execute_prepared};
use crate::{AuditLog, BusinessEventContext, Error, attempt_context, prepare_business_log};

/// 为预先准备的成功审计和业务写入开一个 MongoDB 事务，提交成功后才返回业务结果。闭包仅做数据库写入，不得调用外部 HTTP 或文件 I/O。
/// 写入失败不提交业务或审计；事务结束后补记失败尝试。提交结果无法确认时沿用 `OutcomeUnknown`，调用方不得盲目重放。
pub async fn run_audited<R, E, F>(db: &Database, audit: AuditLog, write: F) -> std::result::Result<R, E>
where
    R: Send + 'static,
    E: AuditFailure + From<Error> + From<persistence_core::Error> + Send + 'static,
    F: for<'a> FnOnce(
            &'a Database,
            &'a mut dyn Executor,
        ) -> Pin<Box<dyn Future<Output = std::result::Result<R, E>> + Send + 'a>>
        + Send
        + 'static,
{
    let audit = prepare_business_log(&audit)?;
    let context = attempt_context(&audit)?;

    let audit_db = db.clone();
    let write_db = audit_db.clone();
    with_attempt(db, context, move |executor| {
        Box::pin(async move {
            let sink = MongoAuditEventSink::new(&audit_db);
            execute_prepared(&audit, &sink, executor, move |executor| {
                Box::pin(async move { write(&write_db, executor).await })
            })
            .await
        })
    })
    .await
}

/// 用已验证的事件上下文为类型化命令建立唯一事务，首次执行将正式写入和安全事件一起提交，回放返回领域恢复的原结果。已有事务应调用 `execute_audited`。业务、结果校验、审计和提交错误保留原分类，未知提交不得自动重放。
pub async fn run_audited_event<C>(
    db: &Database,
    context: BusinessEventContext,
    command: C,
) -> std::result::Result<C::Output, C::Error>
where
    C: AuditedCommand + 'static,
    C::Output: 'static,
    C::Error: AuditFailure + From<persistence_core::Error> + 'static,
{
    let audit_db = db.clone();
    with_attempt(db, context.clone(), move |executor| {
        Box::pin(async move {
            execute_audited(&context, &MongoAuditEventSink::new(&audit_db), executor, &command).await
        })
    })
    .await
}

async fn with_attempt<R, E, F>(
    db: &Database,
    context: BusinessEventContext,
    execute: F,
) -> std::result::Result<R, E>
where
    R: Send,
    E: AuditFailure + From<persistence_core::Error> + Send,
    F: for<'a> FnOnce(
            &'a mut dyn Executor,
        ) -> Pin<Box<dyn Future<Output = std::result::Result<R, E>> + Send + 'a>>
        + Send,
{
    let result = db.client().with_transaction(execute).await;
    finish_attempt(result, &context, &MongoAuditAttemptSink::new(db)).await
}
