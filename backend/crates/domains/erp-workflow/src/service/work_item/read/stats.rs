//! 本人待办统计按持久化分配累计；办理资格不参与任务计数。

use std::collections::HashMap;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use futures_util::future::BoxFuture;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::access::ActorAccess;
use super::dto::WorkItemTypeTiming;
use super::query::{AUTHORIZED_SCAN_BATCH_SIZE, matches_handler};
use super::{
    WorkItemDueFilter, WorkItemFamily, WorkItemFamilyCountsView, WorkItemFilter, WorkItemReadService,
    WorkItemScope, WorkItemStatsParams, WorkItemStatsView, dto, family_of,
};
use crate::entity::work_item::{WorkItemStatus, WorkItemType};
use crate::error::{Error, Result};

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
    work_item_type_counts: HashMap<WorkItemType, u64>,
    work_item_type_timing: HashMap<WorkItemType, WorkItemTypeTiming>,
}

/// 选中范围的待办总数、到期及异常指标。
#[derive(Default)]
struct SelectedCounts {
    assigned: u64,
    due_today: u64,
    overdue: u64,
    exception: u64,
}

impl StatsCounts {
    /// 消费并释放本人开放任务批次，累计器不持有任务或对象资料。
    fn extend_batch(&mut self, batch: impl IntoIterator<Item = StatIdentity>, context: &StatsContext<'_>) {
        for item in batch {
            self.observe(item, context);
        }
    }

    /// 同一任务分配条件供总数与各类型统计使用；业务筛选只收窄结果。
    fn extend_pending(
        &mut self,
        fields: impl IntoIterator<Item = dto::WorkItemFields>,
        query: &dto::WorkItemListQuery,
        context: &StatsContext<'_>,
    ) {
        self.extend_batch(
            fields
                .into_iter()
                .filter(|item| item.is_pending_for(context.actor.id()))
                .filter(|item| matches_handler(item, &query.handler_user_ids))
                .map(|item| StatIdentity { work_item_type: item.work_item_type, due_at: item.due_at }),
            context,
        );
    }

