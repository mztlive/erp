//! 集成本域查询、实体准备与调用方事务内写入。
use application_core::AuditActor;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::IntegrationOpsService;
use super::scope::{ScopedIntegrationList, ensure_page, resolve_list_scope};
use crate::dto::{self, *};
use crate::entity::integration_ops::*;
use crate::repository::IntegrationOpsExt;
use crate::{Error, Result};
/// 错误任务列表筛选条件类型。
type ErrorTaskFilter = <Database as IntegrationOpsExt>::IntegrationErrorTaskFilter;
impl IntegrationOpsService {
    /// 分页查询集成错误任务列表。
    ///
    /// # 错误
    /// 查询参数非法、范围变化或仓储查询失败时返回错误。
    pub async fn error_task_list(
        &self,
        params: &ErrorTaskListParams,
        actor: &AuditActor,
    ) -> Result<ErrorTaskListView> {
        params.validate()?;
        let query = params.normalized()?;
        ensure_page(query.paging.page, query.scope_version.as_deref())?;
        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.error_task_page(query, &actor, executor).await })
            })
            .await
    }

    async fn error_task_page(
        &self,
        query: dto::ErrorTaskListQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ErrorTaskListView> {
        let access = self.access();
        let scope = resolve_list_scope(
            &access,
            actor,
            "integration_error_task",
            &query.org_unit_ids,
            query.include_descendants,
            query.scope_version.as_deref(),
            executor,
        )
        .await?;
        let filter = error_task_filter(&query, &scope.read_scope, scope.owner_org_unit_ids);
        if scope.meta.empty_reason == Some("no_scope") {
            return Ok(empty_error_task_page(&filter, scope.meta));
        }
        let page = self.db.integration_error_tasks().search_error_tasks(&filter, executor).await?;
        Ok(error_task_list_view(page, &filter, scope.meta))
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
pub fn prepare_error_task(
    req: &CreateErrorTaskRequest,
    owner_org_unit_id: String,
) -> Result<IntegrationErrorTask> {
    Ok(IntegrationErrorTask::with_derived_owner_role(
        IntegrationErrorTaskId::new(next_id()),
        req.message_id.clone(),
        req.business_object_id.clone(),
        req.error_class,
        req.owner_user_id.clone(),
        owner_org_unit_id,
    )?)
}

/// 把已规范化查询与授权条件装配为仓储筛选。
fn error_task_filter(
    query: &dto::ErrorTaskListQuery,
    read_scope: &crate::repository::IntegrationReadScope,
    owner_org_unit_ids: Vec<String>,
) -> ErrorTaskFilter {
    ErrorTaskFilter {
        q: query.q.clone(),
        message_id: query.message_id.clone(),
        business_object_id: query.business_object_id.clone(),
        error_class: query.error_class,
        status: query.status,
        owner_role: query.owner_role.clone(),
        handler_user_ids: query.handler_user_ids.clone(),
        operator_user_ids: query.operator_user_ids.clone(),
        owner_org_unit_ids,
        scope_document: Some(read_scope.document()),
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
    }
}

/// 装配带范围元数据的错误任务列表响应。
fn error_task_list_view(
    page: persistence_core::PageResult<crate::repository::integration_ops::IntegrationErrorTaskRow>,
    filter: &ErrorTaskFilter,
    meta: ScopedIntegrationList,
) -> ErrorTaskListView {
    ErrorTaskListView {
        data: PageView {
            items: page.items.into_iter().map(map_error_task_row).collect(),
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        },
        scope_version: meta.scope_version,
        policy_version: meta.policy_version,
        organization_version: meta.organization_version,
        as_of: meta.as_of,
        empty_reason: None,
        scope_summary: meta.scope_summary,
        ownership_basis: meta.ownership_basis,
    }
}

fn map_error_task_row(row: crate::repository::integration_ops::IntegrationErrorTaskRow) -> ErrorTaskView {
    ErrorTaskView {
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
    }
}

fn empty_error_task_page(filter: &ErrorTaskFilter, meta: ScopedIntegrationList) -> ErrorTaskListView {
    ErrorTaskListView {
        data: PageView { items: Vec::new(), total: 0, page: filter.page, page_size: filter.page_size },
        scope_version: meta.scope_version,
        policy_version: meta.policy_version,
        organization_version: meta.organization_version,
        as_of: meta.as_of,
        empty_reason: meta.empty_reason,
        scope_summary: meta.scope_summary,
        ownership_basis: meta.ownership_basis,
    }
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
