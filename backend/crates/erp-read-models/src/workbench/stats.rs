//! 责任队列待办统计；每批授权、资格重验及累计后立即释放。

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_workflow::entity::work_item::{WorkItemStatus, WorkItemType};
use erp_workflow::{WorkItemRow, WorkflowAuthorizationPort};
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::access::{ActorAccess, ViewAccess};
use super::action_projection::ActionProjection;
use super::owner_qualification::QualificationCache;
use super::query::AUTHORIZED_SCAN_BATCH_SIZE;
use super::{
    ProcessingState, WorkItemAllowedAction, WorkItemDueFilter, WorkItemFamily, WorkItemFamilyCountsView,
    WorkItemFilter, WorkItemScope, WorkItemStatsParams, WorkItemStatsView, WorkbenchReadService, dto,
};
use crate::errors::{Error, Result};

/// 统计时限与选中类型共用固定基准，累计状态不保存任何任务或对象事实。
struct StatsContext<'a> {
    actor: &'a AuditActor,
    access: &'a ActorAccess,
    selected_types: &'a [WorkItemType],
    as_of: Instant,
    today_start: Instant,
    tomorrow_start: Instant,
}

/// 一批投影保留的统计字段；其寿命不跨越下一批读取。
#[derive(Clone, Copy)]
struct StatIdentity {
    work_item_type: WorkItemType,
    due_at: Option<Instant>,
}

/// 累计个人全部任务族，并同时汇总当前选中类型。
#[derive(Default)]
struct StatsCounts {
    selected: SelectedCounts,
    family_counts: WorkItemFamilyCountsView,
}

/// 选中范围的可处理总数、到期及异常指标。
#[derive(Default)]
struct SelectedCounts {
    assigned: u64,
    due_today: u64,
    overdue: u64,
    exception: u64,
}

impl StatsCounts {
    /// 消费并释放一个完整批次，累计器不持有任务或权限投影。
    fn extend_batch(
        &mut self,
        batch: impl IntoIterator<Item = (StatIdentity, ViewAccess)>,
        scope: WorkItemScope,
        context: &StatsContext<'_>,
    ) {
        for (item, access) in batch {
            self.observe(item, scope, &access, context);
        }
    }

    /// 只累计正式动作判定通过的任务；任务族和选中类型分别保留原口径。
    fn observe(
        &mut self,
        item: StatIdentity,
        scope: WorkItemScope,
        access: &ViewAccess,
        context: &StatsContext<'_>,
    ) {
        if !counts_as_processable_stat(scope, access) {
            return;
        }
        increment_family_count(&mut self.family_counts, item.work_item_type);
        if !context.selected_types.contains(&item.work_item_type) {
            return;
        }
        self.selected.assigned = self.selected.assigned.saturating_add(1);
        if item.due_at.is_some_and(|due| due >= context.today_start && due < context.tomorrow_start) {
            self.selected.due_today = self.selected.due_today.saturating_add(1);
        }
        if item.due_at.is_some_and(|due| due < context.as_of) {
            self.selected.overdue = self.selected.overdue.saturating_add(1);
        }
        if matches!(
            item.work_item_type,
            WorkItemType::IntegrationResultUnknown | WorkItemType::BusinessException
        ) {
            self.selected.exception = self.selected.exception.saturating_add(1);
        }
    }
}

