//! 运行实例 Mine / Started / Managed 列表查询。

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
    ApprovalRuntimeReadPage, ApprovalRuntimeReadRepository, ApprovalRuntimeReadRow, ApprovalRuntimeReadScope,
    ApprovalRuntimeReadTypeScope,
};
use crate::repository::bpm::{
    ApprovalInstanceListCursor, ApprovalInstanceListFilter, ApprovalInstanceListView, ApprovalInstanceSummary,
};
use crate::repository::prelude::*;
use crate::repository::{ApprovalIntegrationExt, BpmExt, WorkItemExt};
use crate::service::approval::approval_participant_permissions_with_executor;
use crate::service::approval::business_adapter::adapter_spec_of;
use crate::service::approval::policy::{ALL_DOCUMENT_TYPES, DocumentApprovalPolicy, policy_of};
use crate::service::approval::process_kind::process_kind_of;

/// 保持现有授权结果规模上限；流式读取不会保留完整 ID 集合。
const MAX_AUTHORIZED_INSTANCES: u64 = 20_000;

/// 仅累计完整授权数量并保留当前页与一条下一页探针。
#[derive(Default)]
struct AuthorizedRuntimePage {
    rows: Vec<ApprovalRuntimeReadRow>,
    total: u64,
}

impl AuthorizedRuntimePage {
    /// 已证明来源可读的行计入总数，只有游标后的有界页行进入内存。
    fn append(&mut self, row: ApprovalRuntimeReadRow, filter: &ApprovalInstanceListFilter) -> Result<()> {
        self.total += 1;
        ensure_runtime_result_limit(self.total)?;
        if self.rows.len() < usize::try_from(filter.limit).unwrap_or(usize::MAX)
            && runtime_row_after_cursor(&row.instance, filter.view, filter.cursor.as_ref())
        {
            self.rows.push(row);
        }
        Ok(())
    }

    /// 交付完整授权总数与有界页面，不需要再次查询授权 ID 集合。
    fn into_page(self) -> ApprovalRuntimeReadPage {
        ApprovalRuntimeReadPage { items: self.rows, total: self.total }
    }
}

/// 保留原有超限拒绝错误，Started 与 Managed 使用相同结果上限。
fn ensure_runtime_result_limit(total: u64) -> Result<()> {
    if total > MAX_AUTHORIZED_INSTANCES {
        return Err(Error::ValidationError("审批查询授权结果超过 20000 条，请增加类型或业务筛选".into()));
    }
    Ok(())
}

/// 与 Repository 的时间降序、ID 降序游标及 MongoDB 数值比较口径一致。
fn runtime_row_after_cursor(
    row: &ApprovalInstanceSummary,
    view: ApprovalInstanceListView,
    cursor: Option<&ApprovalInstanceListCursor>,
) -> bool {
    let Some(cursor) = cursor else {
        return true;
    };
    let sort_time = match view {
        ApprovalInstanceListView::Started => Some(row.started_at),
        ApprovalInstanceListView::Blocked => row.blocked_at,
        ApprovalInstanceListView::Managed => i64::try_from(row.updated_at).ok(),
    };
    sort_time.is_some_and(|time| time < cursor.sort_time || (time == cursor.sort_time && row.id < cursor.id))
}

