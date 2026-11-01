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
use crate::repository::prelude::*;
use crate::{Error, Result};
/// 错误任务列表筛选条件类型。
type ErrorTaskFilter = <Database as IntegrationOpsExt>::IntegrationErrorTaskFilter;
impl IntegrationOpsService {
    /// 分页查询集成错误任务列表。
    ///
    /// # 参数
    /// * `params` - 列表查询参数
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回带范围元数据的分页视图；角色无有效范围时条目为空且 `empty_reason` 为 `no_scope`。
    ///
    /// # 错误
    /// 查询参数非法、范围版本变化、授权或组织展开失败、仓储查询或事务失败时返回对应错误。
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

    /// 无有效范围时不查库，直接返回空页。
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
        let filter = error_task_filter(query, &scope.read_scope, scope.owner_org_unit_ids);
        if scope.meta.empty_reason == Some("no_scope") {
            return Ok(empty_error_task_page(&filter, scope.meta));
        }
        let page = self.db.integration_error_tasks().search_error_tasks(&filter, executor).await?;
        Ok(error_task_list_view(page, &filter, scope.meta))
    }

    /// 确认关联入站消息存在；不校验处理状态。
    ///
    /// # 参数
    /// * `id` - 消息 ID
    ///
    /// # 返回
    /// 消息存在时成功。
    ///
    /// # 错误
    /// 消息不存在时返回 `NotFound`；仓储读取失败时返回对应错误。
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
///
/// # 参数
/// * `req` - 错误任务登记请求
/// * `owner_org_unit_id` - 处理人当前内部组织
///
/// # 返回
/// 返回带派生责任角色的错误任务。
///
/// # 错误
/// 实体字段或责任不变量失败时返回 `Logic`。
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
    query: dto::ErrorTaskListQuery,
    read_scope: &crate::repository::IntegrationReadScope,
    owner_org_unit_ids: Vec<String>,
) -> ErrorTaskFilter {
    ErrorTaskFilter {
        q: query.q,
        message_id: query.message_id,
        business_object_id: query.business_object_id,
        error_class: query.error_class,
        status: query.status,
        owner_role: query.owner_role,
        handler_user_ids: query.handler_user_ids,
        operator_user_ids: query.operator_user_ids,
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
///
/// # 参数
/// * `db` - 目标数据库
/// * `task` - 待写入的错误任务
/// * `executor` - 调用方执行器
///
/// # 返回
/// 成功时任务已写入。
///
/// # 错误
/// 仓储写入失败时返回对应错误。
pub async fn persist_error_task(
    db: &Database,
    task: &IntegrationErrorTask,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.integration_error_tasks().create(task, executor).await?;
    Ok(())
}