impl<A: WorkflowAuthorizationPort + Clone + Send + Sync + 'static> WorkbenchReadService<A> {
    /// 查询与正式队列复用同一授权快照的待办统计。
    ///
    /// # 参数
    /// * `params` - 责任范围、任务族、类型、时限与工作时区
    /// * `actor` - 已认证操作人
    /// # 返回
    /// 返回个人、今日到期、超期、异常、任务族计数及服务端统计时点。
    /// # 错误
    /// 查询参数、权限范围或对象事实读取失败时返回错误。
    pub async fn work_item_stats(
        &self,
        params: WorkItemStatsParams,
        actor: AuditActor,
    ) -> Result<WorkItemStatsView> {
        params.validate()?;
        let query = params.normalized()?;
        let all_families = params.family.is_none() && params.work_item_type.is_none();
        let this = self.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.stats_at(query, actor, all_families, executor).await })
            })
            .await
    }

    /// 个人统计与任务族统计单遍计算；非个人选中范围仍执行其原授权查询。
    async fn stats_at(
        &self,
        query: dto::WorkItemListQuery,
        actor: AuditActor,
        all_families: bool,
        executor: &mut dyn Executor,
    ) -> Result<WorkItemStatsView> {
        let access = self.actor_access_for(actor.kind(), actor.id(), executor).await?;
        self.scope_filter(&query, &actor, &access)?;
        let as_of = Instant::now();
        let (today_start, tomorrow_start) = business_day_bounds_at(as_of.unix_secs())?;
        let context = StatsContext {
            actor: &actor,
            access: &access,
            selected_types: &query.work_item_types,
            as_of,
            today_start,
            tomorrow_start,
        };
        let mut personal_query = query.clone();
        personal_query.scope = WorkItemScope::Mine;
        personal_query.statuses = vec![WorkItemStatus::Open];
        if !all_families {
            personal_query.work_item_types = registered_work_item_types();
        }
        let personal = self.stream_stats(&personal_query, &context, executor).await?;
        let assigned = personal.selected.assigned;
        let selected = if query.scope == WorkItemScope::Mine {
            personal.selected
        } else {
            self.stream_stats(&query, &context, executor).await?.selected
        };
        Ok(WorkItemStatsView {
            assigned,
            due_today: selected.due_today,
            overdue: selected.overdue,
            exception: selected.exception,
            family_counts: personal.family_counts,
            as_of,
        })
    }

    /// 使用完整授权范围扫描，每批只保留本批动作事实和常量大小计数器。
    async fn stream_stats(
        &self,
        query: &dto::WorkItemListQuery,
        context: &StatsContext<'_>,
        executor: &mut dyn Executor,
    ) -> Result<StatsCounts> {
        let mut filter = self.scope_filter(query, context.actor, context.access)?;
        apply_due_filter_at(&mut filter, query.due, context.as_of)?;
        filter.narrow_current_handlers(&query.handler_user_ids);
        filter.query = None;
        let mut counts = StatsCounts::default();
        let mut qualification = QualificationCache::new(executor);
        let mut scan = self.candidate_scan(&filter, executor).await?;
        loop {
            let rows = scan.next_batch(AUTHORIZED_SCAN_BATCH_SIZE, executor).await?;
            let candidate_count = rows.len();
            if candidate_count == 0 {
                break;
            }
            self.count_stat_batch(rows, query, context, &mut counts, &mut qualification, executor).await?;
            if candidate_count < AUTHORIZED_SCAN_BATCH_SIZE.get() as usize {
                break;
            }
        }
        Ok(counts)
    }

    /// 授权与处理人、来源筛选均发生在累计之前，负责人资格直接复用本批权威事实。
    async fn count_stat_batch(
        &self,
        rows: Vec<WorkItemRow>,
        query: &dto::WorkItemListQuery,
        context: &StatsContext<'_>,
        counts: &mut StatsCounts,
        qualification: &mut QualificationCache,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut facts = self.candidate_object_facts(&rows, false, executor).await?;
        let fields = self.authorize_queue_rows(rows, context.access, &mut facts, executor).await?;
        let mut identities = Vec::with_capacity(fields.len());
        let mut actions = Vec::with_capacity(fields.len());
        for item in fields {
            if !super::query::matches_handler(&item, &query.handler_user_ids)
                || !super::query::matches_order_sources(
                    &item,
                    &facts,
                    &query.sales_order_ids,
                    &query.purchase_order_ids,
                )
            {
                continue;
            }
            let access = self.view_access(&item, query.scope, context.actor, context.access)?;
            identities.push(StatIdentity { work_item_type: item.work_item_type, due_at: item.due_at });
            actions.push(ActionProjection::from_fields(item, access)?);
        }
        self.apply_stat_approval_contexts(&mut actions, executor).await?;
        let authority = facts.into_iter().map(|(key, fact)| (key, fact.authority)).collect();
        self.apply_action_qualification(&mut actions, &authority, qualification, executor).await?;
        counts.extend_batch(
            identities.into_iter().zip(actions).map(|(item, action)| (item, action.access)),
            query.scope,
            context,
        );
        Ok(())
    }
}

/// 判断任务是否计入「当前真的能推进」的指标。
///
/// 单据审批任务由审批运行时直接指派，只会拿到 `Approve`/`Reject`——`allowed_actions`
/// 的 `Process` 分支要求 `owner_role` 能过责任范围校验，而审批任务的 `owner_role`
/// 是语义标签（`sales_order_approver`）不是角色 ID，永远过不了。只认 `Process` 会让
/// 待办列表有条目、「待我处理」却是 0。
///
/// # 参数
/// `scope` 为统计责任范围，`access` 为当前完整资格判断。
/// # 返回
/// 个人范围中准备就绪且可处理或决定的任务返回 true。
/// # 错误
/// 无。
pub(super) fn counts_as_processable_stat(scope: WorkItemScope, access: &ViewAccess) -> bool {
    if access.processing_state != ProcessingState::Ready {
        return false;
    }
    match scope {
        WorkItemScope::Mine => access
            .allowed_actions
            .iter()
            .any(|action| matches!(action, WorkItemAllowedAction::Process | WorkItemAllowedAction::Approve)),
        WorkItemScope::Managed | WorkItemScope::History => false,
    }
}

