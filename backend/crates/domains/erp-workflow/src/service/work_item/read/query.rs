//! 责任队列列表与单条详情查询。

use std::collections::HashMap;
use std::num::NonZeroU32;

use application_core::AuditActor;
use erp_identity::SharedRbacService;
use futures_util::future::BoxFuture;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::access::{QueueAccess, detail_scope};
use super::context::*;
use super::stats::apply_due_filter;
use super::{
    ProcessingBlockerView, WorkItemAllowedAction, WorkItemDueFilter, WorkItemFilter, WorkItemListParams,
    WorkItemPageView, WorkItemScope, WorkItemSearchParams, WorkItemView, WorkspaceFilters, dto,
};
use crate::dto::work_item::ViewAccess;
use crate::error::{Error, Result};
use crate::repository::prelude::*;
use crate::repository::work_item::WorkItemScan;
use crate::service::work_item::WorkItemAuthorization;
use crate::{BpmExt, WorkItemExt};

pub(super) const AUTHORIZED_SCAN_BATCH_SIZE: NonZeroU32 = NonZeroU32::new(100).expect("批次大小必须非零");

struct FocusedQueueContext<'a> {
    page_size: u32,
    access: &'a QueueAccess,
}

/// 工作项领域查询服务。
///
/// 查询入口使用固定 Future 类型，授权实现和内部查询状态机不进入调用方的类型。
#[derive(Clone)]
pub struct WorkItemReadService {
    /// MongoDB handle used for work-item reads.
    pub db: mongodb::Database,
    /// Identity 共享权限服务。
    pub authorization: WorkItemAuthorization,
}

impl WorkItemReadService {
    /// 构造任务查询服务。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    /// * `rbac` - identity 的共享 RBAC 服务
    ///
    /// # 返回
    /// 返回绑定当前授权源的任务查询服务。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: mongodb::Database, rbac: SharedRbacService) -> Self {
        let authorization = WorkItemAuthorization::new(db.clone(), rbac);
        Self { db, authorization }
    }
}

