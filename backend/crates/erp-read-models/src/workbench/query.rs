//! 责任队列列表与单条详情查询。

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::num::NonZeroU32;

use erp_workflow::entity::work_item::{QueueContextField, QueueContextIdentity, WorkItem};
use erp_workflow::BpmExt;
use erp_workflow::WorkItemExt;
use persistence_core::NoTransaction;
use validator::Validate;

use crate::errors::{Error, Result};
use application_core::AuditActor;

use super::access::{authorized_fields, authorized_item_fields, detail_scope, ActorAccess};
use super::dto;
use super::facts::object_policy;
use super::stats::apply_due_filter;
use super::{
    ProcessingBlockerView, WorkItemAllowedAction, WorkItemDueFilter, WorkItemFilter, WorkItemListParams,
    WorkItemPageView, WorkItemScope, WorkItemView,
};

pub(super) const AUTHORIZED_SCAN_BATCH_SIZE: NonZeroU32 = NonZeroU32::new(100).expect("批次大小必须非零");

struct FocusedQueueContext<'a> {
    page_size: u32,
    access: &'a ActorAccess,
}

/// Cross-domain workbench query service.
#[derive(Clone)]
pub struct WorkbenchReadService<A> {
    /// MongoDB handle used for work-item and brief reads.
    pub db: mongodb::Database,
    /// Injected authorization port.
    pub auth: A,
}

impl<A: erp_workflow::WorkflowAuthorizationPort> WorkbenchReadService<A> {
    pub(super) fn facts_reader(&self) -> super::authority::WorkItemFactsReader {
        super::authority::WorkItemFactsReader::new(self.db.clone())
    }

    /// Create a workbench read service.
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    /// * `auth` - 注入的授权 Port
    ///
    /// # 返回
    /// 返回绑定当前授权源的工作台读模型。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: mongodb::Database, auth: A) -> Self {
        Self { db, auth }
    }
}

