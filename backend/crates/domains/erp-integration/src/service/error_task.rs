//! 集成本域查询、实体准备与调用方事务内写入。
use application_core::AuditActor;
use erp_identity::CompiledAccess;
use erp_permissions::integration_ops;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::IntegrationOpsService;
use super::scope::{ScopedIntegrationList, ensure_page, resolve_list_scope};
use crate::dto::{self, *};
use crate::entity::integration_ops::*;
use crate::repository::IntegrationOpsExt;
use crate::repository::integration_ops::IntegrationErrorTaskFilter;
use crate::repository::prelude::*;
use crate::{Error, Result};

impl IntegrationOpsService {
    /// 分页查询集成错误任务列表。
    ///
    /// 返回已执行完整授权条件的分页视图及范围元数据。查询参数非法、范围版本变化、授权或组织展开失败、仓储查询或事务失败时返回对应错误。
    pub async fn error_task_list(
        &self,
        params: &ErrorTaskListParams,
        actor: &AuditActor,
    ) -> Result<ErrorTaskListView> {
        let params = params.clone();
        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (view, access) = this.error_task_list_read(&params, &actor, executor).await?;
                    this.data_scope.validate(&access, executor).await?;
                    Ok(view)
                })
            })
            .await
    }

    /// 在调用方读取阶段取得完整授权分页及待交付的原查询凭证。
    ///
    /// 分页视图及查询凭证；调用方必须补齐展示后通过 `IntegrationAccess::validate` 才能交付。参数非法、资格不足、
    /// 范围变化或分页读取失败时拒绝。
    ///
    /// `params` 为原列表筛选，`actor` 为当前账号，`executor` 为用例原执行器。
    pub async fn error_task_list_read(
        &self,
        params: &ErrorTaskListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(ErrorTaskListView, CompiledAccess)> {
        params.validate()?;
        let query = params.normalized()?;
        ensure_page(query.paging.page, query.scope_version.as_deref())?;

        let scope = resolve_list_scope(
            &self.data_scope,
            actor,
            &integration_ops::ERROR_TASK_LIST,
            &query.org_unit_ids,
            query.include_descendants,
            query.scope_version.as_deref(),
            executor,
        )
        .await?;

        let filter = error_task_filter(query, scope.owner_org_unit_ids);
        let page = self
            .db
            .integration_error_tasks()
            .search_error_tasks(&filter, &scope.read_scope, executor)
            .await?;
        Ok((error_task_list_view(page, &filter, scope.meta), scope.read_scope))
    }

    /// 确认关联入站消息存在；不校验处理状态。
    ///
    /// 消息存在时成功。消息不存在时返回 `NotFound`；仓储读取失败时返回对应错误。
    ///
    /// 使用原用例授权事务或读取阶段的执行器。
    pub async fn ensure_message_exists(&self, id: &str, executor: &mut dyn Executor) -> Result<()> {
        self.db
            .inbox_messages()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("关联消息不存在".to_string()))?;
        Ok(())
    }
}
/// 由已校验请求构造原责任政策下的错误任务。
///
/// 返回带派生责任角色的错误任务。实体字段或责任不变量失败时返回 `Logic`。
///
/// `owner_org_unit_id` 为处理人当前内部组织。
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

// 把已规范化查询与授权条件装配为仓储筛选。
fn error_task_filter(
    query: dto::ErrorTaskListQuery,
    owner_org_unit_ids: Vec<String>,
) -> IntegrationErrorTaskFilter {
    IntegrationErrorTaskFilter {
        q: query.q,
        message_id: query.message_id,
        business_object_id: query.business_object_id,
        error_class: query.error_class,
        status: query.status,
        owner_role: query.owner_role,
        handler_user_ids: query.handler_user_ids,
        operator_user_ids: query.operator_user_ids,
        owner_org_unit_ids,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
    }
}

// 组装错误任务分页响应，供调用方返回当前筛选及授权范围的摘要。
fn error_task_list_view(
    page: persistence_core::PageResult<crate::repository::integration_ops::IntegrationErrorTaskRow>,
    filter: &IntegrationErrorTaskFilter,
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
        business_object_label: None,
        message_label: None,
        error_class: row.error_class,
        status: row.status,
        owner_role: row.owner_role,
        owner_user_id: row.owner_user_id,
        owner_user_name: None,
        attempt_count: row.attempt_count,
        last_attempt_at: row.last_attempt_at.map(|at| at.unix_secs()),
        last_attempt_summary: row.last_attempt_summary,
        resolution_type: row.resolution_type,
        resolved_at: row.resolved_at.map(|at| at.unix_secs()),
        version: row.version,
        created_at: row.created_at,
    }
}

/// 在调用方事务内保存错误任务。成功时任务已写入。
pub async fn persist_error_task(
    db: &Database,
    task: &IntegrationErrorTask,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.integration_error_tasks().create(task, executor).await?;
    Ok(())
}
