//! 责任队列待办统计。

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_workflow::entity::work_item::{WorkItemStatus, WorkItemType};
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::access::{ActorAccess, ViewAccess, authorized_fields};
use super::query::{AUTHORIZED_SCAN_BATCH_SIZE, matches_keyword, next_candidate_offset};
use super::{
    ProcessingState, WorkItemAllowedAction, WorkItemDueFilter, WorkItemFamily, WorkItemFamilyCountsView,
    WorkItemFilter, WorkItemScope, WorkItemStatsParams, WorkItemStatsView, WorkbenchReadService, dto,
};
use crate::errors::{Error, Result};

impl<A: erp_workflow::WorkflowAuthorizationPort + Clone + Send + Sync + 'static> WorkbenchReadService<A> {
    /// 查询与正式队列复用同一授权快照的待办统计。
    ///
    /// # 参数
    /// * `params` - 责任范围、任务族、类型、时限与工作时区
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回个人、今日到期、超期、异常、任务族计数及服务端统计时点。
    ///
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
            .with_transaction(move |session| {
                Box::pin(async move { this.stats_at(query, actor, all_families, session).await })
            })
            .await
    }

    async fn stats_at(
        &self,
        query: dto::WorkItemListQuery,
        actor: AuditActor,
        all_families: bool,
        executor: &mut dyn Executor,
    ) -> Result<WorkItemStatsView> {
        let access = self.actor_access_for(actor.kind(), actor.id(), executor).await?;

        let selected = self.stats_fields_for_scope(&query, &actor, &access, executor).await?;
        let selected =
            self.processable_stats_fields(selected, query.scope, &actor, &access, executor).await?;
        let assigned =
            self.stats_fields_for_open_scope(&query, WorkItemScope::Mine, &actor, &access, executor).await?;
        let assigned =
            self.processable_stats_fields(assigned, WorkItemScope::Mine, &actor, &access, executor).await?;
        let family_items = if all_families {
            assigned.clone()
        } else {
            let mut family_query = query.clone();
            family_query.work_item_types = registered_work_item_types();
            let family_items = self
                .stats_fields_for_open_scope(&family_query, WorkItemScope::Mine, &actor, &access, executor)
                .await?;
            self.processable_stats_fields(family_items, WorkItemScope::Mine, &actor, &access, executor)
                .await?
        };
        let as_of = Instant::now();
        let (today_start, tomorrow_start) = business_day_bounds()?;
        Ok(WorkItemStatsView {
            assigned: count_u64(assigned.len()),
            due_today: count_u64(
                selected
                    .iter()
                    .filter(|item| item.due_at.is_some_and(|due| due >= today_start && due < tomorrow_start))
                    .count(),
            ),
            overdue: count_u64(
                selected.iter().filter(|item| item.due_at.is_some_and(|due| due < as_of)).count(),
            ),
            exception: count_u64(
                selected
                    .iter()
                    .filter(|item| {
                        matches!(
                            item.work_item_type,
                            WorkItemType::IntegrationResultUnknown | WorkItemType::BusinessException
                        )
                    })
                    .count(),
            ),
            family_counts: family_counts_for_types(family_items.iter().map(|item| item.work_item_type)),
            as_of,
        })
    }

    async fn stats_fields_for_scope(
        &self,
        query: &dto::WorkItemListQuery,
        actor: &AuditActor,
        access: &ActorAccess,
        executor: &mut dyn Executor,
    ) -> Result<Vec<dto::WorkItemFields>> {
        let mut filter = self.scope_filter(query, actor, access)?;
        apply_due_filter(&mut filter, query.due)?;
        self.authorized_stat_fields(filter, access, executor).await
    }

    async fn stats_fields_for_open_scope(
        &self,
        query: &dto::WorkItemListQuery,
        scope: WorkItemScope,
        actor: &AuditActor,
        access: &ActorAccess,
        executor: &mut dyn Executor,
    ) -> Result<Vec<dto::WorkItemFields>> {
        let mut query = query.clone();
        query.scope = scope;
        query.statuses = vec![WorkItemStatus::Open];
        self.stats_fields_for_scope(&query, actor, access, executor).await
    }

    /// 复用正式列表逐任务处理状态与允许动作，只保留当前即可执行的统计项。
    async fn processable_stats_fields(
        &self,
        fields: Vec<dto::WorkItemFields>,
        scope: WorkItemScope,
        actor: &AuditActor,
        access: &ActorAccess,
        executor: &mut dyn Executor,
    ) -> Result<Vec<dto::WorkItemFields>> {
        let mut processable = Vec::with_capacity(fields.len());
        for batch in fields.chunks(100) {
            let views = self
                .project_fields(batch.to_vec(), scope, actor, access, "work-item-stats", executor)
                .await?;
            for (item, view) in batch.iter().zip(views) {
                let access = ViewAccess {
                    processing_state: view.processing_state,
                    processing_blocker: view.processing_blocker,
                    allowed_actions: view.allowed_actions,
                    action_blockers: view.action_blockers,
                };
                if counts_as_processable_stat(scope, &access) {
                    processable.push(item.clone());
                }
            }
        }
        Ok(processable)
    }

    /// 分批读取候选行并在每批对象参与权过滤后累计，禁止使用未授权 repository total。
    async fn authorized_stat_fields(
        &self,
        filter: WorkItemFilter,
        access: &ActorAccess,
        executor: &mut dyn Executor,
    ) -> Result<Vec<dto::WorkItemFields>> {
        let mut fields = Vec::new();
        let mut candidate_offset = 0_u64;
        let mut candidates = filter.clone();
        candidates.query = None;
        loop {
            let rows = self.candidate_batch(&candidates, candidate_offset, executor).await?;
            let candidate_count = rows.len();
            if candidate_count == 0 {
                break;
            }
            let mut facts = self.object_facts_for_rows(&rows, executor).await?;
            self.filter_order_access(&access.actor_id, &mut facts, executor).await?;
            let authorized = authorized_fields(rows, access, &facts);
            fields
                .extend(authorized.into_iter().filter(|item| matches_keyword(item, filter.query.as_deref())));
            candidate_offset = next_candidate_offset(candidate_offset, candidate_count)?;
            if candidate_count < AUTHORIZED_SCAN_BATCH_SIZE.get() as usize {
                break;
            }
        }
        Ok(fields)
    }
}

