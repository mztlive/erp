//! 运行实例 Mine / Started / Managed 列表查询。

use application_core::AuditActor;
use erp_identity::CompiledAccess;
use erp_permissions::internal as internal_permissions;
use persistence_core::{Executor, Transactional};

use super::super::read_auth::{
    ensure_mine_page_integrity, mine_execution_ids, mine_instance_ids, mine_runtime_chain_matches,
    unique_by_id,
};
use super::{
    ApprovalRuntimeService, RuntimeInstanceListCursor, RuntimeInstanceListItem, RuntimeInstanceListPage,
    RuntimeInstanceListQuery, RuntimeInstanceListView, cursor_from_summary, hidden_not_found,
    instance_list_filter, item_from_runtime_read_row, item_from_summary, parse_document_type,
};
use crate::entity::document_registry::DocumentType;
use crate::entity::work_item::WorkItem;
use crate::error::{Error, Result};
use crate::repository::approval_integration::{
    ApprovalRuntimeReadPage, ApprovalRuntimeReadRepository, ApprovalRuntimeReadScope,
    ApprovalRuntimeReadTypeScope,
};
use crate::repository::bpm::ApprovalInstanceListFilter;
use crate::repository::prelude::*;
use crate::repository::work_item::{DocumentApprovalPageQuery, DocumentApprovalWorkItemPage};
use crate::repository::{ApprovalIntegrationExt, BpmExt, WorkItemExt};
use crate::service::approval::business_adapter::adapter_spec_of;
use crate::service::approval::policy::{ALL_DOCUMENT_TYPES, DocumentApprovalPolicy, policy_of};
use crate::service::approval::process_kind::process_kind_of;

/// 保持既有授权结果规模上限；数据库在完整过滤后计算总数。
const MAX_AUTHORIZED_INSTANCES: u64 = 20_000;

// 本人发起与管理实例视图共用固定查询结果上限，超过时维持原有拒绝错误。
fn ensure_runtime_result_limit(total: u64) -> Result<()> {
    if total > MAX_AUTHORIZED_INSTANCES {
        return Err(Error::ValidationError("审批查询授权结果超过 20000 条，请增加类型或业务筛选".into()));
    }
    Ok(())
}

