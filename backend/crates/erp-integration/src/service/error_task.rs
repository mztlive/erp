//! 集成本域查询、实体准备与调用方事务内写入。
use super::IntegrationOpsService;
use crate::dto::*;
use crate::entity::integration_ops::*;
use crate::repository::IntegrationOpsExt;
use crate::{Error, Result};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};
use validator::Validate;
/// 错误任务列表筛选条件类型。
type ErrorTaskFilter = <Database as IntegrationOpsExt>::IntegrationErrorTaskFilter;
impl IntegrationOpsService {
    /// 分页查询集成错误任务列表。
    ///
    /// # 错误
    /// 查询参数非法或仓储查询失败时返回错误。
    pub async fn error_task_list(&self, params: &ErrorTaskListParams) -> Result<PageView<ErrorTaskView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ErrorTaskFilter {
            q: query.q,
            message_id: query.message_id,
            business_object_id: query.business_object_id,
            error_class: query.error_class,
            status: query.status,
            owner_role: query.owner_role,
            owner_user_id: query.owner_user_id,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .integration_error_tasks()
            .search_error_tasks(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| ErrorTaskView {
                id: row.id,
                message_id: row.message_id.map(|id| id.to_string()),
                business_object_id: row.business_object_id,
                error_class: row.error_class,
                status: row.status,
                owner_role: row.owner_role,
                owner_user_id: row.owner_user_id,
                attempt_count: row.attempt_count,
                last_attempt_at: row.last_attempt_at.map(|at| at.unix_secs()),
                last_attempt_summary: row.last_attempt_summary,
                resolution_type: row.resolution_type,
                resolved_at: row.resolved_at.map(|at| at.unix_secs()),
                version: row.version,
                created_at: row.created_at,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    pub async fn ensure_message_exists(&self, id: &str) -> Result<()> {
        self.db
            .inbox_messages()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("关联消息不存在".to_string()))?;
        Ok(())
    }
}
/// 由已校验请求构造原责任政策下的错误任务。
/// # Errors
/// 保留原实体字段和责任不变量错误。
pub fn prepare_error_task(req: &CreateErrorTaskRequest) -> Result<IntegrationErrorTask> {
    let task = IntegrationErrorTask::new(
        IntegrationErrorTaskId::new(next_id()),
        IntegrationErrorTaskData {
            message_id: req.message_id.clone(),
            business_object_id: req.business_object_id.clone(),
            error_class: req.error_class,
            owner_role: Some(error_owner_role(req.error_class).to_string()),
            owner_user_id: Some(req.owner_user_id.clone()),
        },
    )?;

    Ok(task)
}

/// 在调用方事务内保存错误任务。
/// # Errors
/// 返回原仓储错误。
pub async fn persist_error_task(
    db: &Database,
    task: &IntegrationErrorTask,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.integration_error_tasks().create(task, executor).await?;
    Ok(())
}