    /// 任务已经通过当前分配与开放状态筛选，缺少办理权限不影响计数。
    fn observe(&mut self, item: StatIdentity, context: &StatsContext<'_>) {
        increment_family_count(&mut self.family_counts, item.work_item_type);
        let count = self.work_item_type_counts.entry(item.work_item_type).or_default();
        *count = count.saturating_add(1);
        let timing = self.work_item_type_timing.entry(item.work_item_type).or_default();
        let today = item.due_at.is_some_and(|due| due >= context.today_start && due < context.tomorrow_start);
        let overdue = item.due_at.is_some_and(|due| due < context.as_of);
        if today {
            timing.due_today = timing.due_today.saturating_add(1);
        }
        if overdue {
            timing.overdue = timing.overdue.saturating_add(1);
        }
        if !context.selected_types.contains(&item.work_item_type) {
            return;
        }
        self.selected.assigned = self.selected.assigned.saturating_add(1);
        if today {
            self.selected.due_today = self.selected.due_today.saturating_add(1);
        }
        if overdue {
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

impl WorkItemReadService {
    /// 查询与正式队列复用同一授权快照的待办统计。
    ///
    /// # 参数
    /// * `params` - 责任范围、任务族、类型、时限与工作时区
    /// * `actor` - 已认证操作人
    /// # 返回
    /// 返回个人、今日到期、超期、异常、任务族计数及服务端统计时点。
    /// # 错误
    /// 查询参数、权限范围或对象事实读取失败时返回错误。
    #[inline(never)]
    pub fn work_item_stats(
        &self,
        params: WorkItemStatsParams,
        actor: AuditActor,
    ) -> BoxFuture<'_, Result<WorkItemStatsView>> {
        Box::pin(async move {
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
        })
    }

    /// 个人统计与任务族统计单遍计算；非个人范围校验入口后保留零待办指标。
    async fn stats_at(
        &self,
        query: dto::WorkItemListQuery,
        actor: AuditActor,
        all_families: bool,
        executor: &mut dyn Executor,
    ) -> Result<WorkItemStatsView> {
        let (access, version) = self.queue_access_with_version(&actor, executor).await?;
        access.scope_filter(&query)?;
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
        let current = self.queue_scope_version(&actor, executor).await?;
        super::context::ensure_scope_version(1, Some(&version), &current)?;
        Ok(WorkItemStatsView {
            assigned,
            due_today: selected.due_today,
            overdue: selected.overdue,
            exception: selected.exception,
            family_counts: personal.family_counts,
            work_item_type_counts: personal.work_item_type_counts,
            work_item_type_timing: personal.work_item_type_timing,
            as_of,
        })
    }

    /// 按当前任务分配分批扫描，累计器只保留类型和时限计数。
    async fn stream_stats(
        &self,
        query: &dto::WorkItemListQuery,
        context: &StatsContext<'_>,
        executor: &mut dyn Executor,
    ) -> Result<StatsCounts> {
        let mut filter = context.access.scope_filter(query)?;
        apply_due_filter_at(&mut filter, query.due, context.as_of)?;
        filter.narrow_current_handlers(&query.handler_user_ids);
        filter.query = None;
        let mut counts = StatsCounts::default();
        if query.scope != WorkItemScope::Mine {
            return Ok(counts);
        }
        let mut scan = self.candidate_scan(&filter, executor).await?;
        loop {
            let rows = scan.next_batch(AUTHORIZED_SCAN_BATCH_SIZE, executor).await?;
            let candidate_count = rows.len();
            if candidate_count == 0 {
                break;
            }
            counts.extend_pending(rows.into_iter().map(dto::WorkItemFields::from), query, context);
            if candidate_count < AUTHORIZED_SCAN_BATCH_SIZE.get() as usize {
                break;
            }
        }
        Ok(counts)
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
    let count = match family_of(work_item_type) {
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

    /// 仅有待办列表权限时，审批与采购任务按同一当前分配规则进入列表和统计。
    #[test]
    fn pending_counts_match_assignment_pages_without_business_or_execution_permissions() {
        use bpm::ids::ApprovalNodeExecutionId;
        use erp_core::ids::WorkItemId;
        use erp_identity::Permission;

        use crate::dto::work_item::WorkItemListParams;
        use crate::entity::work_item::{DocumentApprovalWorkItemData, WorkItem, WorkItemPriority};
        use crate::service::work_item::read::context::AuthorizedPageCollector;

        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let access = ActorAccess::new("actor".into())
            .with_permissions(vec![Permission::parse("work_item:list").unwrap()]);
        let at = Instant::from_unix_secs(1_790_784_000);
        let approval: dto::WorkItemFields = WorkItem::new_document_approval(
            WorkItemId::new("approval"),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: ApprovalNodeExecutionId::new("execution"),
                business_object_type: "sales_order".into(),
                business_object_id: "order".into(),
                subject_version: "1".into(),
                owner_role: "sales_order_approver".into(),
                owner_organization_id: "org".into(),
                owner_user_id: "actor".into(),
                priority: WorkItemPriority::Normal,
                due_at: Some(at),
            },
            at,
        )
        .unwrap()
        .into();
        let mut procurement = approval.clone();
        procurement.id = "procurement".into();
        procurement.work_item_type = WorkItemType::ProcurementOrderCreation;
        procurement.approval_node_execution_id = None;
        let mut rows = vec![approval.clone(), procurement];
        for (status, owner) in [
            (WorkItemStatus::Open, Some("other")),
            (WorkItemStatus::Open, None),
            (WorkItemStatus::Closed, Some("actor")),
            (WorkItemStatus::Completed, Some("actor")),
        ] {
            let mut item = approval.clone();
            item.status = status;
            item.owner_user_id = owner.map(str::to_owned);
            rows.push(item);
        }
        let params: WorkItemListParams = serde_json::from_value(serde_json::json!({"scope":"mine"})).unwrap();
        let query = params.normalized().unwrap();
        let context = context(&actor, &access, &query.work_item_types, at);
        let mut counts = StatsCounts::default();
        // 来源事实缺失或授权未就绪不改变持久化任务数量。
        counts.extend_pending(rows.clone(), &query, &context);
        assert_eq!(counts.selected.assigned, 2);
        assert_eq!(counts.family_counts.approval, 1);
        assert_eq!(counts.family_counts.procurement, 1);
        assert_eq!(counts.selected.due_today, 2);
        for (page, expected) in [(1, vec!["approval"]), (2, vec!["procurement"]), (3, vec![])] {
            let mut collector = AuthorizedPageCollector::new(page, 1).unwrap();
            collector.extend(rows.clone().into_iter().filter(|item| item.is_pending_for(actor.id())));
            let page = collector.finish();
            assert_eq!(u64::try_from(page.total).unwrap(), counts.selected.assigned);
            assert_eq!(page.items.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(), expected);
        }
        let mut other_handler = query.clone();
        other_handler.handler_user_ids = vec!["other".into()];
        let mut narrowed = StatsCounts::default();
        narrowed.extend_pending(rows, &other_handler, &context);
        assert_eq!(narrowed.selected.assigned, 0);
    }

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

    /// 构造已由当前任务分配确定的最小统计身份。
    fn assigned(kind: WorkItemType, due: Option<Instant>) -> StatIdentity {
        StatIdentity { work_item_type: kind, due_at: due }
    }

    /// 模拟扫描批次寿命；生产累计方法消费后必须释放源批次。
    struct TrackedBatch {
        rows: std::vec::IntoIter<StatIdentity>,
        drops: Arc<AtomicUsize>,
    }
    impl Iterator for TrackedBatch {
        fn next(&mut self) -> Option<Self::Item> {
            self.rows.next()
        }
        type Item = StatIdentity;
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
                assigned(WorkItemType::DocumentApproval, None),
                assigned(WorkItemType::FulfillmentOperation, None),
            ],
            vec![
                assigned(WorkItemType::DocumentApproval, None),
                assigned(WorkItemType::BusinessException, None),
            ],
            Vec::new(),
        ] {
            let previous = drops.load(Ordering::SeqCst);
            counts.extend_batch(TrackedBatch { rows: rows.into_iter(), drops: drops.clone() }, &context);
            assert_eq!(drops.load(Ordering::SeqCst), previous + 1);
        }
        assert_eq!(counts.selected.assigned, 2);
        assert_eq!(counts.selected.exception, 0);
        assert_eq!(
            counts.work_item_type_counts,
            HashMap::from([
                (WorkItemType::DocumentApproval, 2),
                (WorkItemType::FulfillmentOperation, 1),
                (WorkItemType::BusinessException, 1),
            ])
        );
        assert_eq!(
            counts.family_counts,
            WorkItemFamilyCountsView { approval: 2, fulfillment: 1, exception: 1, ..Default::default() }
        );
    }

    #[test]
    fn type_timing_keeps_missing_dates_and_day_boundaries_distinct() {
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let access = ActorAccess::new("actor".into());
        let types = [WorkItemType::DocumentApproval];
        let context = context(&actor, &access, &types, Instant::from_unix_secs(1_790_787_600));
        let mut counts = StatsCounts::default();
        for due in [
            None,
            Some(context.today_start),
            Some(context.tomorrow_start),
            Some(Instant::from_unix_secs(context.today_start.unix_secs() - 1)),
        ] {
            let item = assigned(WorkItemType::SupplierPaymentExecution, due);
            counts.observe(item, &context);
        }
        assert_eq!(counts.selected.assigned, 0);
        assert_eq!(counts.work_item_type_counts[&WorkItemType::SupplierPaymentExecution], 4);
        assert_eq!(
            counts.work_item_type_timing[&WorkItemType::SupplierPaymentExecution],
            WorkItemTypeTiming { due_today: 1, overdue: 2 }
        );
    }

    /// 空队列不产生分类，跨越多个页面的任务仍完整累计并返回稳定类型码。
    #[test]
    fn type_counts_cover_multiple_pages_and_empty_queues() {
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let access = ActorAccess::new("actor".into());
        let types = [WorkItemType::ProcurementOrderCreation];
        let context = context(&actor, &access, &types, Instant::from_unix_secs(1_790_784_000));
        let mut counts = StatsCounts::default();
        counts.extend_batch([], &context);
        assert!(counts.work_item_type_counts.is_empty());
        for batch_size in [50, 50, 25] {
            counts.extend_batch(
                (0..batch_size).map(|_| assigned(WorkItemType::ProcurementOrderCreation, None)),
                &context,
            );
        }
        assert_eq!(counts.selected.assigned, 125);
        assert_eq!(counts.family_counts.procurement, 125);
        assert_eq!(
            serde_json::to_value(&counts.work_item_type_counts).unwrap(),
            serde_json::json!({ "PROCUREMENT_ORDER_CREATION": 125 })
        );
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
                assigned(WorkItemType::BusinessException, Some(before)),
                assigned(WorkItemType::BusinessException, Some(midnight)),
                assigned(WorkItemType::BusinessException, Some(context.tomorrow_start)),
                assigned(WorkItemType::BusinessException, None),
            ],
            &context,
        );
        assert_eq!(counts.selected.assigned, 4);
        assert_eq!(counts.selected.due_today, 1);
        assert_eq!(
            counts.work_item_type_timing.get(&WorkItemType::BusinessException),
            Some(&WorkItemTypeTiming { due_today: 1, overdue: 1 })
        );
        assert_eq!(counts.selected.overdue, 1);
        assert_eq!(counts.selected.exception, 4);
        let mut filter = WorkItemFilter::default();
        apply_due_filter_at(&mut filter, Some(WorkItemDueFilter::Today), midnight).unwrap();
        assert_eq!(filter.due_from, Some(midnight));
        assert_eq!(filter.due_before, Some(context.tomorrow_start));
    }
}
