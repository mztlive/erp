//! 集成本域查询、实体准备与调用方事务内写入。
use application_core::AuditActor;
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
use crate::repository::integration_ops::reader::IntegrationReader;
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
    /// 返回已执行完整授权条件的分页视图及范围元数据。
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
                Box::pin(async move { this.error_task_page(query, &actor, executor, &this.db).await })
            })
            .await
    }

    /// 在原事务内完成授权、根分页与交付复验。
    async fn error_task_page(
        &self,
        query: dto::ErrorTaskListQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
        reader: &dyn IntegrationReader,
    ) -> Result<ErrorTaskListView> {
        let access = self.access();
        let scope = resolve_list_scope(
            &access,
            actor,
            &integration_ops::ERROR_TASK_LIST,
            &query.org_unit_ids,
            query.include_descendants,
            query.scope_version.as_deref(),
            executor,
        )
        .await?;
        let filter = error_task_filter(query, scope.owner_org_unit_ids);
        let page = reader.error_page(&filter, &scope.read_scope, executor).await?;
        access.validate(&scope.read_scope, executor).await?;
        Ok(error_task_list_view(page, &filter, scope.meta))
    }

    /// 确认关联入站消息存在；不校验处理状态。
    ///
    /// # 参数
    /// * `executor` - 原用例授权事务或读取阶段的执行器。
    /// * `id` - 消息 ID
    ///
    /// # 返回
    /// 消息存在时成功。
    ///
    /// # 错误
    /// 消息不存在时返回 `NotFound`；仓储读取失败时返回对应错误。
    pub async fn ensure_message_exists(&self, id: &str, executor: &mut dyn Executor) -> Result<()> {
        self.db
            .inbox_messages()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("关联消息不存在".to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod authorization_tests {
    use std::sync::Arc;

    use erp_core::AccountKind;
    use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::testing::{unavailable_authorization, unbound_authorization};
    use persistence_core::Error as PersistenceError;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::repository::integration_ops::reader::testing::{
        Behavior, MemoryReader, actor, own_scope, plan_denies, plan_owned_by, service,
    };
    use crate::service::IntegrationAccess;

    #[tokio::test]
    async fn error_page_runs_real_query_and_validate_with_the_list_permission() {
        let reader = Arc::new(MemoryReader::default());
        let permission = integration_ops::ERROR_TASK_LIST;
        let service = service(vec![permission.clone()], &[permission], own_scope(), reader.clone()).await;
        let mut executor = SnapshotExecutor::default();
        let params =
            ErrorTaskListParams { business_object_id: Some("business".into()), ..Default::default() };
        let view = service
            .error_task_page(params.normalized().unwrap(), &actor(), &mut executor, reader.as_ref())
            .await
            .unwrap();
        assert_eq!((view.data.total, view.data.page, view.data.items.len()), (5, 1, 1));
        assert_eq!(
            (&*view.data.items[0].id, view.data.items[0].attempt_count, view.data.items[0].version),
            ("error", 2, 5)
        );
        let accesses = reader.accesses.lock().unwrap();
        let access = &accesses[0];
        assert_eq!(access.model().resource, "integration_error_task");
        assert!(plan_owned_by(access, "reader"));
        assert_eq!(view.scope_version, access.version());
        assert!(access.ensure_current(&mut executor).is_err(), "真实 validate 必须结束原凭证阶段");
        assert_eq!(*reader.events.lock().unwrap(), ["error_page"]);
        assert_eq!(reader.error_filters.lock().unwrap()[0].business_object_id.as_deref(), Some("business"));
    }

    #[tokio::test]
    async fn error_list_rejects_detail_only_missing_permission_and_other_identity_without_page_reads() {
        for (grants, user) in [
            (vec![], actor()),
            (vec![integration_ops::ERROR_TASK_DETAIL], actor()),
            (
                vec![integration_ops::ERROR_TASK_LIST],
                AuditActor::new("other".into(), "other".into(), AccountKind::Admin),
            ),
        ] {
            let reader = Arc::new(MemoryReader::default());
            let service =
                service(grants, &[integration_ops::ERROR_TASK_LIST], own_scope(), reader.clone()).await;
            assert!(matches!(
                service
                    .error_task_page(
                        ErrorTaskListParams::default().normalized().unwrap(),
                        &user,
                        &mut SnapshotExecutor::default(),
                        reader.as_ref()
                    )
                    .await,
                Err(Error::Forbidden(_))
            ));
            assert!(reader.events.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn error_list_empty_role_scope_retains_a_deny_plan_despite_company_personal_limit() {
        let reader = Arc::new(MemoryReader::with_behavior(Behavior::Empty));
        let permission = integration_ops::ERROR_TASK_LIST;
        let scope = ResolvedScope {
            role_clauses: vec![],
            user_limit: Some(ScopeClause { company: true, ..Default::default() }),
        };
        let service = service(vec![permission.clone()], &[permission], scope, reader.clone()).await;
        let mut executor = SnapshotExecutor::default();
        let view = service
            .error_task_page(
                ErrorTaskListParams::default().normalized().unwrap(),
                &actor(),
                &mut executor,
                reader.as_ref(),
            )
            .await
            .unwrap();
        assert!(view.data.items.is_empty());
        assert_eq!(view.data.total, 0);
        let accesses = reader.accesses.lock().unwrap();
        assert!(!accesses[0].has_role_scope());
        assert!(plan_denies(&accesses[0]));
        assert!(accesses[0].ensure_current(&mut executor).is_err());
    }

    #[tokio::test]
    async fn error_list_discards_pages_on_storage_failure_or_read_stage_change() {
        for behavior in [Behavior::PageFailure, Behavior::PageStage] {
            let reader = Arc::new(MemoryReader::with_behavior(behavior));
            let permission = integration_ops::ERROR_TASK_LIST;
            let service = service(vec![permission.clone()], &[permission], own_scope(), reader.clone()).await;
            let error = service
                .error_task_page(
                    ErrorTaskListParams::default().normalized().unwrap(),
                    &actor(),
                    &mut SnapshotExecutor::default(),
                    reader.as_ref(),
                )
                .await
                .unwrap_err();
            if behavior == Behavior::PageFailure {
                let expected = Error::from(PersistenceError::EntityMetadataOutOfRange("测试分页读取失败"));
                assert_eq!(error.to_string(), expected.to_string());
            } else {
                assert!(
                    matches!(error, Error::ValidationError(message) if message == "授权结果不能跨事务或读取阶段使用")
                );
            }
            assert_eq!(*reader.events.lock().unwrap(), ["error_page"]);
        }
    }

    #[tokio::test]
    async fn error_list_rejects_changed_scope_version_before_reading_a_page() {
        let reader = Arc::new(MemoryReader::default());
        let permission = integration_ops::ERROR_TASK_LIST;
        let service = service(vec![permission.clone()], &[permission], own_scope(), reader.clone()).await;
        let params = ErrorTaskListParams { scope_version: Some("stale".into()), ..Default::default() };
        assert!(
            matches!(service.error_task_page(params.normalized().unwrap(), &actor(), &mut SnapshotExecutor::default(), reader.as_ref()).await, Err(Error::ConflictError(message)) if message.starts_with("DATA_SCOPE_CHANGED"))
        );
        assert!(reader.events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn error_task_page_rejects_missing_mapping_and_identity_failure_before_business_reads() {
        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let db = client.database("error_task_authorization_test");
        let actor = AuditActor::new("user".into(), "login".into(), AccountKind::Admin);
        for (authorization, missing_mapping) in
            [(unbound_authorization(), true), (unavailable_authorization(), false)]
        {
            let access = IntegrationAccess::new(db.clone(), authorization);
            let service = IntegrationOpsService::new(db.clone(), access);
            let params = ErrorTaskListParams::default();
            let error = service
                .error_task_page(params.normalized().unwrap(), &actor, &mut SnapshotExecutor::default(), &db)
                .await
                .expect_err("授权失败必须先于业务仓储读取");
            if missing_mapping {
                assert!(matches!(error, Error::ValidationError(reason) if reason == "缺少查询 Mapping 绑定"));
            } else {
                assert!(matches!(error, Error::Internal(reason) if reason == "测试授权设施不可用"));
            }
        }
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
fn error_task_filter(query: dto::ErrorTaskListQuery, owner_org_unit_ids: Vec<String>) -> ErrorTaskFilter {
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