/// 判断任务是否计入「当前真的能推进」的指标。
///
/// 单据审批任务由审批运行时直接指派，只会拿到 `Approve`/`Reject`——`allowed_actions`
/// 的 `Process` 分支要求 `owner_role` 能过责任范围校验，而审批任务的 `owner_role`
/// 是语义标签（`sales_order_approver`）不是角色 ID，永远过不了。只认 `Process` 会让
/// 待办列表有条目、「待我处理」却是 0。
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

pub(super) fn apply_due_filter(filter: &mut WorkItemFilter, due: Option<WorkItemDueFilter>) -> Result<()> {
    let Some(due) = due else {
        return Ok(());
    };
    let window = due.window_at(Instant::now()).map_err(|error| Error::Internal(error.to_string()))?;
    filter.due_from = window.from;
    filter.due_before = Some(window.before);
    Ok(())
}

fn business_day_bounds() -> Result<(Instant, Instant)> {
    business_day_bounds_at(Instant::now().unix_secs())
}

pub(super) fn business_day_bounds_at(now_unix_secs: i64) -> Result<(Instant, Instant)> {
    let window = WorkItemDueFilter::Today
        .window_at(Instant::from_unix_secs(now_unix_secs))
        .map_err(|error| Error::Internal(error.to_string()))?;
    Ok((window.from.expect("今日窗口必须有下界"), window.before))
}

fn count_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

/// 返回服务端正式注册的全部任务类型，用于形成不受当前分组限制的统计口径。
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

/// 按服务端固定任务族映射汇总可处理任务数量。
pub(super) fn family_counts_for_types(
    work_item_types: impl IntoIterator<Item = WorkItemType>,
) -> WorkItemFamilyCountsView {
    let mut counts = WorkItemFamilyCountsView::default();
    for work_item_type in work_item_types {
        match dto::family_of(work_item_type) {
            WorkItemFamily::Approval => counts.approval = counts.approval.saturating_add(1),
            WorkItemFamily::Procurement => {
                counts.procurement = counts.procurement.saturating_add(1);
            },
            WorkItemFamily::Fulfillment => {
                counts.fulfillment = counts.fulfillment.saturating_add(1);
            },
            WorkItemFamily::Finance => counts.finance = counts.finance.saturating_add(1),
            WorkItemFamily::Exception => counts.exception = counts.exception.saturating_add(1),
        }
    }
    counts
}
