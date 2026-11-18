//! 集成本域查询、实体准备与调用方事务内写入。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::AccessTarget;
use erp_permissions::integration_ops;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::IntegrationOpsService;
use crate::dto::*;
use crate::entity::integration_ops::*;
use crate::repository::IntegrationOpsExt;
use crate::repository::integration_ops::InboxMessageFilter;
use crate::repository::prelude::*;
use crate::{Error, Result};

impl IntegrationOpsService {
    /// 分页查询入站消息列表。
    ///
    /// 排序字段必须经过白名单校验。
    ///
    /// 失败情形如下：
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    ///
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    pub async fn inbox_message_list(
        &self,
        params: &InboxMessageListParams,
        actor: &AuditActor,
    ) -> Result<PageView<InboxMessageListView>> {
        let service = self.clone();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.inbox_page(&params, &actor, executor).await })
            })
            .await
    }

    // 在原读取快照内先检查入站消息列表资格，再按请求分页；仓储不另建授权阶段。
    async fn inbox_page(
        &self,
        params: &InboxMessageListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PageView<InboxMessageListView>> {
        params.validate()?;
        self.data_scope
            .service
            .require(actor, &integration_ops::INBOX_MESSAGE_LIST, AccessTarget::Operation, executor)
            .await?;

        let query = params.normalized()?;
        let filter = InboxMessageFilter {
            source_system_id: query.source_system_id,
            message_type: query.message_type,
            status: query.status,
            source_event_id: query.source_event_id,
            received_at_from: query.received_at_from,
            received_at_to: query.received_at_to,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self.db.inbox_messages().search_inbox_messages(&filter, executor).await?;

        let items = page
            .items
            .into_iter()
            .map(|row| InboxMessageListView {
                id: row.id,
                source_system_id: row.source_system_id.to_string(),
                source_event_id: row.source_event_id,
                message_type: row.message_type,
                business_fact_key: row.business_fact_key,
                payload_schema_version: row.payload_schema_version,
                status: row.status,
                source_sent_at: row.source_sent_at.map(|at| at.unix_secs()),
                received_at: row.received_at.unix_secs(),
                processed_at: row.processed_at.map(|at| at.unix_secs()),
                version: row.version,
                created_at: row.created_at,
            })
            .collect();
        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }

    /// 查询入站消息详情（含规范化内容引用）。
    ///
    /// 失败情形如下：
    /// * `NotFound` - 消息不存在
    /// * 仓储读取失败时返回对应错误。
    pub async fn inbox_message_detail(&self, id: &str, actor: &AuditActor) -> Result<InboxMessageView> {
        let service = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.inbox_detail(&id, &actor, executor).await })
            })
            .await
    }

    // 在原读取快照内先检查入站消息详情资格，再读取该消息及规范化内容引用。
    async fn inbox_detail(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<InboxMessageView> {
        self.data_scope
            .service
            .require(actor, &integration_ops::INBOX_MESSAGE_DETAIL, AccessTarget::Operation, executor)
            .await?;

        let message = self
            .db
            .inbox_messages()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("消息不存在".to_string()))?;
        Ok(message.into())
    }
}
/// 构造已通过请求与来源存在性检查的入站消息。
///
/// 返回状态为已接收的入站消息。`InboxMessage::received` 的字段不变量失败时返回 `Logic`。
///
/// `received_at` 为调用方注入的接收时间。
pub fn prepare_registered_inbox_message(
    req: RegisterInboxMessageRequest,
    received_at: Instant,
) -> Result<InboxMessage> {
    let message = InboxMessage::received(
        InboxMessageId::new(next_id()),
        InboxMessageReceivedData {
            source_system_id: req.source_system_id,
            source_event_id: req.source_event_id,
            message_type: req.message_type,
            business_fact_key: req.business_fact_key,
            payload_schema_version: req.payload_schema_version,
            payload_reference: req.payload_reference,
            source_sent_at: req.source_sent_at.map(Instant::from_unix_secs),
            received_at,
        },
    )?;
    Ok(message)
}
/// 应用已准备的 processed 结果，不写入数据库。
///
/// 成功时消息状态为已处理并带有处理完成时间。`InboxMessage::update` 的领域校验失败时返回 `Logic`。
pub fn apply_processed_outcome(message: &mut InboxMessage, processed_at: Instant) -> Result<()> {
    message.update(InboxMessageUpdate {
        status: Some(InboxMessageStatus::Processed),
        processed_at: Some(processed_at),
    })?;
    Ok(())
}
/// 应用 failed 结果，不生成任务或写入数据库。
///
/// 成功时消息状态为失败且不写处理完成时间。`InboxMessage::update` 的领域校验失败时返回 `Logic`。
pub fn apply_failed_outcome(message: &mut InboxMessage) -> Result<()> {
    message.update(InboxMessageUpdate { status: Some(InboxMessageStatus::Failed), processed_at: None })?;
    Ok(())
}
/// 按原责任政策构造失败任务；仅摘要存在时记录尝试。
///
/// 返回失败消息对应的错误任务。任务构造或尝试记录不满足实体不变量时返回 `Logic`。
///
/// - `message_id`：失败消息 ID。
/// - `actor_id`：当前处理人。
/// - `attempt_summary`：脱敏尝试摘要；`None` 不记尝试。
pub fn prepare_failed_message_task(
    message_id: InboxMessageId,
    error_class: ErrorClass,
    actor_id: &str,
    owner_org_unit_id: String,
    attempt_summary: Option<String>,
    attempt_at: Instant,
) -> Result<IntegrationErrorTask> {
    let mut task = IntegrationErrorTask::with_derived_owner_role(
        IntegrationErrorTaskId::new(next_id()),
        Some(message_id),
        None,
        error_class,
        actor_id.to_string(),
        owner_org_unit_id,
    )?;
    if attempt_summary.is_some() {
        task.record_attempt(attempt_at, attempt_summary)?;
    }
    Ok(task)
}