impl<A: erp_workflow::WorkflowAuthorizationPort + Clone + Send + Sync + 'static> WorkbenchReadService<A> {
    /// 查询服务端授权过滤后的责任队列。
    ///
    /// # 参数
    /// * `params` - 固定 scope 与业务筛选
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回带稳定队列上下文、任务版本和允许动作的分页投影。
    ///
    /// # 错误
    /// 查询参数非法、managed 未授权或授权事实无法读取时返回错误。
    pub async fn work_item_list(
        &self,
        params: WorkItemListParams,
        actor: AuditActor,
    ) -> Result<WorkItemPageView> {
        params.validate()?;
        let query = params.normalized()?;
        let access = self.actor_access(&actor).await?;
        let queue_context_id = queue_context_id(actor.id(), &query, &access);
        ensure_queue_context(&query.queue_context_id, &queue_context_id)?;
        let mut filter = self.scope_filter(&query, &actor, &access)?;
        apply_due_filter(&mut filter, query.due)?;
        let authorized_page = self
            .authorized_page_fields(&filter, query.page, query.page_size, &access)
            .await?;
        let fields = self
            .focused_fields(
                authorized_page.items,
                query.current_work_item_id.as_deref(),
                &filter,
                FocusedQueueContext {
                    page_size: query.page_size,
                    access: &access,
                },
            )
            .await?;
        let items = self
            .project_fields(fields, query.scope, &actor, &access, &queue_context_id)
            .await?;
        Ok(WorkItemPageView {
            items,
            total: authorized_page.total,
            page: query.page,
            page_size: query.page_size,
            queue_context_id,
        })
    }

    /// 在完整授权过滤内把焦点任务移到当前页首。
    ///
    /// 焦点不在当前页时使用同一个 Repository filter 查找；不存在或
    /// 不可见都返回 NotFound，绝不退回无过滤 ID 查询。
    async fn focused_fields(
        &self,
        mut fields: Vec<dto::WorkItemFields>,
        current_work_item_id: Option<&str>,
        filter: &WorkItemFilter,
        context: FocusedQueueContext<'_>,
    ) -> Result<Vec<dto::WorkItemFields>> {
        let Some(current_id) = current_work_item_id else {
            return Ok(fields);
        };
        if let Some(index) = fields.iter().position(|item| item.id == current_id) {
            let current = fields.remove(index);
            fields.insert(0, current);
            return Ok(fields);
        }
        let current = self
            .db
            .work_items()
            .find_visible_by_id(current_id, filter, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("当前焦点任务不在授权队列中".to_string()))?;
        let current = self
            .authorized_fields_for_items(vec![current], context.access)
            .await?;
        let Some(current) = current.into_iter().next() else {
            return Err(Error::NotFound("当前焦点任务不在授权队列中".to_string()));
        };
        fields.insert(0, current);
        fields.truncate(context.page_size as usize);
        Ok(fields)
    }

    /// 扫描全部仓储候选，逐批授权后仅保留请求页。
    async fn authorized_page_fields(
        &self,
        filter: &WorkItemFilter,
        page: u64,
        page_size: u32,
        access: &ActorAccess,
    ) -> Result<AuthorizedPage<dto::WorkItemFields>> {
        let mut collector = AuthorizedPageCollector::new(page, page_size)?;
        let mut candidate_offset = 0_u64;
        loop {
            let rows = self.candidate_batch(filter, candidate_offset).await?;
            let candidate_count = rows.len();
            if candidate_count == 0 {
                break;
            }
            let facts = self.object_facts_for_rows(&rows).await?;
            let fields = authorized_fields(rows, access, &facts);
            collector.extend(fields);
            candidate_offset = next_candidate_offset(candidate_offset, candidate_count)?;
            if candidate_count < AUTHORIZED_SCAN_BATCH_SIZE.get() as usize {
                break;
            }
        }
        Ok(collector.finish())
    }

    /// 读取一个固定大小候选批次，不执行候选总数计数。
    pub(super) async fn candidate_batch(
        &self,
        filter: &WorkItemFilter,
        offset: u64,
    ) -> Result<Vec<erp_workflow::WorkItemRow>> {
        self.db
            .work_items()
            .scan_work_item_batch(filter, offset, AUTHORIZED_SCAN_BATCH_SIZE, &mut NoTransaction)
            .await
            .map_err(Error::from)
    }

    /// 为完整任务列表重新读取对象并执行参与权过滤。
    pub(super) async fn authorized_fields_for_items(
        &self,
        items: Vec<WorkItem>,
        access: &ActorAccess,
    ) -> Result<Vec<dto::WorkItemFields>> {
        let keys = items
            .iter()
            .filter_map(|item| {
                object_policy(item.work_item_type, &item.business_object_type)
                    .map(|policy| (policy.object_kind, item.business_object_id.clone()))
            })
            .collect::<HashSet<_>>();
        let facts = self.load_object_facts(&keys, &mut NoTransaction).await?;
        Ok(items
            .into_iter()
            .filter_map(|item| authorized_item_fields(item, access, &facts))
            .collect())
    }

    /// 为已授权任务逐条计算审批阻断与允许动作。
    async fn project_fields(
        &self,
        fields: Vec<dto::WorkItemFields>,
        scope: WorkItemScope,
        actor: &AuditActor,
        actor_access: &ActorAccess,
        queue_context_id: &str,
    ) -> Result<Vec<WorkItemView>> {
        let mut items = Vec::with_capacity(fields.len());
        for fields in fields {
            let access = self.view_access(&fields, scope, actor, actor_access)?;
            items.push(
                WorkItemView::from_fields(fields, queue_context_id.to_string())?.with_access(
                    access.processing_state,
                    access.processing_blocker,
                    access.allowed_actions,
                    access.action_blockers,
                ),
            );
        }
        self.apply_party_names(&mut items).await?;
        self.apply_approval_contexts(&mut items).await?;
        Ok(items)
    }

    /// 批量补齐审批任务的当前节点和最近驳回事实。
    ///
    /// # 参数
    /// * `items` - 已通过工作项和业务对象授权的安全投影
    ///
    /// # 返回
    /// 无审批节点的任务保持不变；审批任务获得与节点执行严格绑定的运行上下文。
    ///
    /// # 错误
    /// BPM 节点执行或实例有界投影查询失败时返回错误。
    ///
    /// # 关键业务约束
    /// 列表固定执行两次批量查询，不得逐任务读取审批历史；最近驳回只取实例有界投影。
    pub(super) async fn apply_approval_contexts(&self, items: &mut [WorkItemView]) -> Result<()> {
        let execution_ids = items
            .iter()
            .filter_map(|item| item.approval_node_execution_id.as_deref())
            .map(bpm::ids::ApprovalNodeExecutionId::new)
            .collect::<Vec<_>>();
        if execution_ids.is_empty() {
            return Ok(());
        }
        let executions = self
            .db
            .bpm_workflow()
            .list_executions_by_ids(&execution_ids, &mut NoTransaction)
            .await?;
        let instance_ids = executions
            .iter()
            .map(|execution| execution.process_instance_id.clone())
            .collect::<Vec<_>>();
        let summaries = self
            .db
            .bpm_workflow()
            .list_instance_summaries_by_ids(&instance_ids, &mut NoTransaction)
            .await?;
        let executions = executions
            .into_iter()
            .map(|execution| (execution.base.id.clone(), execution))
            .collect::<HashMap<_, _>>();
        let summaries = summaries
            .into_iter()
            .map(|summary| (summary.id.clone(), summary))
            .collect::<HashMap<_, _>>();
        for item in items {
            let Some(execution_id) = item.approval_node_execution_id.as_deref() else {
                continue;
            };
            let Some(execution) = executions.get(execution_id) else {
                fail_closed_missing_approval_context(item);
                continue;
            };
            let Some(summary) = summaries.get(execution.process_instance_id.as_ref()) else {
                fail_closed_missing_approval_context(item);
                continue;
            };
            item.set_approval_context(approval_context_view(execution, summary));
        }
        Ok(())
    }

    /// 查询当前用户有权查看的单条任务。
    ///
    /// # 错误
    /// 任务不存在或当前用户不在任一安全责任范围时返回错误。
    pub async fn work_item_detail(self, id: String, actor: AuditActor) -> Result<WorkItemView> {
        let item = self.load(id.clone()).await?;
        let item_id = item.base.id.clone();
        let access = self.actor_access(&actor).await?;
        let scope = detail_scope(&item, actor.id(), &access)?;
        let fields = self.authorized_fields_for_items(vec![item], &access).await?;
        let fields = fields
            .into_iter()
            .next()
            .ok_or_else(|| Error::NotFound("任务或业务对象不可见".to_string()))?;
        let queue_context_id = single_item_context_id(actor.id(), &item_id);
        let view_access = self.view_access(&fields, scope, &actor, &access)?;
        let mut view = WorkItemView::from_fields(fields, queue_context_id)?.with_access(
            view_access.processing_state,
            view_access.processing_blocker,
            view_access.allowed_actions,
            view_access.action_blockers,
        );
        self.apply_party_names(std::slice::from_mut(&mut view)).await?;
        self.apply_approval_contexts(std::slice::from_mut(&mut view))
            .await?;
        Ok(view)
    }

    /// 按稳定 ID 加载未删除工作项。
    ///
    /// # 参数
    /// * `id` - 工作项稳定 ID
    ///
    /// # 返回
    /// 返回当前工作项实体。
    ///
    /// # 错误
    /// 工作项不存在或仓储查询失败时返回错误。
    pub(super) fn load(&self, id: String) -> impl Future<Output = Result<WorkItem>> + Send + 'static {
        let db = self.db.clone();
        async move {
            db.work_items()
                .find_work_item(&id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("任务不存在".to_string()))
        }
    }
}