impl WorkItemReadService {
    /// 在供给影响读取中复用当前任务详情阅读资格，不另建事务。
    ///
    /// # 参数
    /// * `actor` - 当前真实内部账号。
    /// * `id` - 已读取的精确履约任务身份。
    /// * `executor` - 影响读取沿用的执行器。
    /// # 返回
    /// 任务详情授权成立时返回 `true`。任务不存在或禁止访问时返回 `false`，不返回任务或业务显示内容。
    /// # 错误
    /// 除不存在和禁止访问外，授权配置及对象读取故障继续传播。
    #[inline(never)]
    pub fn work_item_readable_with_executor<'a>(
        &'a self,
        actor: &'a AuditActor,
        id: &'a str,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<bool>> {
        Box::pin(async move {
            let result = self.authorization.require(actor, "detail", id, executor).await.map(|()| true);
            match result {
                Err(Error::NotFound(_) | Error::Forbidden(_)) => Ok(false),
                other => other,
            }
        })
    }

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
    pub fn work_item_list(
        &self,
        params: WorkItemListParams,
        actor: AuditActor,
    ) -> BoxFuture<'_, Result<WorkItemPageView>> {
        self.work_item_search(WorkItemSearchParams { base: params, filters: None }, actor)
    }

    /// 查询带结构化条件的授权任务队列。
    /// # 参数
    /// `params` 为责任范围与筛选，`actor` 为当前账号。
    /// # 返回
    /// 返回服务端筛选计数、分页及候选项。
    /// # 错误
    /// 非法条件、授权或查询失败时返回错误。
    #[inline(never)]
    pub fn work_item_search(
        &self,
        params: WorkItemSearchParams,
        actor: AuditActor,
    ) -> BoxFuture<'_, Result<WorkItemPageView>> {
        Box::pin(async move {
            params.base.validate()?;
            let mut query = params.base.normalized()?;
            query.filters = WorkspaceFilters::parse(params.filters.as_deref())?;
            query.merge_filters()?;
            query.due = match query.filters.due.as_deref() {
                Some("today") => Some(WorkItemDueFilter::Today),
                Some("overdue") => Some(WorkItemDueFilter::Overdue),
                _ => query.due,
            };
            let this = self.clone();
            self.db
                .client()
                .clone()
                .with_transaction(move |executor| {
                    Box::pin(async move { this.queue_page(query, actor, executor).await })
                })
                .await
        })
    }

    /// 范围、对象事实、完整计数和分页共享原事务快照。
    async fn queue_page(
        &self,
        query: dto::WorkItemListQuery,
        actor: AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<WorkItemPageView> {
        let (access, identity_version) = self.queue_access_with_version(&actor, executor).await?;
        let queue_context_id = queue_context_id(actor.id(), &query, &access);
        ensure_queue_context(&query.queue_context_id, &queue_context_id)?;
        let mut filter = access.scope_filter(&query)?;
        apply_due_filter(&mut filter, query.due)?;
        filter.narrow_current_handlers(&query.handler_user_ids);
        let (authorized_page, result_version, filter_options) =
            self.authorized_page_fields(&filter, &query, &access, executor).await?;
        let scope_version = queue_scope_version(&identity_version, &queue_context_id, &result_version);
        ensure_scope_version(query.page, query.scope_version.as_deref(), &scope_version)?;
        let fields = self
            .focused_fields(
                authorized_page.items,
                query.current_work_item_id.as_deref(),
                &filter,
                &query,
                FocusedQueueContext { page_size: query.page_size, access: &access },
                executor,
            )
            .await?;
        let items = self.project_fields(fields, query.scope, &access, &queue_context_id, executor).await?;
        self.validate_access(&access, executor).await?;
        Ok(WorkItemPageView {
            filter_options,
            items,
            total: authorized_page.total,
            page: query.page,
            page_size: query.page_size,
            queue_context_id,
            scope_version,
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
        query: &dto::WorkItemListQuery,
        context: FocusedQueueContext<'_>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<dto::WorkItemFields>> {
        let Some(current_id) = current_work_item_id else {
            return Ok(fields);
        };
        if let Some(index) = fields.iter().position(|item| item.id == current_id) {
            let current = fields.remove(index);
            fields.insert(0, current);
            return Ok(fields);
        }
        let mut candidates = filter.clone();
        candidates.query = None;
        let current = self
            .db
            .work_items()
            .find_visible_by_id(current_id, &candidates, executor)
            .await?
            .ok_or_else(|| Error::NotFound("当前焦点任务不在授权队列中".to_string()))?;
        let mut current = vec![dto::WorkItemFields::from(current)];
        for field in &mut current {
            field.search = super::search::task_document(field);
        }
        if query.filters.blocked {
            self.search_blockers(&mut current, query.scope, context.access, executor).await?;
        }
        let Some(current) = current.into_iter().find(|fields| {
            fields.search.matches(filter.query.as_deref()).is_some()
                && query.filters.matches(&fields.search)
                && matches_handler(fields, &query.handler_user_ids)
        }) else {
            return Err(Error::NotFound("当前焦点任务不在授权队列中".to_string()));
        };
        fields.insert(0, current);
        fields.truncate(context.page_size as usize);
        Ok(fields)
    }

    /// 打开一次候选游标，排序和完整授权计数仍沿用队列合同。
    ///
    /// # 参数
    /// `filter` 为候选筛选；`executor` 在整个扫描及授权过程中保持不变。
    /// # 返回
    /// 返回可逐批消费的排序游标，授权、计数和分页由调用方执行。
    /// # 错误
    /// 仓储查询失败时返回原领域错误。
    pub(super) async fn candidate_scan(
        &self,
        filter: &WorkItemFilter,
        executor: &mut dyn Executor,
    ) -> Result<WorkItemScan> {
        self.db
            .work_items()
            .open_work_item_scan(filter, AUTHORIZED_SCAN_BATCH_SIZE, executor)
            .await
            .map_err(Error::from)
    }

    /// 为已授权任务逐条计算审批阻断与允许动作。
    ///
    /// # 参数
    /// * `fields` - 已授权任务字段
    /// * `scope` - 责任范围
    /// * `actor` - 已认证操作人
    /// * `actor_access` - 操作人访问事实
    /// * `queue_context_id` - 当前队列上下文
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回补齐姓名、审批上下文和负责人资格后的任务投影。
    ///
    /// # 错误
    /// 供给预警、视图访问、路由投影、姓名、审批上下文或负责人资格读取失败时返回对应错误。
    pub(super) async fn project_fields(
        &self,
        fields: Vec<dto::WorkItemFields>,
        scope: WorkItemScope,
        actor_access: &QueueAccess,
        queue_context_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItemView>> {
        let projected = self.projected_access(actor_access, &fields, executor).await?;
        let mut items = Vec::with_capacity(fields.len());
        for fields in fields {
            let access = ViewAccess::from_fields(&fields, scope, &projected.task);
            items.push(WorkItemView::from_fields(fields, queue_context_id.to_string())?.with_access(
                access.processing_state,
                access.processing_blocker,
                access.allowed_actions,
                access.action_blockers,
            ));
        }
        self.apply_party_names_with(executor, &mut items).await?;
        self.apply_approval_contexts(&mut items, executor).await?;
        Ok(items)
    }

    /// 批量补齐审批任务的当前节点和最近驳回事实。
    ///
    /// # 参数
    /// * `items` - 已通过工作项和业务对象授权的安全投影
    /// * `executor` - 审批节点与实例读取沿用的执行器
    ///
    /// # 返回
    /// 无审批节点的任务保持不变；审批任务获得与节点执行严格绑定的运行上下文。
    ///
    /// # 错误
    /// BPM 节点执行或实例有界投影查询失败时返回错误。
    ///
    /// # 关键业务约束
    /// 列表固定执行两次批量查询，不得逐任务读取审批历史；最近驳回只取实例有界投影。
    pub(super) async fn apply_approval_contexts(
        &self,
        items: &mut [WorkItemView],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let execution_ids = items
            .iter()
            .filter_map(|item| item.approval_node_execution_id.as_deref())
            .map(bpm::ids::ApprovalNodeExecutionId::new)
            .collect::<Vec<_>>();
        if execution_ids.is_empty() {
            return Ok(());
        }
        let executions = self.db.bpm_workflow().list_executions_by_ids(&execution_ids, executor).await?;
        let instance_ids =
            executions.iter().map(|execution| execution.process_instance_id.clone()).collect::<Vec<_>>();
        let summaries =
            self.db.bpm_workflow().list_instance_summaries_by_ids(&instance_ids, executor).await?;
        let executions = executions
            .into_iter()
            .map(|execution| (execution.base.id.clone(), execution))
            .collect::<HashMap<_, _>>();
        let summaries =
            summaries.into_iter().map(|summary| (summary.id.clone(), summary)).collect::<HashMap<_, _>>();
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
    /// # 参数
    /// * `id` - 工作项稳定 ID
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回同一事务内重验授权后的单条任务投影。
    ///
    /// # 错误
    /// 任务不存在、不在安全责任范围、业务对象不可见，或授权与展示读取失败时返回错误。
    #[inline(never)]
    pub fn work_item_detail(self, id: String, actor: AuditActor) -> BoxFuture<'static, Result<WorkItemView>> {
        Box::pin(async move {
            let this = self.clone();
            self.db
                .client()
                .clone()
                .with_transaction(move |executor| {
                    Box::pin(async move { this.detail_page(&id, &actor, executor).await })
                })
                .await
        })
    }

    /// 使用 manage 的数据范围返回操作结果，不追加 detail 权限要求。
    /// # 参数
    /// `id` 为操作后的任务，`actor` 为真实操作人。
    /// # 返回
    /// 当前管理范围内的任务身份和状态。
    /// # 错误
    /// 范围撤销、任务不存在或读取失败时拒绝。
    pub fn work_item_action_view(
        self,
        id: String,
        actor: AuditActor,
    ) -> BoxFuture<'static, Result<WorkItemView>> {
        Box::pin(async move {
            let this = self.clone();
            self.db
                .client()
                .clone()
                .with_transaction(move |executor| {
                    Box::pin(async move { this.detail_page_for(&id, &actor, "manage", executor).await })
                })
                .await
        })
    }

    /// 在同一事务快照内重验详情授权、对象事实与展示补齐。
    ///
    /// # 参数
    /// * `id` - 工作项稳定 ID
    /// * `actor` - 已认证操作人
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 返回已授权的单条任务投影。
    ///
    /// # 错误
    /// 任务不存在、不可见或授权事实读取失败时返回错误。
    ///
    /// # 关键业务约束
    /// 授权快照、对象事实与展示读取共用同一执行器，不得换成 `NoTransaction`。
    #[inline(never)]
    pub fn detail_page<'a>(
        &'a self,
        id: &'a str,
        actor: &'a AuditActor,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<WorkItemView>> {
        self.detail_page_for(id, actor, "detail", executor)
    }

    /// 详情与操作结果使用各自动作的数据范围，材料不进入结果。
    fn detail_page_for<'a>(
        &'a self,
        id: &'a str,
        actor: &'a AuditActor,
        action: &'static str,
        executor: &'a mut dyn Executor,
    ) -> BoxFuture<'a, Result<WorkItemView>> {
        Box::pin(async move {
            self.authorization.require(actor, action, id, executor).await?;
            let item = self
                .db
                .work_items()
                .find_work_item(id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("任务不存在".to_string()))?;
            let item_id = item.base.id.clone();
            let access = self.task_access(actor, action, executor).await?;
            let scope = detail_scope(&item, actor.id(), &access)?;
            let fields = dto::WorkItemFields::from(item);
            let queue_context_id = single_item_context_id(actor.id(), &item_id);
            let projected = self.projected_access(&access, std::slice::from_ref(&fields), executor).await?;
            let view_access = ViewAccess::from_fields(&fields, scope, &projected.task);
            let mut view = WorkItemView::from_fields(fields, queue_context_id)?.with_access(
                view_access.processing_state,
                view_access.processing_blocker,
                view_access.allowed_actions,
                view_access.action_blockers,
            );
            self.apply_party_names_with(executor, std::slice::from_mut(&mut view)).await?;
            self.apply_approval_contexts(std::slice::from_mut(&mut view), executor).await?;
            self.validate_access(&access, executor).await?;
            Ok(view)
        })
    }
}

