//! 待办统计按持久化责任范围与业务筛选累计；办理资格不参与任务计数。

use std::collections::{BTreeMap, HashMap};

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_permissions::work_item;
use futures_util::future::BoxFuture;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::access::QueueAccess;
use super::dto::WorkItemTypeTiming;
use super::query::{AUTHORIZED_SCAN_BATCH_SIZE, matches_counterparty, matches_handler};
use super::{
    WorkItemDueFilter, WorkItemFamily, WorkItemFamilyCountsView, WorkItemFilter, WorkItemReadService,
    WorkItemScope, WorkItemStatsParams, WorkItemStatsView, dto, family_of,
};
use crate::entity::work_item::{WorkItemStatus, WorkItemType};
use crate::error::{Error, Result};

/// 统计时限与选中类型共用固定基准，累计状态不保存任何任务或对象事实。
struct StatsContext<'a> {
    actor: &'a AuditActor,
    selected_types: &'a [WorkItemType],
    as_of: Instant,
    today_start: Instant,
    tomorrow_start: Instant,
}

/// 一批投影保留的统计字段；其寿命不跨越下一批读取。
struct StatIdentity {
    work_item_type: WorkItemType,
    business_object_type: String,
    status: WorkItemStatus,
    due_at: Option<Instant>,
    matches_item_types: bool,
}

/// 累计当前范围的全部任务族、类型、审批单据拆解，并同时汇总当前选中类型。
#[derive(Default)]
struct StatsCounts {
    selected: SelectedCounts,
    family_counts: WorkItemFamilyCountsView,
    work_item_type_counts: HashMap<WorkItemType, u64>,
    work_item_type_timing: HashMap<WorkItemType, WorkItemTypeTiming>,
    approval_document_type_counts: BTreeMap<String, u64>,
    item_type_counts: BTreeMap<String, u64>,
}

/// 选中范围的待办总数、到期及异常指标。
#[derive(Default)]
struct SelectedCounts {
    total: u64,
    due_today: u64,
    overdue: u64,
    exception: u64,
}

impl StatsCounts {
    // 累计一批经授权任务后释放该批数据；累计器只保留计数，不留任务和单据明细。
    fn extend_batch(&mut self, batch: impl IntoIterator<Item = StatIdentity>, context: &StatsContext<'_>) {
        for item in batch {
            self.observe(item, context);
        }
    }

    // 在当前责任范围内按 `q`、`filters`、处理人与往来方收窄并累计各类拆分统计。
    fn extend_scoped(
        &mut self,
        fields: impl IntoIterator<Item = dto::WorkItemFields>,
        query: &dto::WorkItemListQuery,
        keyword: Option<&str>,
        context: &StatsContext<'_>,
    ) {
        self.extend_batch(
            fields
                .into_iter()
                .filter(|item| matches_scope_row(item, query, context.actor.id()))
                .filter(|item| matches_handler(item, &query.handler_user_ids))
                .filter(|item| matches_counterparty(item, &query.counterparty_ids))
                .filter(|item| item.search.matches(keyword).is_some())
                .filter(|item| query.filters.matches_without_item_types(&item.search))
                .map(|item| {
                    let matches_item_types = query.filters.matches_item_types(&item.search);
                    StatIdentity {
                        work_item_type: item.work_item_type,
                        business_object_type: item.business_object_type.clone(),
                        status: item.status,
                        due_at: item.due_at,
                        matches_item_types,
                    }
                }),
            context,
        );
    }