fn approval_context_view(
    execution: &bpm::model::ApprovalNodeExecution,
    summary: &erp_workflow::repository::bpm::ApprovalInstanceSummary,
) -> dto::WorkItemApprovalContextView {
    dto::WorkItemApprovalContextView {
        instance_id: summary.id.clone(),
        status: summary.status.as_str().to_string(),
        current_round_no: execution.round_no,
        current_node_label: execution.node_name.clone(),
        current_assignee_label: non_empty_text(&execution.assignee_name_snapshot),
        latest_rejection_reason: summary
            .latest_rejection_summary
            .as_deref()
            .and_then(non_empty_text),
        process_version: Some(summary.definition_version),
    }
}

/// 审批运行上下文缺失时移除决定动作并追加稳定阻断信息。
fn fail_closed_missing_approval_context(item: &mut WorkItemView) {
    if !remove_approval_decision_actions(&mut item.allowed_actions) {
        return;
    }
    item.action_blockers.push(ProcessingBlockerView {
        code: "APPROVAL_CONTEXT_MISSING".to_string(),
        message: "审批运行信息暂不可用，请刷新；仍未恢复时联系管理员修复审批数据".to_string(),
    });
}

/// 从动作集合中移除审批决定并返回是否发生移除。
pub(super) fn remove_approval_decision_actions(actions: &mut Vec<WorkItemAllowedAction>) -> bool {
    let before = actions.len();
    actions.retain(|action| {
        !matches!(
            action,
            WorkItemAllowedAction::Approve | WorkItemAllowedAction::Reject
        )
    });
    actions.len() != before
}

