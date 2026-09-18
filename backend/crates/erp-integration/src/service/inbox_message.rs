//! 集成本域查询、实体准备与调用方事务内写入。
use erp_core::common::time::Instant;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::IntegrationOpsService;
use crate::dto::*;
use crate::entity::integration_ops::*;
use crate::repository::IntegrationOpsExt;
use crate::repository::prelude::*;
use crate::{Error, Result};
/// 入站消息列表筛选条件类型（经 `IntegrationOpsExt` 关联类型跨 crate 可达）。
type InboxMessageFilter = <Database as IntegrationOpsExt>::InboxMessageFilter;
impl IntegrationOpsService {
    /// 分页查询入站消息列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传；
    /// 投影行类型属于仓储私有子树（`repository/mod.rs` 冻结，无法命名），
    /// 此处按字段映射为响应视图。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn inbox_message_list(
        &self,
        params: &InboxMessageListParams,
    ) -> Result<PageView<InboxMessageListView>> {
        params.validate()?;
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
        let page = self.db.inbox_messages().search_inbox_messages(&filter, &mut NoTransaction).await?;
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
    /// # 参数
    /// * `id` - 消息 ID
    ///
    /// # 返回
    /// 返回消息详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 消息不存在
    pub async fn inbox_message_detail(&self, id: &str) -> Result<InboxMessageView> {
        let message = self
            .db
            .inbox_messages()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("消息不存在".to_string()))?;
        Ok(message.into())
    }
}
/// 构造已通过请求与来源存在性检查的入站消息。
/// # Errors
/// 保留原 InboxMessage::received 不变量错误。
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
/// # Errors
/// 保留原消息状态校验失败。
pub fn apply_processed_outcome(message: &mut InboxMessage, processed_at: Instant) -> Result<()> {
    message.update(InboxMessageUpdate {
        status: Some(InboxMessageStatus::Processed),
        processed_at: Some(processed_at),
    })?;
    Ok(())
}
/// 应用 failed 结果，不生成任务或写入数据库。
/// # Errors
/// 保留原消息状态校验失败。
pub fn apply_failed_outcome(message: &mut InboxMessage) -> Result<()> {
    message.update(InboxMessageUpdate { status: Some(InboxMessageStatus::Failed), processed_at: None })?;
    Ok(())
}
/// 按原责任政策构造失败任务；仅摘要存在时记录尝试。
/// # Errors
/// 任务或尝试不满足原实体不变量时返回原错误。
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

/// 在调用方 Executor 写入收到的消息。
/// # Errors
/// 持久化失败时返回原仓储错误。
pub async fn persist_inbox_message(
    db: &Database,
    message: &InboxMessage,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.inbox_messages().create(message, executor).await?;
    Ok(())
}
/// 在调用方 Executor 按原版本写回消息。
/// # Errors
/// CAS 或持久化失败时返回原错误。
pub async fn update_inbox_message(
    db: &Database,
    message: &mut InboxMessage,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.inbox_messages().update(message, executor).await?;
    Ok(())
}
/// 错误任务写入后更新失败消息，复用调用方 Executor。
/// # Errors
/// 任一步写入失败立即返回原错误。
pub async fn persist_error_task_with_message_failure(
    db: &Database,
    task: &IntegrationErrorTask,
    message: &mut InboxMessage,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.integration_ops().create_error_task_with_message_failure(task, message, executor).await?;
    Ok(())
}