fn approval_context_view(
    execution: &bpm::model::ApprovalNodeExecution,
    summary: &crate::repository::bpm::ApprovalInstanceSummary,
) -> dto::WorkItemApprovalContextView {
    dto::WorkItemApprovalContextView {
        instance_id: summary.id.clone(),
        status: summary.status.as_str().to_string(),
        current_round_no: execution.round_no,
        current_node_label: execution.node_name.clone(),
        current_assignee_label: non_empty_text(&execution.assignee_name_snapshot),
        latest_rejection_reason: summary.latest_rejection_summary.as_deref().and_then(non_empty_text),
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
///
/// # 参数
/// * `actions` - 待就地过滤的允许动作
///
/// # 返回
/// 移除了 `Approve` 或 `Reject` 时返回 `true`。
///
/// # 错误
/// 不返回错误。
pub(super) fn remove_approval_decision_actions(actions: &mut Vec<WorkItemAllowedAction>) -> bool {
    let before = actions.len();
    actions
        .retain(|action| !matches!(action, WorkItemAllowedAction::Approve | WorkItemAllowedAction::Reject));
    actions.len() != before
}

/// 返回去除首尾空白后的非空展示文本。
fn non_empty_text(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
pub(super) fn matches_keyword(fields: &dto::WorkItemFields, q: Option<&str>) -> bool {
    super::search::task_document(fields).matches(q).is_some()
}

/// 当前处理人筛选只收窄授权结果；空集表示不过滤。
///
/// # 参数
/// * `fields` - 已授权任务字段
/// * `handler_ids` - 处理人稳定 ID；空切片表示不筛选
///
/// # 返回
/// 不筛选，或 `owner_user_id` 落在 `handler_ids` 中时返回 `true`。
///
/// # 错误
/// 不返回错误。
pub(super) fn matches_handler(fields: &dto::WorkItemFields, handler_ids: &[String]) -> bool {
    if handler_ids.is_empty() {
        return true;
    }
    fields.owner_user_id.as_deref().is_some_and(|owner| handler_ids.iter().any(|id| id == owner))
}

#[cfg(test)]
mod tests {
    use super::{
        AUTHORIZED_SCAN_BATCH_SIZE, AuthorizedPage, AuthorizedPageCollector, WorkItemAllowedAction,
        ensure_queue_context, remove_approval_decision_actions,
    };
    use crate::dto::work_item::WorkItemScope;

    #[test]
    fn keyword_filters_task_identity_without_searching_business_materials() {
        use erp_core::ids::WorkItemId;

        use crate::entity::work_item::{
            AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
        };
        let mut fields: super::dto::WorkItemFields = WorkItem::new(
            WorkItemId::new("task"),
            WorkItemData {
                work_item_type: WorkItemType::CardFundsReview,
                business_object_type: "receivable_account".into(),
                business_object_id: "object".into(),
                subject_version: "submission".into(),
                owner_role: "finance".into(),
                owner_organization_id: "company".into(),
                owner_user_id: "actor".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
        )
        .unwrap()
        .into();
        fields.business_object_label = "销售单 SO.[1]".into();
        fields.counterparty_label = Some("Acme 客户".into());
        assert!(!super::matches_keyword(&fields, Some(" acME ")));
        assert!(!super::matches_keyword(&fields, Some("SO.[1]")));
        assert!(super::matches_keyword(&fields, Some(" object ")));
        assert!(!super::matches_keyword(&fields, Some(".*")));
        assert!(super::matches_keyword(&fields, Some("  ")));
        let mut other = fields.clone();
        other.business_object_id = "other-object-id".into();
        other.id = "different".into();
        let mut collector = AuthorizedPageCollector::new(1, 1).unwrap();
        collector.extend(
            [other, fields.clone(), fields]
                .into_iter()
                .filter(|item| super::matches_keyword(item, Some("task"))),
        );
        let page = collector.finish();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.total, 2);
    }

    #[test]
    fn authorized_pagination_slices_after_authorization() {
        let mut collector = AuthorizedPageCollector::new(2, 2).unwrap();

        collector.extend(["authorized-1"]);
        collector.extend(["authorized-2", "authorized-3", "authorized-4"]);

        assert_eq!(
            collector.finish(),
            AuthorizedPage { items: vec!["authorized-3", "authorized-4"], total: 4 }
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
    fn scope_version_requires_anchor_and_detects_permission_or_result_changes() {
        let first = super::queue_scope_version("policy-1", "query", "result-1");
        assert!(super::ensure_scope_version(1, None, &first).is_ok());
        assert!(super::ensure_scope_version(2, None, &first).is_err());
        assert!(super::ensure_scope_version(2, Some(&first), &first).is_ok());
        for changed in [
            super::queue_scope_version("policy-2", "query", "result-1"),
            super::queue_scope_version("policy-1", "query", "result-2"),
            super::queue_scope_version("policy-1", "other-query", "result-1"),
        ] {
            let error = super::ensure_scope_version(2, Some(&first), &changed).unwrap_err();
            assert!(
                matches!(error, crate::error::Error::ConflictError(message) if message.starts_with("DATA_SCOPE_CHANGED"))
            );
        }
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
        assert_eq!(actions, vec![WorkItemAllowedAction::View, WorkItemAllowedAction::Process]);
        assert!(!remove_approval_decision_actions(&mut actions));
    }

    #[test]
    fn id_list_params_deduplicate_sort_and_reject_overflow() {
        let normalized = crate::dto::work_item::read::WorkItemListParams::normalized(
            &work_item_params_with_ids(Some("handler-2, handler-1,handler-2")),
        )
        .unwrap();
        assert_eq!(normalized.handler_user_ids, vec!["handler-1".to_string(), "handler-2".to_string()]);
        assert!(
            crate::dto::work_item::read::WorkItemListParams::normalized(&work_item_params_with_ids(Some(
                "handler-1,,handler-2"
            )))
            .is_err()
        );
        let overflow = (0..101).map(|index| format!("user-{index}")).collect::<Vec<_>>().join(",");
        assert!(
            crate::dto::work_item::read::WorkItemListParams::normalized(&work_item_params_with_ids(Some(
                &overflow
            )))
            .is_err()
        );
    }

    #[test]
    fn queue_context_binds_handler_and_order_filters_for_cross_page_version() {
        use crate::service::work_item::read::access::QueueAccess;

        fn query_with(
            handler: &[&str],
            sales: &[&str],
            purchase: &[&str],
        ) -> crate::dto::work_item::read::WorkItemListQuery {
            crate::dto::work_item::read::WorkItemListQuery {
                filters: Default::default(),
                scope: WorkItemScope::Mine,
                work_item_types: Vec::new(),
                statuses: Vec::new(),
                due: None,
                priorities: Vec::new(),
                query: None,
                handler_user_ids: handler.iter().map(|value| value.to_string()).collect(),
                sales_order_ids: sales.iter().map(|value| value.to_string()).collect(),
                purchase_order_ids: purchase.iter().map(|value| value.to_string()).collect(),
                current_work_item_id: None,
                sort_by: "created_at",
                sort_ascending: false,
                page: 1,
                page_size: 20,
                queue_context_id: None,
                scope_version: None,
            }
        }

        fn access() -> QueueAccess {
            QueueAccess::new("actor".to_string())
        }

        let access = access();
        let base = query_with(&[], &[], &[]);
        let base_context = super::queue_context_id("actor", &base, &access);
        assert_eq!(base_context, super::queue_context_id("actor", &query_with(&[], &[], &[]), &access));
        for varied in [
            query_with(&["handler-1"], &[], &[]),
            query_with(&[], &["so-1"], &[]),
            query_with(&[], &[], &["po-1"]),
        ] {
            let varied_context = super::queue_context_id("actor", &varied, &access);
            assert_ne!(base_context, varied_context);
            assert!(super::ensure_queue_context(&Some(base_context.clone()), &varied_context).is_err());
            let base_version = super::queue_scope_version("identity", &base_context, "result");
            let varied_version = super::queue_scope_version("identity", &varied_context, "result");
            assert_ne!(base_version, varied_version);
            assert!(super::ensure_scope_version(2, Some(&base_version), &varied_version).is_err());
        }
    }

    fn work_item_params_with_ids(
        handler_user_ids: Option<&str>,
    ) -> crate::dto::work_item::read::WorkItemListParams {
        crate::dto::work_item::read::WorkItemListParams {
            scope: WorkItemScope::Mine,
            family: None,
            work_item_type: None,
            status: None,
            due: None,
            priorities: None,
            q: None,
            handler_user_ids: handler_user_ids.map(str::to_string),
            sales_order_ids: None,
            purchase_order_ids: None,
            sort: None,
            queue_context_id: None,
            scope_version: None,
            current_work_item_id: None,
            timezone: Some("Asia/Shanghai".to_string()),
            page: None,
            page_size: None,
        }
    }
}