/// 在调用方 Executor 写入收到的消息。成功时消息已写入。
///
/// 沿用调用方的执行器，不另开事务。
pub async fn persist_inbox_message(
    db: &Database,
    message: &InboxMessage,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.inbox_messages().create(message, executor).await?;
    Ok(())
}
/// 在调用方 Executor 按原版本写回消息。
///
/// 成功时消息已按原版本更新。版本冲突或仓储写入失败时返回对应错误。
///
/// 沿用调用方的执行器，不另开事务。
pub async fn update_inbox_message(
    db: &Database,
    message: &mut InboxMessage,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.inbox_messages().update(message, executor).await?;
    Ok(())
}
/// 登记消息处理失败时，先插入错误任务，再按原版本更新消息。
/// 两步复用调用方事务的执行器；任一步失败立即返回，由调用方处理回滚。
pub async fn persist_error_task_with_message_failure(
    db: &Database,
    task: &IntegrationErrorTask,
    message: &mut InboxMessage,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.integration_ops().create_error_task_with_message_failure(task, message, executor).await?;
    Ok(())
}

#[cfg(test)]
mod authorization_tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::service::IntegrationAccess;

    #[tokio::test]
    async fn inbox_reads_require_identity_before_loading_messages() {
        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let db = client.database("inbox_authorization_test");
        let access = IntegrationAccess::new(db.clone(), unavailable_authorization());
        let service = IntegrationOpsService::new(db, access);
        let actor = AuditActor::new("user".into(), "login".into(), AccountKind::Admin);
        let params = serde_json::from_value::<InboxMessageListParams>(serde_json::json!({})).unwrap();

        let errors = [
            service
                .inbox_page(&params, &actor, &mut SnapshotExecutor::default())
                .await
                .expect_err("必须拒绝"),
            service
                .inbox_detail("message", &actor, &mut SnapshotExecutor::default())
                .await
                .expect_err("必须拒绝"),
        ];
        for error in errors {
            assert!(matches!(error, Error::Internal(reason) if reason == "测试授权设施不可用"));
        }
    }
}
