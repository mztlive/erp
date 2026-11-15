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

/// 保留原有超限拒绝错误，Started 与 Managed 使用相同结果上限。
fn ensure_runtime_result_limit(total: u64) -> Result<()> {
    if total > MAX_AUTHORIZED_INSTANCES {
        return Err(Error::ValidationError("审批查询授权结果超过 20000 条，请增加类型或业务筛选".into()));
    }
    Ok(())
}

impl<A: crate::ports::WorkflowAuthorizationPort> ApprovalRuntimeService<A> {
    /// 返回由开放审批任务映射的运行实例列表页。
    ///
    /// # 参数
    /// * `actor` - 当前已认证账号
    /// * `query` - 可选单据类型与页大小
    ///
    /// # 返回
    /// 返回当前账号拥有的开放单据审批任务页。
    ///
    /// # 错误
    /// WorkItem Repository 查询失败时返回错误。
    ///
    /// # 关键业务约束
    /// 任务类型、开放状态、责任人与可选单据类型全部由 Repository 固定查询封装。
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

    /// 本人开放任务的中央查询、完整性检查和交付沿同一快照。
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

    /// 本人查询的业务条件只收窄中央实例凭证，不另行选择授权规则。
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
                    return Err(Error::ValidationError("mine cursor sort_time 不能为负数".to_string()));
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

    /// 批量装载 Mine 页的 execution、instance、summary 与 snapshot，并按原任务
    /// 顺序重建实例行。任一身份链漂移时整页失败关闭，禁止静默丢行后伪造 total。
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
    /// # 参数
    /// `actor` 为当前账号，`query` 仅允许 started，`executor` 为调用方事务。
    /// # 返回
    /// 返回完整授权后的候选页，用于跨域内容检索。
    /// # 错误
    /// 非 started、无效账号、权限或仓储失败时拒绝。
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
        let type_scopes = self.runtime_read_type_scopes(query)?;
        let filter = instance_list_filter(actor, query)?;
        let scope = ApprovalRuntimeReadScope::Started {
            process_kinds: type_scopes.iter().map(|scope| scope.process_kind).collect(),
            submitted_by: actor.id().to_owned(),
        };
        let access =
            self.authorization.query(actor, &internal_permissions::APPROVAL_STARTED_LIST, executor).await?;
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

    /// 查询本人发起或管理范围内的审批实例。
    ///
    /// # 参数
    /// * `actor` - 当前已认证账号
    /// * `query` - 已规范化查询，可含字面量检索
    ///
    /// # 返回
    /// 所有视图均返回数据库在完整授权过滤后计算的分页和总数。
    ///
    /// # 错误
    /// 单据类型未登记或仓储失败时返回错误。
    ///
    /// # 关键业务约束
    /// 检索必须在 MongoDB 内施加。不得先取当前页再内存过滤。
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

    /// 在同一事务中形成类型范围、完整授权总数与稳定页面。
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

    /// 所有列表在同一读取阶段编译固定规则，并由数据库先过滤再分页计数。
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

    /// 返回业务已接入的流程种类；账号资格、同角色要求和来源范围统一由 query 判断。
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

/// 返回政策矩阵中必须接入审批运行时的固定单据类型。
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
    use super::{MAX_AUTHORIZED_INSTANCES, ensure_runtime_result_limit};
    use crate::error::Error;

    #[test]
    fn runtime_authorized_limit_preserves_existing_failure() {
        assert!(ensure_runtime_result_limit(MAX_AUTHORIZED_INSTANCES).is_ok());
        assert!(matches!(ensure_runtime_result_limit(MAX_AUTHORIZED_INSTANCES + 1),
            Err(Error::ValidationError(message))
                if message == "审批查询授权结果超过 20000 条，请增加类型或业务筛选"));
    }
}
