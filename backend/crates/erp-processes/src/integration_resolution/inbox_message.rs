//! 入站消息登记与结果回写（processed / failed+错误任务）的跨域根。
//!
//! 消息层/业务事实层幂等由唯一索引保证，服务层不做「先查后插」重复性判断；
//! 所有业务写入与审计日志在同一 MongoDB 事务原子提交（模板见 `super::transaction`）。
use super::creation_writes::{persist_created, CreatedFact};
use super::producer::error_work_item;
use super::IntegrationResolutionProcess;
use crate::{Error, Result};
use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::common::time::Instant;
use erp_integration::dto::*;
use erp_integration::entity::integration_ops::InboxMessageId;
use erp_integration::repository::IntegrationOpsExt;
use erp_integration::service::inbox_message::*;
use erp_integration::service::validation::ensure_version;
use erp_support::SourceRegistryExt;
use persistence_core::NoTransaction;
use validator::Validate;

impl IntegrationResolutionProcess {
    /// 登记入站消息（消息层与业务事实层幂等由唯一索引保证）。
    ///
    /// 消息状态由服务端置为 `received`；来源系统存在性经 D01 `SourceRegistryExt`
    /// 跨域只读校验。重复投递（同来源事件或同业务事实键）由唯一索引透出 409。
    ///
    /// # 参数
    /// * `req` - 登记请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建消息的详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 来源系统不存在
    /// * `ConflictError` - 消息身份或业务事实键重复（唯一索引透出）
    /// * `ValidationError` - 请求体校验失败
    pub async fn register_inbox_message(
        &self,
        req: RegisterInboxMessageRequest,
        actor: &AuditActor,
    ) -> Result<InboxMessageView> {
        req.validate()?;
        self.db
            .source_systems()
            .find_by_id(req.source_system_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源系统不存在".to_string()))?;

        let received_at = Instant::from_unix_secs(req.received_at.unwrap_or_else(now_secs));
        let message = prepare_registered_inbox_message(req, received_at)?;
        let audit =
            actor
                .clone()
                .resource_log("inbox_message.register", "inbox_message", message.base.id.clone())?;
        let stored = message.clone();
        self.run_audited(move |db, session| {
            Box::pin(async move {
                persist_inbox_message(db, &stored, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok(())
            })
        })
        .await?;

        Ok(message.into())
    }

    /// 回写入站消息处理结果。
    ///
    /// `processed`：状态置为已处理并记录处理完成时间；`failed`：状态置为失败，
    /// 并在同一事务登记错误任务（仓库 `create_error_task_with_message_failure`
    /// 必须收到事务执行器）。消息处理状态与任务登记原子可见。
    ///
    /// # 参数
    /// * `id` - 消息 ID
    /// * `req` - 回写请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回回写后的消息详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 消息不存在
    /// * `ConflictError` - 期望版本不一致，或消息已有进行中的同分类错误任务
    /// * `ValidationError` - 请求体校验失败或失败回写缺少错误分类
    pub async fn write_back_inbox_result(
        &self,
        id: &str,
        req: WriteBackInboxResultRequest,
        actor: &AuditActor,
    ) -> Result<InboxMessageView> {
        let outcome = PreparedWriteBackOutcome::prepare(&req, Instant::now())?;
        let mut message = self
            .db
            .inbox_messages()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("消息不存在".to_string()))?;
        ensure_version(message.base.version, req.version)?;

        match outcome {
            PreparedWriteBackOutcome::Processed { processed_at } => {
                apply_processed_outcome(&mut message, processed_at)?;
                let audit = actor.clone().resource_log(
                    "inbox_message.processed",
                    "inbox_message",
                    message.base.id.clone(),
                )?;
                let stored = self
                    .run_audited(move |db, session| {
                        let mut stored = message;
                        Box::pin(async move {
                            update_inbox_message(db, &mut stored, session).await?;
                            db.audit_logs().create(&audit, session).await?;
                            Ok(stored)
                        })
                    })
                    .await?;
                Ok(stored.into())
            }
            PreparedWriteBackOutcome::Failed {
                error_class,
                attempt_summary,
                attempt_at,
            } => {
                apply_failed_outcome(&mut message)?;
                let task = prepare_failed_message_task(
                    InboxMessageId::new(message.base.id.clone()),
                    error_class,
                    actor.id(),
                    attempt_summary,
                    attempt_at,
                )?;
                let work_item = error_work_item(&task, actor.id())?;
                let audit = actor.clone().resource_log(
                    "inbox_message.failed",
                    "inbox_message",
                    message.base.id.clone(),
                )?;
                let stored = self
                    .run_audited(move |db, session| {
                        let mut stored = message;
                        Box::pin(async move {
                            persist_created(
                                db,
                                CreatedFact::FailedMessage {
                                    task: &task,
                                    message: &mut stored,
                                },
                                &work_item,
                                &audit,
                                session,
                            )
                            .await?;
                            Ok(stored)
                        })
                    })
                    .await?;
                Ok(stored.into())
            }
        }
    }
}
/// 返回当前时间的秒级时间戳。
fn now_secs() -> i64 {
    Instant::now().unix_secs()
}
#[cfg(test)]
mod tests {
    /// 生产代码（测试模块之前部分），供分层守卫断言，避免字面量自匹配。
    ///
    /// # 返回
    /// 返回去掉测试模块后的生产代码全文。
    fn production_source() -> String {
        format!(
            "{}\n{}",
            include_str!("inbox_message.rs")
                .split("#[cfg(test)]")
                .next()
                .unwrap(),
            include_str!("../../../erp-integration/src/service/inbox_message.rs")
        )
    }

    /// 分层守卫（INT-E18）：登记态与回写形状归领域与 Prepared DTO，服务只做编排。
    ///
    /// 锁定旧拼装源（`build_inbox_message`、内联 `Received` 状态与 outcome
    /// 匹配）已删除；登记走实体 `received` 工厂，回写走 tagged 决定。
    #[test]
    fn inbox_shapes_are_owned_by_domain_and_prepared() {
        let source = production_source();
        assert!(!source.contains("fn build_inbox_message"));
        assert!(!source.contains("status: InboxMessageStatus::Received"));
        assert!(!source.contains("match req.outcome"));
        assert!(source.contains("InboxMessage::received("));
        assert!(source.contains("PreparedWriteBackOutcome::prepare(&req"));
    }
}