    // 只统计已通过范围及非类型业务筛选的任务；没有办理权限不从本人待办计数中移除。
    fn observe(&mut self, item: StatIdentity, context: &StatsContext<'_>) {
        increment_family_count(&mut self.family_counts, item.work_item_type);
        increment_item_type_counts(
            &mut self.approval_document_type_counts,
            &mut self.item_type_counts,
            item.work_item_type,
            &item.business_object_type,
        );
        let count = self.work_item_type_counts.entry(item.work_item_type).or_default();
        *count = count.saturating_add(1);
        let timing = self.work_item_type_timing.entry(item.work_item_type).or_default();
        let open = item.status == WorkItemStatus::Open;
        let today =
            open && item.due_at.is_some_and(|due| due >= context.today_start && due < context.tomorrow_start);
        let overdue = open && item.due_at.is_some_and(|due| due < context.as_of);
        if today {
            timing.due_today = timing.due_today.saturating_add(1);
        }
        if overdue {
            timing.overdue = timing.overdue.saturating_add(1);
        }

        if !context.selected_types.contains(&item.work_item_type) || !item.matches_item_types {
            return;
        }
        self.selected.total = self.selected.total.saturating_add(1);
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

fn matches_scope_row(item: &dto::WorkItemFields, query: &dto::WorkItemListQuery, actor_id: &str) -> bool {
    match query.scope {
        WorkItemScope::Mine => item.is_pending_for(actor_id),
        WorkItemScope::Managed => item.status == WorkItemStatus::Open,
        WorkItemScope::History => query.statuses.contains(&item.status),
    }
}

fn increment_item_type_counts(
    approval_counts: &mut BTreeMap<String, u64>,
    item_type_counts: &mut BTreeMap<String, u64>,
    work_item_type: WorkItemType,
    business_object_type: &str,
) {
    let key = if work_item_type == WorkItemType::DocumentApproval {
        let approval_key = format!("approval:{business_object_type}");
        let entry = approval_counts.entry(approval_key.clone()).or_default();
        *entry = entry.saturating_add(1);
        approval_key
    } else {
        format!("task:{}", work_item_type.as_str())
    };
    let entry = item_type_counts.entry(key).or_default();
    *entry = entry.saturating_add(1);
}

impl WorkItemReadService {
    /// 查询与正式队列复用同一授权快照的待办统计。
    /// 返回个人待办、当前范围总数、到期、超期、异常、任务族、类型、审批单据拆解及服务端统计时点。
    ///
    /// 查询参数、权限范围或对象事实读取失败时返回错误。
    ///
    /// `params`：责任范围、任务族、类型、状态、时限、关键词、结构化筛选与工作时区。
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

    // 在指定时点按请求范围（mine / managed / history）统计任务分布及明细计数；非个人范围同时保留个人开放待办数 `assigned`。
    async fn stats_at(
        &self,
        query: dto::WorkItemListQuery,
        actor: AuditActor,
        all_families: bool,
        executor: &mut dyn Executor,
    ) -> Result<WorkItemStatsView> {
        let access = QueueAccess::new(self.authorization.query(&actor, &work_item::LIST, executor).await?);
        let as_of = Instant::now();
        let (today_start, tomorrow_start) = business_day_bounds_at(as_of.unix_secs())?;
        let context = StatsContext {
            actor: &actor,
            selected_types: &query.work_item_types,
            as_of,
            today_start,
            tomorrow_start,
        };

        let mut scoped_query = query.clone();
        if !all_families {
            scoped_query.work_item_types = registered_work_item_types();
        }
        let scoped = self.stream_stats(&scoped_query, &access, &context, executor).await?;
        let assigned = if query.scope == WorkItemScope::Mine {
            scoped.selected.total
        } else {
            let mut personal_query = scoped_query.clone();
            personal_query.scope = WorkItemScope::Mine;
            personal_query.statuses = vec![WorkItemStatus::Open];
            personal_query.filters.statuses.clear();
            self.stream_stats(&personal_query, &access, &context, executor).await?.selected.total
        };

        self.validate_access(&access, executor).await?;
        Ok(WorkItemStatsView {
            assigned,
            total: scoped.selected.total,
            due_today: scoped.selected.due_today,
            overdue: scoped.selected.overdue,
            exception: scoped.selected.exception,
            family_counts: scoped.family_counts,
            work_item_type_counts: scoped.work_item_type_counts,
            work_item_type_timing: scoped.work_item_type_timing,
            approval_document_type_counts: scoped.approval_document_type_counts,
            item_type_counts: scoped.item_type_counts,
            as_of,
        })
    }

    // 分批扫描授权任务，完成往来方与结构化筛选后只累计统计计数，不在内存保留全部命中任务。
    async fn stream_stats(
        &self,
        query: &dto::WorkItemListQuery,
        access: &QueueAccess,
        context: &StatsContext<'_>,
        executor: &mut dyn Executor,
    ) -> Result<StatsCounts> {
        let mut filter = access.scope_filter(query)?;
        apply_due_filter_at(&mut filter, query.due, context.as_of)?;
        filter.narrow_current_handlers(&query.handler_user_ids);
        let keyword = filter.query.take();
        let mut counts = StatsCounts::default();

        let mut scan = self.candidate_scan(&filter, executor).await?;
        loop {
            let rows = scan.next_batch(AUTHORIZED_SCAN_BATCH_SIZE, executor).await?;
            let candidate_count = rows.len();
            if candidate_count == 0 {
                break;
            }
            let mut fields = rows.into_iter().map(dto::WorkItemFields::from).collect::<Vec<_>>();
            self.prepare_search_fields(&mut fields, query, access, executor).await?;
            counts.extend_scoped(fields, query, keyword.as_deref(), context);
            if candidate_count < AUTHORIZED_SCAN_BATCH_SIZE.get() as usize {
                break;
            }
        }
        Ok(counts)
    }
}

// 正式列表按调用时点构造到期条件；统计另用固定响应时点。
// 原地写入时限条件；无筛选时保持不变。
//
// 工作时区窗口无法形成时返回错误。
//
// `filter` 为候选范围，`due` 为可选时限筛选。
pub(super) fn apply_due_filter(filter: &mut WorkItemFilter, due: Option<WorkItemDueFilter>) -> Result<()> {
    apply_due_filter_at(filter, due, Instant::now())
}

// 按与响应相同的固定服务端时点计算到期窗口，并在各统计批次使用该窗口，不随扫描耗时漂移。
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

// 按固定时点计算工作时区的今日半开区间。
// 返回今日下界与明日上界。
//
// 工作时区窗口无法形成时返回错误。
//
// 触发 panic 的条件：
// `window.from` 为 `None` 时 panic。`WorkItemDueFilter::Today` 的窗口总会带下界，此分支不应发生。
//
// `now_unix_secs` 为与响应一致的服务端时点。
pub(super) fn business_day_bounds_at(now_unix_secs: i64) -> Result<(Instant, Instant)> {
    let window = WorkItemDueFilter::Today
        .window_at(Instant::from_unix_secs(now_unix_secs))
        .map_err(|error| Error::Internal(error.to_string()))?;
    Ok((window.from.expect("今日窗口必须有下界"), window.before))
}

// 为全族统计取得服务端登记的全部任务种类，不按当前选中的用户分组缩减。
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

// 统计一条任务时通过领域固定映射增加其任务族计数，不从展示名称推断分类。
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
    #[test]
    fn business_day_bounds_use_fixed_asia_shanghai_timezone() {
        let (previous_start, previous_end) = business_day_bounds_at(1_722_441_599).unwrap();
        assert_eq!(previous_start.unix_secs(), 1_722_355_200);
        assert_eq!(previous_end.unix_secs(), 1_722_441_600);

        let (start, end) = business_day_bounds_at(1_722_441_600).unwrap();
        assert_eq!(start.unix_secs(), 1_722_441_600);
        assert_eq!(end.unix_secs(), 1_722_528_000);
    }

    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use erp_core::AccountKind;

    use super::*;

    // 验证：仅有待办列表权限时，审批与采购任务按同一当前分配规则进入列表和统计。
    #[test]
    fn pending_counts_match_assignment_pages_without_business_or_execution_permissions() {
        use bpm::ids::ApprovalNodeExecutionId;
        use erp_core::ids::WorkItemId;

        use crate::dto::work_item::WorkItemListParams;
        use crate::entity::work_item::{DocumentApprovalWorkItemData, WorkItem, WorkItemPriority};
        use crate::service::work_item::read::context::AuthorizedPageCollector;

        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
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
        let context = context(&actor, &query.work_item_types, at);
        let mut counts = StatsCounts::default();
        let indexed = |items: Vec<dto::WorkItemFields>| {
            items
                .into_iter()
                .map(|mut item| {
                    item.search = super::super::search::task_document(&item);
                    item
                })
                .collect::<Vec<_>>()
        };
        // 来源事实缺失或授权未就绪不改变持久化任务数量。
        counts.extend_scoped(indexed(rows.clone()), &query, None, &context);
        assert_eq!(counts.selected.total, 2);
        assert_eq!(counts.family_counts.approval, 1);
        assert_eq!(counts.family_counts.procurement, 1);
        assert_eq!(counts.approval_document_type_counts.get("approval:sales_order"), Some(&1));
        assert_eq!(counts.item_type_counts.get("approval:sales_order"), Some(&1));
        assert_eq!(counts.item_type_counts.get("task:PROCUREMENT_ORDER_CREATION"), Some(&1));
        assert_eq!(counts.selected.due_today, 2);

        for (page, expected) in [(1, vec!["approval"]), (2, vec!["procurement"]), (3, vec![])] {
            let mut collector = AuthorizedPageCollector::new(page, 1).unwrap();
            collector.extend(rows.clone().into_iter().filter(|item| item.is_pending_for(actor.id())));
            let page = collector.finish();
            assert_eq!(u64::try_from(page.total).unwrap(), counts.selected.total);
            assert_eq!(page.items.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(), expected);
        }

        let mut other_handler = query.clone();
        other_handler.handler_user_ids = vec!["other".into()];
        let mut narrowed = StatsCounts::default();
        narrowed.extend_scoped(indexed(rows), &other_handler, None, &context);
        assert_eq!(narrowed.selected.total, 0);
    }

    // 验证：managed/history 范围、关键词 `q`、结构化 `filters`、`counterparty_ids` 及 `approval:<document_type>` 拆分计数。
    #[test]
    fn stats_supports_managed_history_keyword_filters_and_approval_document_breakdown() {
        use bpm::ids::ApprovalNodeExecutionId;
        use erp_core::ids::WorkItemId;

        use crate::entity::work_item::{DocumentApprovalWorkItemData, WorkItem, WorkItemPriority};

        let actor = AuditActor::new("manager".into(), "manager".into(), AccountKind::Admin);
        let at = Instant::from_unix_secs(1_790_784_000);
        let make_approval =
            |id: &str, doc_type: &str, owner: &str, status: WorkItemStatus, party: &str, party_id: &str| {
                let mut fields: dto::WorkItemFields = WorkItem::new_document_approval(
                    WorkItemId::new(id),
                    DocumentApprovalWorkItemData {
                        approval_node_execution_id: ApprovalNodeExecutionId::new(format!("exec-{id}")),
                        business_object_type: doc_type.into(),
                        business_object_id: format!("obj-{id}"),
                        subject_version: "1".into(),
                        owner_role: "approver".into(),
                        owner_organization_id: "org".into(),
                        owner_user_id: owner.into(),
                        priority: WorkItemPriority::Normal,
                        due_at: Some(at),
                    },
                    at,
                )
                .unwrap()
                .into();
                fields.status = status;
                fields.counterparty_label = Some(party.into());
                fields.counterparty_ids = vec![party_id.into()];
                fields.search = super::super::search::task_document(&fields);
                fields
            };

        let rows = vec![
            make_approval("a1", "sales_order", "staff-1", WorkItemStatus::Open, "星辰科技", "cust-1"),
            make_approval("a2", "purchase_order", "staff-2", WorkItemStatus::Open, "星辰供应链", "sup-1"),
            make_approval("a3", "sales_order", "staff-1", WorkItemStatus::Completed, "星辰科技", "cust-1"),
        ];

        // Managed + q 命中往来单位 + filters.item_types 选中 sales_order 时：
        // breakdown 保留所有命中的 approval:<doc_type>，selected.total 只统计选中类型。
        let managed_params: WorkItemStatsParams = serde_json::from_value(serde_json::json!({
            "scope": "managed",
            "q": "星辰",
            "filters": r#"{"item_types":["approval:sales_order"]}"#
        }))
        .unwrap();
        let managed_query = managed_params.normalized().unwrap();
        let ctx = context(&actor, &managed_query.work_item_types, at);
        let mut managed_counts = StatsCounts::default();
        managed_counts.extend_scoped(rows.clone(), &managed_query, managed_query.query.as_deref(), &ctx);
        assert_eq!(managed_counts.selected.total, 1);
        assert_eq!(managed_counts.selected.due_today, 1);
        assert_eq!(managed_counts.family_counts.approval, 2);
        assert_eq!(
            managed_counts.approval_document_type_counts,
            BTreeMap::from([("approval:purchase_order".into(), 1), ("approval:sales_order".into(), 1),])
        );

        // counterparty_ids 精确收窄。
        let cp_params: WorkItemStatsParams = serde_json::from_value(serde_json::json!({
            "scope": "managed",
            "counterparty_ids": "sup-1"
        }))
        .unwrap();
        let cp_query = cp_params.normalized().unwrap();
        let mut cp_counts = StatsCounts::default();
        cp_counts.extend_scoped(rows.clone(), &cp_query, None, &ctx);
        assert_eq!(cp_counts.selected.total, 1);
        assert_eq!(
            cp_counts.approval_document_type_counts,
            BTreeMap::from([("approval:purchase_order".into(), 1)])
        );

        // History 只统计历史终态且不计入 due_today / overdue。
        let history_params: WorkItemStatsParams = serde_json::from_value(serde_json::json!({
            "scope": "history"
        }))
        .unwrap();
        let history_query = history_params.normalized().unwrap();
        let mut history_counts = StatsCounts::default();
        history_counts.extend_scoped(rows, &history_query, None, &ctx);
        assert_eq!(history_counts.selected.total, 1);
        assert_eq!(history_counts.selected.due_today, 0);
        assert_eq!(history_counts.selected.overdue, 0);
        assert_eq!(
            history_counts.approval_document_type_counts,
            BTreeMap::from([("approval:sales_order".into(), 1)])
        );
    }

    // 以固定业务时间构造统计测试上下文，避免运行单测时跨越真实日期或时限。
    fn context<'a>(actor: &'a AuditActor, types: &'a [WorkItemType], as_of: Instant) -> StatsContext<'a> {
        let (today_start, tomorrow_start) = business_day_bounds_at(as_of.unix_secs()).unwrap();
        StatsContext { actor, selected_types: types, as_of, today_start, tomorrow_start }
    }

    // 搭建已明确当前个人分配的最小任务身份，使计数断言只依赖分配及开放状态。
    fn assigned(kind: WorkItemType, due: Option<Instant>) -> StatIdentity {
        StatIdentity {
            work_item_type: kind,
            business_object_type: if kind == WorkItemType::DocumentApproval {
                "sales_order".into()
            } else {
                "object".into()
            },
            status: WorkItemStatus::Open,
            due_at: due,
            matches_item_types: true,
        }
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

    // 验证：全族计数与选定类型并行累计，且消费完每一批后释放原批次。
    #[test]
    fn batches_release_items_and_keep_selected_and_personal_family_counts() {
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let selected_types = [WorkItemType::DocumentApproval];
        let context = context(&actor, &selected_types, Instant::from_unix_secs(1_790_784_000));
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
        assert_eq!(counts.selected.total, 2);
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
        let types = [WorkItemType::DocumentApproval];
        let context = context(&actor, &types, Instant::from_unix_secs(1_790_787_600));
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
        assert_eq!(counts.selected.total, 0);
        assert_eq!(counts.work_item_type_counts[&WorkItemType::SupplierPaymentExecution], 4);
        assert_eq!(
            counts.work_item_type_timing[&WorkItemType::SupplierPaymentExecution],
            WorkItemTypeTiming { due_today: 1, overdue: 2 }
        );
    }

    // 验证：空队列不产生分类，跨越多个页面的任务仍完整累计并返回稳定类型码。
    #[test]
    fn type_counts_cover_multiple_pages_and_empty_queues() {
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let types = [WorkItemType::ProcurementOrderCreation];
        let context = context(&actor, &types, Instant::from_unix_secs(1_790_784_000));
        let mut counts = StatsCounts::default();
        counts.extend_batch([], &context);
        assert!(counts.work_item_type_counts.is_empty());
        for batch_size in [50, 50, 25] {
            counts.extend_batch(
                (0..batch_size).map(|_| assigned(WorkItemType::ProcurementOrderCreation, None)),
                &context,
            );
        }
        assert_eq!(counts.selected.total, 125);
        assert_eq!(counts.family_counts.procurement, 125);
        assert_eq!(
            serde_json::to_value(&counts.work_item_type_counts).unwrap(),
            serde_json::json!({ "PROCUREMENT_ORDER_CREATION": 125 })
        );
    }

    // 验证：今日使用半开窗口，等于统计时点的任务不算逾期，午夜两侧按同一时点判定。
    #[test]
    fn midnight_due_and_overdue_boundaries_use_response_timestamp() {
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let types = [WorkItemType::BusinessException];
        let midnight = Instant::from_unix_secs(1_790_697_600);
        let context = context(&actor, &types, midnight);
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
        assert_eq!(counts.selected.total, 4);
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