impl<A: crate::ports::WorkflowAuthorizationPort> ApprovalRuntimeService<A> {
    // 返回由开放审批任务映射的运行实例列表页。
    // 任务类型、开放状态、责任人与可选单据类型全部由 Repository 固定查询封装。
    //
    // 返回当前账号拥有的开放单据审批任务页。
    //
    // WorkItem Repository 查询失败时返回错误。
    //
    // `actor`：当前已认证账号；`query`：可选单据类型与页大小。
    pub(super) async fn list_mine(
        &self,
        actor: &AuditActor,
        query: &RuntimeInstanceListQuery,
    ) -> Result<RuntimeInstanceListPage> {
        let service = self.clone();
        let actor = actor.clone();
        let query = query.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.list_mine_stage(&actor, &query, executor).await })
            })
            .await
    }

    // 本人开放审批任务使用中央实例查询范围；候选完整性检查、分页及交付复验沿同一快照。
    async fn list_mine_stage(
        &self,
        actor: &AuditActor,
        query: &RuntimeInstanceListQuery,
        executor: &mut dyn Executor,
    ) -> Result<RuntimeInstanceListPage> {
        let access = match self
            .authorization
            .query(actor, &internal_permissions::APPROVAL_MINE_LIST, executor)
            .await
        {
            Ok(access) => access,
            Err(erp_identity::Error::Forbidden(_)) => {
                return Ok(RuntimeInstanceListPage {
                    items: Vec::new(),
                    total: 0,
                    next_cursor: None,
                    document_type_counts: None,
                });
            },
            Err(error) => return Err(error.into()),
        };

        let page = self.mine_task_page(actor, query, &access, executor).await?;
        ensure_mine_page_integrity(page.integrity_conflicts.len())?;
        let items = self.hydrate_mine_items(actor, page.items, executor).await?;
        let next_cursor = if page.has_more {
            page.next_cursor.map(|(sort_time, id)| RuntimeInstanceListCursor { sort_time, id })
        } else {
            None
        };

        self.authorization.validate(&access, executor).await?;
        Ok(RuntimeInstanceListPage { items, total: page.total, next_cursor, document_type_counts: None })
    }

    // 为本人审批查询追加业务筛选，只收窄中央实例凭证已允许的范围，不另行选择授权规则。
    async fn mine_task_page(
        &self,
        actor: &AuditActor,
        query: &RuntimeInstanceListQuery,
        access: &CompiledAccess,
        executor: &mut dyn Executor,
    ) -> Result<DocumentApprovalWorkItemPage> {
        let document_type = query.document_type.as_deref().map(parse_document_type).transpose()?;
        let cursor = query
            .cursor
            .as_ref()
            .map(|cursor| {
                if cursor.sort_time < 0 {
                    return Err(Error::ValidationError(
                        "我的待审批列表的翻页位置无效，请返回第一页重新查询".to_string(),
                    ));
                }
                Ok((cursor.sort_time, cursor.id.as_str()))
            })
            .transpose()?;
        self.db
            .work_items()
            .page_open_document_approval_owned_by(
                DocumentApprovalPageQuery {
                    owner_user_id: actor.id(),
                    business_object_type: document_type.map(|document_type| document_type.as_str()),
                    query: query.query.as_deref(),
                    cursor,
                    limit: query.limit,
                },
                access,
                executor,
            )
            .await
    }

    // 为本人审批页批量加载节点执行、实例、摘要及冻结快照，再按原任务顺序组装结果。
    // 任一责任链与冻结身份不符时拒绝整页，不能丢行后仍返回原总数。
    async fn hydrate_mine_items(
        &self,
        actor: &AuditActor,
        tasks: Vec<WorkItem>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<RuntimeInstanceListItem>> {
        let execution_ids = mine_execution_ids(&tasks)?;
        let executions = self.db.bpm_workflow().list_executions_by_ids(&execution_ids, executor).await?;
        let execution_by_id = unique_by_id(executions, |execution| execution.base.id.clone())?;

        let instance_ids = mine_instance_ids(&execution_ids, &execution_by_id)?;
        let summaries =
            self.db.bpm_workflow().list_instance_summaries_by_ids(&instance_ids, executor).await?;
        let mut summary_by_id = unique_by_id(summaries, |summary| summary.id.clone())?;
        let instance_id_strings = instance_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        let snapshots = self
            .db
            .approval_subject_snapshots()
            .find_by_process_instance_ids(&instance_id_strings, executor)
            .await?;
        let snapshot_by_instance =
            unique_by_id(snapshots, |snapshot| snapshot.approval_process_instance_id.to_string())?;

        tasks
            .into_iter()
            .map(|task| {
                let execution_id = task.approval_node_execution_id.as_ref().ok_or_else(hidden_not_found)?;
                let execution = execution_by_id.get(execution_id.as_ref()).ok_or_else(hidden_not_found)?;
                let summary = summary_by_id
                    .remove(execution.process_instance_id.as_ref())
                    .ok_or_else(hidden_not_found)?;
                let snapshot = snapshot_by_instance.get(summary.id.as_str());
                if !mine_runtime_chain_matches(&task, execution, &summary, snapshot, actor.id())? {
                    return Err(hidden_not_found());
                }
                item_from_summary(summary, snapshot)
            })
            .collect()
    }

    /// 在调用方快照中读取本人发起审批的一批授权候选。
    /// 返回完整授权后的候选页，用于跨域内容检索。
    ///
    /// 非 started、无效账号、权限或仓储失败时拒绝。
    ///
    /// `actor` 为当前账号，`query` 仅允许 started，`executor` 为调用方事务。
    pub async fn started_instance_page_with(
        &self,
        actor: &AuditActor,
        query: &RuntimeInstanceListQuery,
        executor: &mut dyn Executor,
    ) -> Result<(Vec<RuntimeInstanceListItem>, Option<RuntimeInstanceListCursor>)> {
        query.validate()?;
        if query.view != RuntimeInstanceListView::Started {
            return Err(Error::ValidationError("工作台审批查询只允许本人发起范围".into()));
        }
        self.scan_scoped_workspace_page(actor, query, executor).await
    }

    /// 在调用方快照中按视图（`started` / `mine` / `managed` / `blocked`）读取一批授权候选。
    /// 返回原顺序的候选页与续读游标，供工作台全量检索与结构化筛选使用。
    ///
    /// 参数非法、授权失败或仓储错误时拒绝。
    pub async fn workspace_instance_page_with(
        &self,
        actor: &AuditActor,
        query: &RuntimeInstanceListQuery,
        executor: &mut dyn Executor,
    ) -> Result<(Vec<RuntimeInstanceListItem>, Option<RuntimeInstanceListCursor>)> {
        query.validate()?;
        if query.view == RuntimeInstanceListView::Mine {
            let page = self.list_mine_stage(actor, query, executor).await?;
            return Ok((page.items, page.next_cursor));
        }
        self.scan_scoped_workspace_page(actor, query, executor).await
    }

    // 在调用方事务快照中扫描 Started / Managed / Blocked 授权实例批次。
    async fn scan_scoped_workspace_page(
        &self,
        actor: &AuditActor,
        query: &RuntimeInstanceListQuery,
        executor: &mut dyn Executor,
    ) -> Result<(Vec<RuntimeInstanceListItem>, Option<RuntimeInstanceListCursor>)> {
        let type_scopes = self.runtime_read_type_scopes(query)?;
        let filter = instance_list_filter(actor, query)?;
        let (permission, scope) = if query.view == RuntimeInstanceListView::Started {
            (
                &internal_permissions::APPROVAL_STARTED_LIST,
                ApprovalRuntimeReadScope::Started {
                    process_kinds: type_scopes.iter().map(|scope| scope.process_kind).collect(),
                    submitted_by: actor.id().to_owned(),
                },
            )
        } else {
            (
                &internal_permissions::APPROVAL_MANAGEMENT_READ,
                ApprovalRuntimeReadScope::Managed { type_scopes: type_scopes.clone() },
            )
        };

        let access = self.authorization.query(actor, permission, executor).await?;
        let (rows, last) = ApprovalRuntimeReadRepository::new(&self.db)
            .scan_authorized(&filter, &scope, &access, executor)
            .await?;

        let next = last.map(|row| cursor_from_summary(filter.view, &row));
        let items = rows
            .into_iter()
            .map(|row| item_from_runtime_read_row(row, actor, query.view, &type_scopes))
            .collect::<Result<Vec<_>>>()?;

        self.authorization.validate(&access, executor).await?;
        Ok((items, next))
    }

    // 查询本人发起或管理范围内的审批实例。
    // 检索必须在 MongoDB 内施加。不得先取当前页再内存过滤。
    //
    // 所有视图均返回数据库在完整授权过滤后计算的分页和总数。
    //
    // 单据类型未登记或仓储失败时返回错误。
    //
    // `actor`：当前已认证账号；`query`：已规范化查询，可含字面量检索。
    pub(super) async fn list_managed_or_started(
        &self,
        actor: &AuditActor,
        query: &RuntimeInstanceListQuery,
    ) -> Result<RuntimeInstanceListPage> {
        let this = self.clone();
        let actor = actor.clone();
        let query = query.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.list_scoped_instances(&actor, &query, executor).await })
            })
            .await
    }

    // 在一个事务内形成类型范围、完整授权总数与稳定页面。
    async fn list_scoped_instances(
        &self,
        actor: &AuditActor,
        query: &RuntimeInstanceListQuery,
        executor: &mut dyn Executor,
    ) -> Result<RuntimeInstanceListPage> {
        let type_scopes = self.runtime_read_type_scopes(query)?;
        let mut filter = instance_list_filter(actor, query)?;
        filter.limit = query.limit.saturating_add(1);
        let scope = if query.view == RuntimeInstanceListView::Started {
            ApprovalRuntimeReadScope::Started {
                process_kinds: type_scopes.iter().map(|scope| scope.process_kind).collect(),
                submitted_by: actor.id().to_string(),
            }
        } else {
            ApprovalRuntimeReadScope::Managed { type_scopes: type_scopes.clone() }
        };

        let (mut page, access) =
            self.scoped_runtime_page(actor, query.view, &filter, &scope, executor).await?;
        ensure_runtime_result_limit(page.total)?;

        let has_more = page.items.len() > query.limit as usize;
        if has_more {
            page.items.truncate(query.limit as usize);
        }
        let next_cursor = has_more
            .then(|| page.items.last().map(|row| cursor_from_summary(filter.view, &row.instance)))
            .flatten();
        let items = page
            .items
            .into_iter()
            .map(|row| item_from_runtime_read_row(row, actor, query.view, &type_scopes))
            .collect::<Result<Vec<_>>>()?;
        let result = RuntimeInstanceListPage {
            items,
            total: page.total,
            next_cursor,
            document_type_counts: (query.view == RuntimeInstanceListView::Started)
                .then_some(page.document_type_counts),
        };

        self.authorization.validate(&access, executor).await?;
        Ok(result)
    }

    // 在原读取阶段编译该视图的固定权限，数据库先应用完整授权再进行计数及分页。
    async fn scoped_runtime_page(
        &self,
        actor: &AuditActor,
        view: RuntimeInstanceListView,
        filter: &ApprovalInstanceListFilter,
        scope: &ApprovalRuntimeReadScope,
        executor: &mut dyn Executor,
    ) -> Result<(ApprovalRuntimeReadPage, CompiledAccess)> {
        let permission = if view == RuntimeInstanceListView::Started {
            &internal_permissions::APPROVAL_STARTED_LIST
        } else {
            &internal_permissions::APPROVAL_MANAGEMENT_READ
        };
        let access = self.authorization.query(actor, permission, executor).await?;
        let page = ApprovalRuntimeReadRepository::new(&self.db)
            .search_authorized(filter, scope, &access, executor)
            .await?;
        Ok((page, access))
    }

    // 为实例列表提供已接入的流程种类候选；当前账号、同角色资格及来源范围仍以中央 `query` 为准。
    fn runtime_read_type_scopes(
        &self,
        query: &RuntimeInstanceListQuery,
    ) -> Result<Vec<ApprovalRuntimeReadTypeScope>> {
        let requested = query.document_type.as_deref().map(parse_document_type).transpose()?;
        let mut allowed = process_required_document_types()?;
        if let Some(requested) = requested {
            allowed.retain(|document_type| *document_type == requested);
        }
        let mut scopes = Vec::new();
        for document_type in allowed {
            adapter_spec_of(document_type)?;
            scopes.push(ApprovalRuntimeReadTypeScope { process_kind: process_kind_of(document_type) });
        }
        Ok(scopes)
    }
}