/// 正式列表按调用时点构造到期条件；统计另用固定响应时点。
///
/// # 参数
/// `filter` 为候选范围，`due` 为可选时限筛选。
/// # 返回
/// 原地写入时限条件；无筛选时保持不变。
/// # 错误
/// 工作时区窗口无法形成时返回错误。
pub(super) fn apply_due_filter(filter: &mut WorkItemFilter, due: Option<WorkItemDueFilter>) -> Result<()> {
    apply_due_filter_at(filter, due, Instant::now())
}

/// 使用固定服务端时点构造到期范围，保证各统计批次和响应时点一致。
fn apply_due_filter_at(
    filter: &mut WorkItemFilter,
    due: Option<WorkItemDueFilter>,
    as_of: Instant,
) -> Result<()> {
    let Some(due) = due else {
        return Ok(());
    };
    let window = due.window_at(as_of).map_err(|error| Error::Internal(error.to_string()))?;
    filter.due_from = window.from;
    filter.due_before = Some(window.before);
    Ok(())
}

/// 按固定时点计算工作时区的今日半开区间。
///
/// # 参数
/// `now_unix_secs` 为与响应一致的服务端时点。
/// # 返回
/// 返回今日下界与明日上界。
/// # 错误
/// 工作时区窗口无法形成时返回错误。
///
/// # Panics
/// `window.from` 为 `None` 时 panic。`WorkItemDueFilter::Today` 的窗口总会带下界，此分支不应发生。
pub(super) fn business_day_bounds_at(now_unix_secs: i64) -> Result<(Instant, Instant)> {
    let window = WorkItemDueFilter::Today
        .window_at(Instant::from_unix_secs(now_unix_secs))
        .map_err(|error| Error::Internal(error.to_string()))?;
    Ok((window.from.expect("今日窗口必须有下界"), window.before))
}

/// 返回服务器注册的全部任务类型，不受当前选中分组限制。
fn registered_work_item_types() -> Vec<WorkItemType> {
    [
        WorkItemFamily::Approval,
        WorkItemFamily::Procurement,
        WorkItemFamily::Fulfillment,
        WorkItemFamily::Finance,
        WorkItemFamily::Exception,
    ]
    .into_iter()
    .flat_map(WorkItemFamily::work_item_types)
    .collect()
}