/// 原始候选末行必须具有真实排序时间，且下一批游标严格向后推进。
fn checked_scan_cursor(
    view: ApprovalInstanceListView,
    last: &ApprovalInstanceSummary,
    previous: Option<&ApprovalInstanceListCursor>,
) -> Result<ApprovalInstanceListCursor> {
    let sort_time = match view {
        ApprovalInstanceListView::Started => Some(last.started_at),
        ApprovalInstanceListView::Blocked => last.blocked_at,
        ApprovalInstanceListView::Managed => i64::try_from(last.updated_at).ok(),
    }
    .ok_or_else(|| Error::version_conflict("审批候选"))?;
    let cursor = ApprovalInstanceListCursor { sort_time, id: last.id.clone() };
    if previous.is_some_and(|previous| {
        cursor.sort_time > previous.sort_time
            || (cursor.sort_time == previous.sort_time && cursor.id >= previous.id)
    }) {
        return Err(Error::version_conflict("审批候选"));
    }
    Ok(cursor)
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
        if !approval_participant_permissions_with_executor(&self.auth, actor, &mut NoTransaction).await? {
            return Ok(RuntimeInstanceListPage { items: Vec::new(), total: 0, next_cursor: None });
        }
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
        let mut summary_by_id = unique_by_id(summaries, |summary| summary.id.clone())?;
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

    /// 查询本人发起或管理范围内的审批实例。
    ///
    /// # 参数
    /// * `actor` - 当前已认证账号
    /// * `query` - 已规范化查询，可含字面量检索
    ///
    /// # 返回
    /// Started 返回数据库分页及计数；管理视图返回有界候选扫描形成的真实来源授权页及总数。
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
        let mut page = if query.view == RuntimeInstanceListView::Started {
            ApprovalRuntimeReadRepository::new(&self.db).search(&filter, &scope, executor).await?
        } else {
            self.authorized_runtime_page(actor, &filter, &scope, executor).await?
        };
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
        Ok(RuntimeInstanceListPage { items, total: page.total, next_cursor })
    }

    /// 有界扫描不计算候选总数，只保留授权页；最终总数覆盖全部真实来源授权行。
    async fn authorized_runtime_page(
        &self,
        actor: &AuditActor,
        filter: &ApprovalInstanceListFilter,
        scope: &ApprovalRuntimeReadScope,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalRuntimeReadPage> {
        let mut scan = filter.clone();
        scan.cursor = None;
        scan.limit = 100;
        let mut page = AuthorizedRuntimePage::default();
        loop {
            let (items, last) =
                ApprovalRuntimeReadRepository::new(&self.db).scan(&scan, scope, executor).await?;
            let Some(last) = last else {
                break;
            };
            self.append_authorized_batch(actor, items, filter, &mut page, executor).await?;
            scan.cursor = Some(checked_scan_cursor(scan.view, &last, scan.cursor.as_ref())?);
        }
        Ok(page.into_page())
    }

    /// 每个来源在当前事务中独立证明，缺失快照或不可读来源均不计入页面及总数。
    async fn append_authorized_batch(
        &self,
        actor: &AuditActor,
        items: Vec<ApprovalRuntimeReadRow>,
        filter: &ApprovalInstanceListFilter,
        page: &mut AuthorizedRuntimePage,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        for row in items {
            let Some(snapshot) = &row.snapshot else {
                continue;
            };
            if !self
                .auth
                .approval_source_readable(
                    actor,
                    snapshot.document_type,
                    &snapshot.business_object_id,
                    executor,
                )
                .await?
            {
                continue;
            }
            page.append(row, filter)?;
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
            let organization_ids = None;
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        AuthorizedRuntimePage, MAX_AUTHORIZED_INSTANCES, checked_scan_cursor, ensure_runtime_result_limit,
        runtime_row_after_cursor,
    };
    use crate::error::Error;
    use crate::repository::approval_integration::ApprovalRuntimeReadRow;
    use crate::repository::bpm::{
        ApprovalInstanceListCursor, ApprovalInstanceListFilter, ApprovalInstanceListView,
        ApprovalInstanceSummary,
    };

    /// 构造不收窄候选的管理视图分页参数。
    fn filter(limit: u32) -> ApprovalInstanceListFilter {
        ApprovalInstanceListFilter {
            view: ApprovalInstanceListView::Managed,
            process_kind: None,
            status: None,
            started_by: None,
            subject_kind: None,
            authorized_instance_ids: None,
            subject_ids: None,
            text_query: None,
            cursor: None,
            limit,
        }
    }

    /// 构造真实摘要类型，用同一排序时间覆盖所有列表视图。
    fn summary(id: &str, sort_time: u64) -> ApprovalInstanceSummary {
        serde_json::from_value(json!({
            "id": id, "process_kind": "sales_order", "process_definition_id": "definition-1",
            "definition_version": 1, "subject": { "subject_kind": "sales_order", "subject_id": "order-1" },
            "subject_version": 1, "status": "RUNNING", "current_round_no": 1,
            "current_node_execution_id": null, "started_by": "starter-1", "started_at": sort_time,
            "blocked_at": sort_time, "version": 1, "updated_at": sort_time,
        }))
        .unwrap()
    }

    /// 构造已完成授权的页输入，冻结快照不会参与分页游标判断。
    fn row(id: &str, sort_time: u64) -> ApprovalRuntimeReadRow {
        ApprovalRuntimeReadRow { instance: summary(id, sort_time), snapshot: None }
    }

    /// 第一页保留 limit+1，之后的授权行只计数，不增加保存行数。
    #[test]
    fn authorized_page_counts_full_result_and_keeps_bounded_probe() {
        let filter = filter(3);
        let mut page = AuthorizedRuntimePage::default();
        for index in (1..=100).rev() {
            page.append(row(&format!("inst-{index:03}"), index), &filter).unwrap();
        }
        let page = page.into_page();
        assert_eq!(page.total, 100);
        assert_eq!(
            page.items.iter().map(|row| row.instance.id.as_str()).collect::<Vec<_>>(),
            ["inst-100", "inst-099", "inst-098"]
        );
    }

    /// 翻页时 total 不受游标影响，等时间按 ID 降序继续，空页仍保留完整 total。
    #[test]
    fn authorized_page_uses_stable_cursor_and_full_total_for_tail_and_empty_pages() {
        let mut filter = filter(3);
        filter.cursor = Some(ApprovalInstanceListCursor { sort_time: 10, id: "inst-b".into() });
        let mut page = AuthorizedRuntimePage::default();
        for (id, time) in [("inst-d", 11), ("inst-c", 10), ("inst-b", 10), ("inst-a", 10), ("inst-z", 9)] {
            page.append(row(id, time), &filter).unwrap();
        }
        let page = page.into_page();
        assert_eq!(page.total, 5);
        assert_eq!(
            page.items.iter().map(|row| row.instance.id.as_str()).collect::<Vec<_>>(),
            ["inst-a", "inst-z"]
        );
        filter.cursor = Some(ApprovalInstanceListCursor { sort_time: 0, id: "inst-0".into() });
        let mut empty = AuthorizedRuntimePage::default();
        empty.append(row("inst-a", 10), &filter).unwrap();
        assert!(empty.rows.is_empty());
        assert_eq!(empty.total, 1);
    }

    /// 三视图的游标使用对应时间，阻塞时间缺失及超出 i64 的更新时间不会错误进入后续页。
    #[test]
    fn runtime_cursor_keeps_view_time_and_numeric_type_boundaries() {
        let mut row = summary("inst-a", 10);
        row.started_at = 3;
        row.blocked_at = Some(7);
        let cursor = ApprovalInstanceListCursor { sort_time: 5, id: "inst-z".into() };
        assert!(runtime_row_after_cursor(&row, ApprovalInstanceListView::Started, Some(&cursor)));
        assert!(!runtime_row_after_cursor(&row, ApprovalInstanceListView::Blocked, Some(&cursor)));
        assert!(!runtime_row_after_cursor(&row, ApprovalInstanceListView::Managed, Some(&cursor)));
        row.blocked_at = None;
        row.updated_at = u64::MAX;
        assert!(!runtime_row_after_cursor(&row, ApprovalInstanceListView::Blocked, Some(&cursor)));
        assert!(!runtime_row_after_cursor(&row, ApprovalInstanceListView::Managed, Some(&cursor)));
        assert!(runtime_row_after_cursor(&row, ApprovalInstanceListView::Blocked, None));
    }

    /// 精确保持原授权结果上限：20000 成功，20001 返回原校验错误。
    #[test]
    fn runtime_authorized_limit_preserves_existing_failure() {
        assert!(ensure_runtime_result_limit(MAX_AUTHORIZED_INSTANCES).is_ok());
        assert!(matches!(ensure_runtime_result_limit(MAX_AUTHORIZED_INSTANCES + 1),
            Err(Error::ValidationError(message))
                if message == "审批查询授权结果超过 20000 条，请增加类型或业务筛选"));
        let filter = filter(2);
        let mut page = AuthorizedRuntimePage { rows: Vec::new(), total: MAX_AUTHORIZED_INSTANCES };
        assert!(page.append(row("inst-a", 1), &filter).is_err());
        assert!(page.rows.is_empty());
    }

    /// 原始批次的阻塞时间缺失必须失败关闭，不能以更新时间重扫已经经过的实例。
    #[test]
    fn runtime_scan_cursor_rejects_missing_block_time_and_nonadvancing_rows() {
        let previous = ApprovalInstanceListCursor { sort_time: 10, id: "inst-b".into() };
        let mut last = summary("inst-a", 10);
        let cursor = checked_scan_cursor(ApprovalInstanceListView::Blocked, &last, Some(&previous)).unwrap();
        assert_eq!(cursor.sort_time, 10);
        assert_eq!(cursor.id, "inst-a");
        last.blocked_at = None;
        assert!(matches!(checked_scan_cursor(ApprovalInstanceListView::Blocked, &last, None),
            Err(Error::ConflictError(message)) if message == "审批候选版本已变化，请刷新后重试"));
        for (id, time) in [("inst-b", 10), ("inst-c", 10), ("inst-a", 11)] {
            assert!(
                matches!(checked_scan_cursor(ApprovalInstanceListView::Managed, &summary(id, time), Some(&previous)),
                Err(Error::ConflictError(message)) if message == "审批候选版本已变化，请刷新后重试")
            );
        }
        last.updated_at = u64::MAX;
        assert!(checked_scan_cursor(ApprovalInstanceListView::Managed, &last, None).is_err());
    }
}