// 从固定政策矩阵枚举必须接入审批运行时的类型，不把无审批类别带入实例查询。
fn process_required_document_types() -> Result<Vec<DocumentType>> {
    let mut document_types = Vec::new();
    for document_type in ALL_DOCUMENT_TYPES {
        if matches!(policy_of(document_type)?, DocumentApprovalPolicy::ProcessRequired(_)) {
            document_types.push(document_type);
        }
    }
    Ok(document_types)
}

#[cfg(test)]
mod tests {
    use persistence_core::testing::SnapshotExecutor;

    use super::{MAX_AUTHORIZED_INSTANCES, ensure_runtime_result_limit, *};
    use crate::error::Error;
    use crate::ports::FailClosedWorkflowAuthorizationPort;
    use crate::service::approval::definition::test_support::unavailable_service;

    #[tokio::test]
    async fn instance_lists_authorize_in_their_snapshot_before_business_reads() {
        let (definition, actor) = unavailable_service().await;
        let runtime = ApprovalRuntimeService::new(
            definition.db,
            definition.identity,
            definition.authorization,
            FailClosedWorkflowAuthorizationPort,
        );
        for view in [
            RuntimeInstanceListView::Mine,
            RuntimeInstanceListView::Started,
            RuntimeInstanceListView::Managed,
            RuntimeInstanceListView::Blocked,
        ] {
            let query = RuntimeInstanceListQuery::prepare(view, None, None, None, None, None).unwrap();
            let result = match view {
                RuntimeInstanceListView::Mine => {
                    runtime.list_mine_stage(&actor, &query, &mut SnapshotExecutor::default()).await
                },
                _ => runtime.list_scoped_instances(&actor, &query, &mut SnapshotExecutor::default()).await,
            };
            assert!(
                matches!(result, Err(Error::Internal(reason)) if reason == "测试授权设施不可用"),
                "{view:?}"
            );
            let workspace_result =
                runtime.workspace_instance_page_with(&actor, &query, &mut SnapshotExecutor::default()).await;
            assert!(
                matches!(workspace_result, Err(Error::Internal(reason)) if reason == "测试授权设施不可用"),
                "workspace {view:?}"
            );
        }
    }

    #[test]
    fn runtime_authorized_limit_preserves_existing_failure() {
        assert!(ensure_runtime_result_limit(MAX_AUTHORIZED_INSTANCES).is_ok());
        assert!(matches!(ensure_runtime_result_limit(MAX_AUTHORIZED_INSTANCES + 1),
            Err(Error::ValidationError(message))
                if message == "审批查询授权结果超过 20000 条，请增加类型或业务筛选"));
    }
}