/// 单条任务族计数只依赖领域的固定映射。
fn increment_family_count(counts: &mut WorkItemFamilyCountsView, work_item_type: WorkItemType) {
    let count = match dto::family_of(work_item_type) {
        WorkItemFamily::Approval => &mut counts.approval,
        WorkItemFamily::Procurement => &mut counts.procurement,
        WorkItemFamily::Fulfillment => &mut counts.fulfillment,
        WorkItemFamily::Finance => &mut counts.finance,
        WorkItemFamily::Exception => &mut counts.exception,
    };
    *count = count.saturating_add(1);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use erp_core::AccountKind;

    use super::*;

    /// 使用固定业务时点构造实际计数上下文。
    fn context<'a>(
        actor: &'a AuditActor,
        access: &'a ActorAccess,
        types: &'a [WorkItemType],
        as_of: Instant,
    ) -> StatsContext<'a> {
        let (today_start, tomorrow_start) = business_day_bounds_at(as_of.unix_secs()).unwrap();
        StatsContext { actor, access, selected_types: types, as_of, today_start, tomorrow_start }
    }

    /// 构造已完成正式资格检查的最小统计身份与动作。
    fn ready(kind: WorkItemType, due: Option<Instant>) -> (StatIdentity, ViewAccess) {
        (
            StatIdentity { work_item_type: kind, due_at: due },
            ViewAccess::ready(vec![WorkItemAllowedAction::Process]),
        )
    }

    /// 模拟扫描批次寿命；生产累计方法消费后必须释放源批次。
    struct TrackedBatch {
        rows: std::vec::IntoIter<(StatIdentity, ViewAccess)>,
        drops: Arc<AtomicUsize>,
    }
    impl Iterator for TrackedBatch {
        fn next(&mut self) -> Option<Self::Item> {
            self.rows.next()
        }
        type Item = (StatIdentity, ViewAccess);
    }
    impl Drop for TrackedBatch {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// 全族计数与选定类型并行累计，且消费完每一批后释放原批次。
    #[test]
    fn batches_release_items_and_keep_selected_and_personal_family_counts() {
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let access = ActorAccess::new("actor".into());
        let selected_types = [WorkItemType::DocumentApproval];
        let context = context(&actor, &access, &selected_types, Instant::from_unix_secs(1_790_784_000));
        let drops = Arc::new(AtomicUsize::new(0));
        let mut counts = StatsCounts::default();
        for rows in [
            vec![
                ready(WorkItemType::DocumentApproval, None),
                ready(WorkItemType::FulfillmentOperation, None),
            ],
            vec![ready(WorkItemType::DocumentApproval, None), ready(WorkItemType::BusinessException, None)],
            Vec::new(),
        ] {
            let previous = drops.load(Ordering::SeqCst);
            counts.extend_batch(
                TrackedBatch { rows: rows.into_iter(), drops: drops.clone() },
                WorkItemScope::Mine,
                &context,
            );
            assert_eq!(drops.load(Ordering::SeqCst), previous + 1);
        }
        assert_eq!(counts.selected.assigned, 2);
        assert_eq!(counts.selected.exception, 0);
        assert_eq!(
            counts.family_counts,
            WorkItemFamilyCountsView { approval: 2, fulfillment: 1, exception: 1, ..Default::default() }
        );
    }

    /// 审批可决定即计入；失效、受阻、仅查看和非个人范围沿正式动作语义排除。
    #[test]
    fn eligibility_and_scope_exclude_unprocessable_statistics() {
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let access = ActorAccess::new("actor".into());
        let types = [WorkItemType::DocumentApproval];
        let context = context(&actor, &access, &types, Instant::from_unix_secs(1_790_784_000));
        let identity = StatIdentity {
            work_item_type: WorkItemType::DocumentApproval,
            due_at: Some(context.today_start),
        };
        let mut counts = StatsCounts::default();
        let approve = ViewAccess::ready(vec![WorkItemAllowedAction::Approve, WorkItemAllowedAction::Reject]);
        counts.observe(identity, WorkItemScope::Mine, &approve, &context);
        counts.observe(identity, WorkItemScope::Managed, &approve, &context);
        counts.observe(identity, WorkItemScope::History, &approve, &context);
        counts.observe(
            identity,
            WorkItemScope::Mine,
            &ViewAccess::ready(vec![WorkItemAllowedAction::View]),
            &context,
        );
        for state in [ProcessingState::ApprovalBlocked, ProcessingState::ExecutionBlocked] {
            let mut blocked = ViewAccess::ready(vec![WorkItemAllowedAction::Process]);
            blocked.processing_state = state;
            counts.observe(identity, WorkItemScope::Mine, &blocked, &context);
        }
        assert_eq!(counts.selected.assigned, 1);
        assert_eq!(counts.selected.due_today, 1);
        assert_eq!(counts.family_counts.approval, 1);
    }

    /// 今日使用半开窗口，等于统计时点的任务不算逾期，午夜两侧按同一时点判定。
    #[test]
    fn midnight_due_and_overdue_boundaries_use_response_timestamp() {
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let access = ActorAccess::new("actor".into());
        let types = [WorkItemType::BusinessException];
        let midnight = Instant::from_unix_secs(1_790_697_600);
        let context = context(&actor, &access, &types, midnight);
        assert_eq!(context.today_start, midnight);
        let mut counts = StatsCounts::default();
        let before = Instant::from_unix_secs(midnight.unix_secs() - 1);
        counts.extend_batch(
            [
                ready(WorkItemType::BusinessException, Some(before)),
                ready(WorkItemType::BusinessException, Some(midnight)),
                ready(WorkItemType::BusinessException, Some(context.tomorrow_start)),
                ready(WorkItemType::BusinessException, None),
            ],
            WorkItemScope::Mine,
            &context,
        );
        assert_eq!(counts.selected.assigned, 4);
        assert_eq!(counts.selected.due_today, 1);
        assert_eq!(counts.selected.overdue, 1);
        assert_eq!(counts.selected.exception, 4);
        let mut filter = WorkItemFilter::default();
        apply_due_filter_at(&mut filter, Some(WorkItemDueFilter::Today), midnight).unwrap();
        assert_eq!(filter.due_from, Some(midnight));
        assert_eq!(filter.due_before, Some(context.tomorrow_start));
    }
}