/// 返回去除首尾空白后的非空展示文本。
fn non_empty_text(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn queue_context_id(actor_id: &str, query: &dto::WorkItemListQuery, access: &ActorAccess) -> String {
    QueueContextIdentity::new(
        "work-items",
        [
            QueueContextField::scalar("actor", actor_id),
            QueueContextField::scalar("scope", query.scope.as_str()),
            QueueContextField::set(
                "types",
                query
                    .work_item_types
                    .iter()
                    .map(|value| value.as_str().to_string()),
            ),
            QueueContextField::set(
                "statuses",
                query.statuses.iter().map(|value| value.as_str().to_string()),
            ),
            QueueContextField::optional("due", query.due.map(WorkItemDueFilter::as_str)),
            QueueContextField::set(
                "priorities",
                query.priorities.iter().map(|value| value.as_str().to_string()),
            ),
            QueueContextField::optional("query", query.query.as_deref()),
            QueueContextField::scalar("sort", query.sort_by),
            QueueContextField::scalar("ascending", query.sort_ascending.to_string()),
            QueueContextField::set(
                "responsibilities",
                access.responsibility_scopes.iter().map(|(role, organization)| {
                    QueueContextField::tuple([
                        role.clone(),
                        organization.clone().unwrap_or_else(|| "*".to_string()),
                    ])
                }),
            ),
            QueueContextField::set("organizations", access.organization_ids.clone()),
            QueueContextField::scalar("can_manage", access.can_manage.to_string()),
        ],
    )
    .into_string()
}

pub(super) fn single_item_context_id(actor_id: &str, work_item_id: &str) -> String {
    QueueContextIdentity::new(
        "work-item-single",
        [
            QueueContextField::scalar("actor", actor_id),
            QueueContextField::scalar("work_item", work_item_id),
        ],
    )
    .into_string()
}

pub(super) fn ensure_queue_context(provided: &Option<String>, expected: &str) -> Result<()> {
    if provided.as_deref().is_none_or(|provided| provided == expected) {
        return Ok(());
    }
    Err(Error::ConflictError("队列上下文已变化，请刷新队列".to_string()))
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct AuthorizedPage<T> {
    pub(super) items: Vec<T>,
    pub(super) total: i64,
}

/// 有界累加授权行：统计完整授权总数，仅保留请求页。
pub(super) struct AuthorizedPageCollector<T> {
    pub(super) start: u64,
    pub(super) end: u64,
    pub(super) total: u64,
    pub(super) items: Vec<T>,
}

impl<T> AuthorizedPageCollector<T> {
    pub(super) fn new(page: u64, page_size: u32) -> Result<Self> {
        let start = page
            .max(1)
            .checked_sub(1)
            .and_then(|page_index| page_index.checked_mul(u64::from(page_size)))
            .ok_or_else(|| Error::ValidationError("分页偏移超出支持范围".to_string()))?;
        let end = start
            .checked_add(u64::from(page_size))
            .ok_or_else(|| Error::ValidationError("分页偏移超出支持范围".to_string()))?;
        Ok(Self {
            start,
            end,
            total: 0,
            items: Vec::with_capacity(page_size as usize),
        })
    }

    pub(super) fn extend(&mut self, authorized: impl IntoIterator<Item = T>) {
        for item in authorized {
            let position = self.total;
            self.total = self.total.saturating_add(1);
            if position >= self.start && position < self.end {
                self.items.push(item);
            }
        }
    }

    pub(super) fn finish(self) -> AuthorizedPage<T> {
        AuthorizedPage {
            items: self.items,
            total: i64::try_from(self.total).unwrap_or(i64::MAX),
        }
    }
}

pub(super) fn next_candidate_offset(current: u64, batch_len: usize) -> Result<u64> {
    let batch_len = u64::try_from(batch_len)
        .map_err(|_| Error::Internal("责任队列候选批次大小超出支持范围".to_string()))?;
    current
        .checked_add(batch_len)
        .ok_or_else(|| Error::Internal("责任队列候选偏移溢出".to_string()))
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_queue_context, next_candidate_offset, remove_approval_decision_actions, AuthorizedPage,
        AuthorizedPageCollector,
    };
    use super::{WorkItemAllowedAction, AUTHORIZED_SCAN_BATCH_SIZE};

    #[test]
    fn authorized_pagination_slices_after_authorization() {
        let mut collector = AuthorizedPageCollector::new(2, 2).unwrap();

        collector.extend(["authorized-1"]);
        collector.extend(["authorized-2", "authorized-3", "authorized-4"]);

        assert_eq!(
            collector.finish(),
            AuthorizedPage {
                items: vec!["authorized-3", "authorized-4"],
                total: 4,
            }
        );
    }

    #[test]
    fn authorized_pagination_reaches_later_batch_and_counts_full_total() {
        let mut collector = AuthorizedPageCollector::new(1, 2).unwrap();
        collector.extend(Vec::<&str>::new());
        collector.extend(["allowed-0", "allowed-1", "allowed-2"]);
        let page = collector.finish();
        assert_eq!(page.items, vec!["allowed-0", "allowed-1"]);
        assert_eq!(page.total, 3);
        assert_eq!(AUTHORIZED_SCAN_BATCH_SIZE.get(), 100);
    }

    #[test]
    fn queue_context_mismatch_fails_closed() {
        assert!(ensure_queue_context(&None, "ctx-1").is_ok());
        assert!(ensure_queue_context(&Some("ctx-1".to_string()), "ctx-1").is_ok());
        assert!(ensure_queue_context(&Some("ctx-2".to_string()), "ctx-1").is_err());
    }

    #[test]
    fn approval_decision_actions_are_removed_fail_closed() {
        let mut actions = vec![
            WorkItemAllowedAction::View,
            WorkItemAllowedAction::Approve,
            WorkItemAllowedAction::Reject,
            WorkItemAllowedAction::Process,
        ];
        assert!(remove_approval_decision_actions(&mut actions));
        assert_eq!(
            actions,
            vec![WorkItemAllowedAction::View, WorkItemAllowedAction::Process]
        );
        assert!(!remove_approval_decision_actions(&mut actions));
    }

    #[test]
    fn candidate_offset_advances_by_batch_len() {
        assert_eq!(next_candidate_offset(0, 100).unwrap(), 100);
        assert!(next_candidate_offset(u64::MAX, 1).is_err());
    }
}
