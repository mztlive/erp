//! 运行实例 Mine / Started / Managed 列表查询。

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

use application_core::AuditActor;
use persistence_core::{Executor, NoTransaction, Transactional};

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
    ApprovalRuntimeReadRepository, ApprovalRuntimeReadRow, ApprovalRuntimeReadScope,
    ApprovalRuntimeReadTypeScope,
};
use crate::repository::bpm::{ApprovalInstanceListCursor, ApprovalInstanceListFilter};
use crate::repository::prelude::*;
use crate::repository::{ApprovalIntegrationExt, BpmExt, WorkItemExt};
use crate::service::approval::approval_document_read_scope_with_executor;
use crate::service::approval::business_adapter::adapter_spec_of;
use crate::service::approval::policy::{ALL_DOCUMENT_TYPES, DocumentApprovalPolicy, policy_of};
use crate::service::approval::process_kind::process_kind_of;
use crate::service::approval::scope::ApprovalManagementScope;

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
        let page = self
            .db
            .work_items()
            .page_open_document_approval_owned_by(
                actor.id(),
                document_type.map(|document_type| document_type.as_str()),
                query.query.as_deref(),
                cursor,
                query.limit,
                &mut NoTransaction,
            )
            .await?;
        ensure_mine_page_integrity(page.integrity_conflicts.len())?;
        let items = self.hydrate_mine_items(actor, page.items).await?;
        let next_cursor = if page.has_more {
            page.next_cursor.map(|(sort_time, id)| RuntimeInstanceListCursor { sort_time, id })
        } else {
            None
        };
        Ok(RuntimeInstanceListPage { items, total: page.total, next_cursor })
    }

    /// 批量装载 Mine 页的 execution、instance、summary 与 snapshot，并按原任务
    /// 顺序重建实例行。任一身份链漂移时整页失败关闭，禁止静默丢行后伪造 total。
    async fn hydrate_mine_items(
        &self,
        actor: &AuditActor,
        tasks: Vec<WorkItem>,
    ) -> Result<Vec<RuntimeInstanceListItem>> {
        let execution_ids = mine_execution_ids(&tasks)?;
        let executions =
            self.db.bpm_workflow().list_executions_by_ids(&execution_ids, &mut NoTransaction).await?;
        let execution_by_id = unique_by_id(executions, |execution| execution.base.id.clone())?;

        let instance_ids = mine_instance_ids(&execution_ids, &execution_by_id)?;
        let summaries =
            self.db.bpm_workflow().list_instance_summaries_by_ids(&instance_ids, &mut NoTransaction).await?;
        let summary_by_id = unique_by_id(summaries, |summary| summary.id.clone())?;
        let instance_id_strings = instance_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        let snapshots = self
            .db
            .approval_subject_snapshots()
            .find_by_process_instance_ids(&instance_id_strings, &mut NoTransaction)
            .await?;
        let snapshot_by_instance =
            unique_by_id(snapshots, |snapshot| snapshot.approval_process_instance_id.to_string())?;

        tasks
            .into_iter()
            .map(|task| {
                let execution_id = task.approval_node_execution_id.as_ref().ok_or_else(hidden_not_found)?;
                let execution = execution_by_id.get(execution_id.as_ref()).ok_or_else(hidden_not_found)?;
                let summary =
                    summary_by_id.get(execution.process_instance_id.as_ref()).ok_or_else(hidden_not_found)?;
                let snapshot = snapshot_by_instance.get(summary.id.as_str());
                if !mine_runtime_chain_matches(&task, execution, summary, snapshot, actor.id())? {
                    return Err(hidden_not_found());
                }
                item_from_summary(summary.clone(), snapshot)
            })
            .collect()
    }

    /// 查询本人发起或管理范围内的审批实例。
    ///
    /// # 参数
    /// * `actor` - 当前已认证账号
    /// * `query` - 已规范化查询，可含字面量检索
    ///
    /// # 返回
    /// 返回 MongoDB 联合不可变快照完成授权过滤、检索、计数与分页后的实例页。
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

    async fn list_scoped_instances(
        &self,
        actor: &AuditActor,
        query: &RuntimeInstanceListQuery,
        executor: &mut dyn Executor,
    ) -> Result<RuntimeInstanceListPage> {
        let type_scopes = self.runtime_read_type_scopes(actor, query, executor).await?;
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
        filter.authorized_instance_ids =
            Some(self.authorized_instance_ids(actor, query, &filter, &scope, executor).await?);
        let mut page = ApprovalRuntimeReadRepository::new(&self.db).search(&filter, &scope, executor).await?;
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
        Ok(RuntimeInstanceListPage { items, total: page.total, next_cursor })
    }

    /// 候选按稳定游标分批读取；当前对象授权完成后才交 Repository 计数与分页。
    async fn authorized_instance_ids(
        &self,
        actor: &AuditActor,
        query: &RuntimeInstanceListQuery,
        filter: &ApprovalInstanceListFilter,
        scope: &ApprovalRuntimeReadScope,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let mut scan = filter.clone();
        scan.cursor = None;
        scan.limit = 100;
        let mut allowed = Vec::new();
        let mut read_scopes = HashMap::new();
        loop {
            let page = ApprovalRuntimeReadRepository::new(&self.db).search(&scan, scope, executor).await?;
            if page.items.is_empty() {
                break;
            }
            self.append_authorized_batch(actor, query, &page.items, &mut allowed, &mut read_scopes, executor)
                .await?;
            let last = page.items.last().expect("nonempty batch");
            let cursor = cursor_from_summary(scan.view, &last.instance);
            scan.cursor = Some(ApprovalInstanceListCursor { sort_time: cursor.sort_time, id: cursor.id });
        }
        Ok(allowed)
    }

    async fn append_authorized_batch(
        &self,
        actor: &AuditActor,
        query: &RuntimeInstanceListQuery,
        items: &[ApprovalRuntimeReadRow],
        allowed: &mut Vec<String>,
        read_scopes: &mut HashMap<DocumentType, ApprovalManagementScope>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let keys = items
            .iter()
            .filter_map(|row| row.snapshot.as_ref())
            .map(|snapshot| (snapshot.document_type, snapshot.business_object_id.clone()))
            .collect::<HashSet<_>>();
        let objects = self.auth.approval_scope_objects(&keys, executor).await?;
        let sources = objects.values().filter_map(|object| object.order_source.clone()).collect();
        let readable = self.auth.readable_order_sources(actor, &sources, executor).await?;
        for row in items {
            let Some(snapshot) = &row.snapshot else {
                continue;
            };
            let Some(object) = objects.get(&(snapshot.document_type, snapshot.business_object_id.clone()))
            else {
                continue;
            };
            if object.order_source.as_ref().is_some_and(|source| !readable.contains(source)) {
                continue;
            }
            if query.view != RuntimeInstanceListView::Started {
                if let Entry::Vacant(entry) = read_scopes.entry(snapshot.document_type) {
                    let access = approval_document_read_scope_with_executor(
                        &self.auth,
                        actor,
                        snapshot.document_type,
                        executor,
                    )
                    .await?;
                    entry.insert(access);
                }
                if !read_scopes[&snapshot.document_type].covers_object(object) {
                    continue;
                }
            }
            if allowed.len() >= 20_000 {
                return Err(Error::ValidationError(
                    "审批查询授权结果超过 20000 条，请增加类型或业务筛选".into(),
                ));
            }
            allowed.push(row.instance.id.clone());
        }
        Ok(())
    }

    /// 计算 Started 或管理视图可进入 Repository 的固定流程种类。
    ///
    /// # 参数
    /// * `actor` - 当前有效账号
    /// * `query` - 已规范化视图与可选固定单据类型
    ///
    /// # 返回
    /// Started 返回全部必须审批类型或请求类型；Managed/Blocked 返回当前账号具备
    /// `runtime_admin_permission` 的类型交集。
    ///
    /// # 错误
    /// 单据类型、政策或 RBAC 读取失败时返回错误。
    ///
    /// # 关键业务约束
    /// Service 只把已证明的类型集合交给 Repository；空集合固定形成空页。
    async fn runtime_read_type_scopes(
        &self,
        actor: &AuditActor,
        query: &RuntimeInstanceListQuery,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalRuntimeReadTypeScope>> {
        let requested = query.document_type.as_deref().map(parse_document_type).transpose()?;
        let mut allowed = if query.view == RuntimeInstanceListView::Started {
            process_required_document_types()?
        } else {
            crate::service::approval::scope::definition_management_visibility_with_executor(
                &self.auth, actor, executor,
            )
            .await?
            .runtime_admin_types()
            .to_vec()
        };
        if let Some(requested) = requested {
            allowed.retain(|document_type| *document_type == requested);
        }
        let mut scopes = Vec::new();
        for document_type in allowed {
            adapter_spec_of(document_type)?;
            let organization_ids = if query.view == RuntimeInstanceListView::Started {
                None
            } else {
                let scope =
                    approval_document_read_scope_with_executor(&self.auth, actor, document_type, executor)
                        .await?;
                if scope.is_empty() {
                    continue;
                }
                None
            };
            scopes.push(ApprovalRuntimeReadTypeScope {
                process_kind: process_kind_of(document_type),
                organization_ids,
            });
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
