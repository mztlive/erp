//! D03 人工任务责任查询与责任动作编排。
//!
//! 查询范围由认证身份、RBAC 角色与数据范围形成；客户端不能提交任意责任人或
//! 组织扩大范围。正式业务决定继续由各任务类型的强类型命令与审批运行时完成。

use std::{
    collections::{HashMap, HashSet},
    num::NonZeroU32,
};

use chrono::{Datelike, FixedOffset, TimeZone, Utc};
use database::{
    AccessControlExt, ApprovalExt, DocumentRegistryExt, Executor, IntegrationOpsExt, InventoryExt,
    LegacyImportExt, MallSyncExt, MongoCasbinAdapter, NoTransaction, PurchaseOrderExt, ReceivableExt,
    SalesOrderExt, SalesReviewExt, StartProcessingEligibility, StartProcessingOutcome,
    SupplierFulfillmentExt, SupplierOfferingExt, SupplierSettlementExt, Transactional, WorkItemExt,
};
use entities::supplier_offering::{AvailabilityStatus, OfferingStatus};
use entities::{
    access_control::{DataScope, DataScopeSubjectType, DataScopeType},
    approval::{ApprovalInstanceStatus, ApprovalStepStatus},
    common::time::Instant,
    integration_ops::{
        ErrorClass, ErrorTaskStatus, ReconciliationDifferenceId, ReconciliationDifferenceResolution,
        ReconciliationDifferenceResolutionId, ResolutionAction, ResolutionType,
    },
    work_item::{AssignmentMode, WorkItem, WorkItemCloseData, WorkItemStatus, WorkItemType},
    Permission,
};
use mongodb::{bson::doc, Database};
use sha2::{Digest, Sha256};
use validator::Validate;

use crate::{
    approval::ApprovalAssigneeResolver,
    audit::AuditActor,
    errors::{Error, Result},
    iam::SharedRbacService,
};

mod brief;
mod dto;
mod party_names;
mod presentation;
mod procurement_brief;
mod purchase_review_brief;

pub use dto::{
    CloseWorkItemRequest, ProcessingBlockerView, ProcessingState, ReassignWorkItemRequest,
    ReleaseToTeamRequest, StartProcessingRequest, WorkItemAllowedAction, WorkItemConflict,
    WorkItemConflictKind, WorkItemDueFilter, WorkItemFamily, WorkItemListParams, WorkItemMutationOutcome,
    WorkItemPageView, WorkItemPartyView, WorkItemScope, WorkItemSort, WorkItemStatsParams, WorkItemStatsView,
    WorkItemView,
};
pub(crate) use purchase_review_brief::purchase_review_reason_code;

type WorkItemFilter = <mongodb::Database as WorkItemExt>::WorkItemFilter;

const MANAGE_PERMISSION: &str = "work_item:manage";
const IDEMPOTENCY_AUDIT_PREFIX: &str = "work-item-command-";
const COMMAND_FINGERPRINT_PREFIX: &str = "command_sha256=";
const AUTHORIZATION_SNAPSHOT_ATTEMPTS: usize = 3;
const AUTHORIZED_SCAN_BATCH_SIZE: NonZeroU32 = NonZeroU32::new(100).expect("批次大小必须非零");

/// 人工任务责任服务。
pub struct WorkItemService {
    db: Database,
    rbac: SharedRbacService,
}

enum WorkItemWriteOutcome {
    Updated(Box<WorkItem>),
    VersionConflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AssignmentAuthorizationSnapshot {
    policy_revision: u64,
    actor_kind: entities::AccountKind,
    assignee_kind: entities::AccountKind,
    read_permission: Permission,
    actor_read_role_ids: Vec<String>,
    actor_manage_role_ids: Vec<String>,
    assignee_read_role_ids: Vec<String>,
}

struct FocusedQueueContext<'a> {
    scope: WorkItemScope,
    page_size: u32,
    actor: &'a AuditActor,
    access: &'a ActorAccess,
}

#[derive(Debug)]
enum WorkItemWriteError {
    VersionConflict,
    Service(Error),
}

impl From<database::Error> for WorkItemWriteError {
    fn from(error: database::Error) -> Self {
        Self::Service(Error::from(error))
    }
}

impl From<Error> for WorkItemWriteError {
    fn from(error: Error) -> Self {
        Self::Service(error)
    }
}

impl From<entities::Error> for WorkItemWriteError {
    fn from(error: entities::Error) -> Self {
        Self::Service(Error::from(error))
    }
}

/// 只把任务实体自身的 CAS 未命中归类为任务版本冲突。
fn work_item_update_error(error: database::Error) -> WorkItemWriteError {
    match error {
        database::Error::OptimisticLockingError => WorkItemWriteError::VersionConflict,
        error => WorkItemWriteError::Service(Error::from(error)),
    }
}

impl WorkItemService {
    /// 创建服务。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    /// * `rbac` - 共享 RBAC 服务
    ///
    /// # 返回
    /// 返回绑定当前应用授权源的服务。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
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
    pub async fn work_item_list(
        &self,
        params: &WorkItemListParams,
        actor: &AuditActor,
    ) -> Result<WorkItemPageView> {
        params.validate()?;
        let query = params.normalized()?;
        let access = self.actor_access(actor).await?;
        let queue_context_id = queue_context_id(actor.id(), &query, &access);
        ensure_queue_context(&query.queue_context_id, &queue_context_id)?;
        let mut filter = self.scope_filter(&query, actor, &access)?;
        apply_due_filter(&mut filter, query.due)?;
        let authorized_page = self
            .authorized_page_fields(&filter, query.scope, query.page, query.page_size, actor, &access)
            .await?;
        let fields = self
            .focused_fields(
                authorized_page.items,
                query.current_work_item_id.as_deref(),
                &filter,
                FocusedQueueContext {
                    scope: query.scope,
                    page_size: query.page_size,
                    actor,
                    access: &access,
                },
            )
            .await?;
        let items = self
            .project_fields(fields, query.scope, actor, &access, &queue_context_id)
            .await?;
        Ok(WorkItemPageView {
            items,
            total: authorized_page.total,
            page: query.page,
            page_size: query.page_size,
            queue_context_id,
        })
    }

    /// 查询与正式队列复用同一授权快照的待办统计。
    ///
    /// # 参数
    /// * `params` - 责任范围、任务族、类型、时限与工作时区
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回个人、团队、今日到期、超期和异常计数及服务端统计时点。
    ///
    /// # 错误
    /// 查询参数、权限范围或对象事实读取失败时返回错误。
    pub async fn work_item_stats(
        &self,
        params: &WorkItemStatsParams,
        actor: &AuditActor,
    ) -> Result<WorkItemStatsView> {
        params.validate()?;
        let query = params.normalized()?;
        let access = self.actor_access(actor).await?;
        let selected = self.stats_fields_for_scope(&query, actor, &access).await?;
        let selected = self
            .processable_stats_fields(selected, query.scope, actor, &access)
            .await?;
        let assigned = self
            .stats_fields_for_open_scope(&query, WorkItemScope::Mine, actor, &access)
            .await?;
        let assigned = self
            .processable_stats_fields(assigned, WorkItemScope::Mine, actor, &access)
            .await?;
        let team = self
            .stats_fields_for_open_scope(&query, WorkItemScope::Team, actor, &access)
            .await?;
        let team = self
            .processable_stats_fields(team, WorkItemScope::Team, actor, &access)
            .await?;
        let as_of = Instant::now();
        let (today_start, tomorrow_start) = business_day_bounds()?;
        Ok(WorkItemStatsView {
            assigned: count_u64(assigned.len()),
            team: count_u64(team.len()),
            due_today: count_u64(
                selected
                    .iter()
                    .filter(|item| {
                        item.due_at
                            .is_some_and(|due| due >= today_start && due < tomorrow_start)
                    })
                    .count(),
            ),
            overdue: count_u64(
                selected
                    .iter()
                    .filter(|item| item.due_at.is_some_and(|due| due < as_of))
                    .count(),
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
            as_of,
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
        if context.scope == WorkItemScope::Team
            && !self.authoritative_team_candidate(context.actor, &current).await?
        {
            return Err(Error::NotFound("当前焦点任务不在授权队列中".to_string()));
        }
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
        scope: WorkItemScope,
        page: u64,
        page_size: u32,
        actor: &AuditActor,
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
            collector.extend(self.team_candidate_fields(fields, scope, actor).await?);
            candidate_offset = next_candidate_offset(candidate_offset, candidate_count)?;
            if candidate_count < AUTHORIZED_SCAN_BATCH_SIZE.get() as usize {
                break;
            }
        }
        Ok(collector.finish())
    }

    /// 读取一个固定大小候选批次，不执行候选总数计数。
    async fn candidate_batch(
        &self,
        filter: &WorkItemFilter,
        offset: u64,
    ) -> Result<Vec<database::WorkItemRow>> {
        self.db
            .work_items()
            .scan_work_item_batch(filter, offset, AUTHORIZED_SCAN_BATCH_SIZE, &mut NoTransaction)
            .await
            .map_err(Error::from)
    }

    /// 批量读取当前页任务的权威对象事实，避免按行 N+1。
    async fn object_facts_for_rows(&self, rows: &[database::WorkItemRow]) -> Result<ObjectFactMap> {
        let keys = rows
            .iter()
            .filter_map(|row| {
                object_policy(row.work_item_type, &row.business_object_type)
                    .map(|policy| (policy.kind, row.business_object_id.clone()))
            })
            .collect::<HashSet<_>>();
        self.load_object_facts(&keys, &mut NoTransaction).await
    }

    async fn stats_fields_for_scope(
        &self,
        query: &dto::WorkItemListQuery,
        actor: &AuditActor,
        access: &ActorAccess,
    ) -> Result<Vec<dto::WorkItemFields>> {
        let mut filter = self.scope_filter(query, actor, access)?;
        apply_due_filter(&mut filter, query.due)?;
        self.authorized_stat_fields(filter, query.scope, actor, access)
            .await
    }

    async fn stats_fields_for_open_scope(
        &self,
        query: &dto::WorkItemListQuery,
        scope: WorkItemScope,
        actor: &AuditActor,
        access: &ActorAccess,
    ) -> Result<Vec<dto::WorkItemFields>> {
        if scope == WorkItemScope::Team && access.responsibility_scopes.is_empty() {
            return Ok(Vec::new());
        }
        let mut query = query.clone();
        query.scope = scope;
        query.statuses = vec![WorkItemStatus::Open];
        self.stats_fields_for_scope(&query, actor, access).await
    }

    /// 复用正式列表逐任务处理状态与允许动作，只保留当前即可执行的统计项。
    async fn processable_stats_fields(
        &self,
        fields: Vec<dto::WorkItemFields>,
        scope: WorkItemScope,
        actor: &AuditActor,
        access: &ActorAccess,
    ) -> Result<Vec<dto::WorkItemFields>> {
        let mut processable = Vec::with_capacity(fields.len());
        for item in fields {
            let view_access = self.view_access(&item, scope, actor, access).await?;
            if counts_as_processable_stat(scope, &view_access) {
                processable.push(item);
            }
        }
        Ok(processable)
    }

    /// 分批读取候选行并在每批对象参与权过滤后累计，禁止使用未授权 repository total。
    async fn authorized_stat_fields(
        &self,
        filter: WorkItemFilter,
        scope: WorkItemScope,
        actor: &AuditActor,
        access: &ActorAccess,
    ) -> Result<Vec<dto::WorkItemFields>> {
        let mut fields = Vec::new();
        let mut candidate_offset = 0_u64;
        loop {
            let rows = self.candidate_batch(&filter, candidate_offset).await?;
            let candidate_count = rows.len();
            if candidate_count == 0 {
                break;
            }
            let facts = self.object_facts_for_rows(&rows).await?;
            let authorized = authorized_fields(rows, access, &facts);
            fields.extend(self.team_candidate_fields(authorized, scope, actor).await?);
            candidate_offset = next_candidate_offset(candidate_offset, candidate_count)?;
            if candidate_count < AUTHORIZED_SCAN_BATCH_SIZE.get() as usize {
                break;
            }
        }
        Ok(fields)
    }

    /// 团队队列仅保留此刻可按正式责任策略形成本人责任的候选任务。
    async fn team_candidate_fields(
        &self,
        fields: Vec<dto::WorkItemFields>,
        scope: WorkItemScope,
        actor: &AuditActor,
    ) -> Result<Vec<dto::WorkItemFields>> {
        if scope != WorkItemScope::Team || fields.is_empty() {
            return Ok(fields);
        }
        let ids = fields.iter().map(|field| field.id.clone()).collect::<Vec<_>>();
        let items = self
            .db
            .work_items()
            .find_many(doc! { "id": { "$in": ids } }, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|item| (item.base.id.clone(), item))
            .collect::<HashMap<_, _>>();
        let mut eligible_ids = HashSet::new();
        for (id, item) in &items {
            if self.authoritative_team_candidate(actor, item).await? {
                eligible_ids.insert(id.clone());
            }
        }
        Ok(retain_team_eligible_fields(fields, &eligible_ids))
    }

    /// 复用开始处理的权威账号、角色、范围、对象参与权与岗位分离快照。
    async fn authoritative_team_candidate(&self, actor: &AuditActor, item: &WorkItem) -> Result<bool> {
        if item.status != WorkItemStatus::Open
            || item.assignment_mode != AssignmentMode::Pool
            || item.owner_user_id.is_some()
        {
            return Ok(false);
        }
        match self
            .assignment_authorization_snapshot(actor, actor.id(), item, false)
            .await
        {
            Ok(_) => Ok(true),
            Err(error) if is_assignment_candidate_denial(&error) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// 为完整任务列表重新读取对象并执行参与权过滤。
    async fn authorized_fields_for_items(
        &self,
        items: Vec<WorkItem>,
        access: &ActorAccess,
    ) -> Result<Vec<dto::WorkItemFields>> {
        let keys = items
            .iter()
            .filter_map(|item| {
                object_policy(item.work_item_type, &item.business_object_type)
                    .map(|policy| (policy.kind, item.business_object_id.clone()))
            })
            .collect::<HashSet<_>>();
        let facts = self.load_object_facts(&keys, &mut NoTransaction).await?;
        Ok(items
            .into_iter()
            .filter_map(|item| authorized_item_fields(item, access, &facts))
            .collect())
    }

    /// 写命令执行前重验对象存在、阅读权限和参与依据。
    async fn ensure_object_participation(&self, actor: &AuditActor, item: &WorkItem) -> Result<()> {
        let access = self.actor_access(actor).await?;
        self.ensure_item_access(item, &access)
            .await
            .map_err(|_| Error::Forbidden("当前账号无权处理该业务对象".to_string()))
    }

    async fn ensure_item_access(&self, item: &WorkItem, access: &ActorAccess) -> Result<()> {
        if self
            .authorized_fields_for_items(vec![item.clone()], access)
            .await?
            .is_empty()
        {
            return Err(Error::Forbidden("业务对象不可访问".to_string()));
        }
        Ok(())
    }

    /// 按固定对象注册表分组查询；未注册类型不会进入本映射。
    async fn load_object_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        executor: &mut dyn Executor,
    ) -> Result<ObjectFactMap> {
        let mut facts = ObjectFactMap::new();
        self.load_sales_order_facts(keys, &mut facts, executor).await?;
        self.load_procurement_confirmation_facts(keys, &mut facts, executor)
            .await?;
        self.load_purchase_order_facts(keys, &mut facts, executor).await?;
        self.load_sales_change_review_facts(keys, &mut facts, executor)
            .await?;
        self.load_receivable_account_facts(keys, &mut facts, executor)
            .await?;
        self.load_independent_object_facts(keys, &mut facts, executor)
            .await?;
        self.load_master_mapping_task_facts(keys, &mut facts, executor)
            .await?;
        Ok(facts)
    }

    async fn load_sales_order_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SalesOrder);
        if ids.is_empty() {
            return Ok(());
        }
        for order in self
            .db
            .sales_orders()
            .find_many(doc! { "id": { "$in": ids } }, executor)
            .await?
        {
            facts.insert(
                (ObjectKind::SalesOrder, order.base.id.clone()),
                ObjectFact::new(
                    order.base.id.clone(),
                    format!("销售单 {}", order.order_no),
                    order.stable.created_by,
                ),
            );
        }
        Ok(())
    }

    async fn load_sales_change_review_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SalesChangeReview);
        if ids.is_empty() {
            return Ok(());
        }
        let reviews = self
            .db
            .sales_change_reviews()
            .find_many(doc! { "id": { "$in": ids } }, executor)
            .await?;
        let submission_ids = reviews
            .iter()
            .map(|review| review.sales_change_submission_id.to_string())
            .collect::<Vec<_>>();
        let submissions = self
            .db
            .sales_change_submissions()
            .find_many(doc! { "id": { "$in": submission_ids } }, executor)
            .await?
            .into_iter()
            .map(|submission| (submission.base.id.clone(), submission))
            .collect::<HashMap<_, _>>();
        for review in reviews {
            let Some(submission) = submissions.get(&review.sales_change_submission_id.to_string()) else {
                continue;
            };
            facts.insert(
                (ObjectKind::SalesChangeReview, review.base.id.clone()),
                ObjectFact::new(
                    submission.sales_order_id.to_string(),
                    "销售变更复核",
                    submission.submitted_by.clone(),
                ),
            );
        }
        Ok(())
    }

    async fn load_receivable_account_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::ReceivableAccount);
        if ids.is_empty() {
            return Ok(());
        }
        for account in self
            .db
            .receivable_accounts()
            .find_many(doc! { "id": { "$in": ids } }, executor)
            .await?
        {
            facts.insert(
                (ObjectKind::ReceivableAccount, account.base.id.clone()),
                ObjectFact::new(
                    account.sales_order_id.to_string(),
                    format!("卡券应收子账 {}", account.account_seq),
                    account.stable.created_by,
                ),
            );
        }
        Ok(())
    }

    async fn load_independent_object_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.load_stock_adjustment_facts(keys, facts, executor).await?;
        self.load_supplier_settlement_facts(keys, facts, executor).await?;
        self.load_legacy_import_batch_facts(keys, facts, executor).await?;
        self.load_integration_error_task_facts(keys, facts, executor)
            .await?;
        self.load_reconciliation_difference_facts(keys, facts, executor)
            .await?;
        self.load_supplier_fulfillment_order_facts(keys, facts, executor)
            .await?;
        self.load_supplier_offering_facts(keys, facts, executor).await
    }

    async fn load_stock_adjustment_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::StockAdjustment);
        if ids.is_empty() {
            return Ok(());
        }
        for adjustment in self
            .db
            .stock_adjustments()
            .find_many(doc! { "id": { "$in": ids } }, executor)
            .await?
        {
            facts.insert(
                (ObjectKind::StockAdjustment, adjustment.base.id.clone()),
                ObjectFact::new(
                    adjustment.base.id.clone(),
                    format!("库存调整单 {}", adjustment.adjustment_no),
                    adjustment.prepared_by,
                ),
            );
        }
        Ok(())
    }

    async fn load_supplier_settlement_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierSettlement);
        if ids.is_empty() {
            return Ok(());
        }
        for statement in self
            .db
            .supplier_settlement_statements()
            .find_many(doc! { "id": { "$in": ids } }, executor)
            .await?
        {
            facts.insert(
                (ObjectKind::SupplierSettlement, statement.base.id.clone()),
                ObjectFact::new(
                    statement.base.id.clone(),
                    format!("供应商结算单 {}", statement.statement_no),
                    statement.prepared_by,
                ),
            );
        }
        Ok(())
    }

    async fn load_legacy_import_batch_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::LegacyImportBatch);
        if ids.is_empty() {
            return Ok(());
        }
        for batch in self
            .db
            .legacy_import_batches()
            .find_many(doc! { "id": { "$in": ids } }, executor)
            .await?
        {
            facts.insert(
                (ObjectKind::LegacyImportBatch, batch.base.id.clone()),
                ObjectFact::new(
                    batch.base.id.clone(),
                    format!("旧数据导入批次 {}", batch.batch_no),
                    SYSTEM_OBJECT_OWNER,
                ),
            );
        }
        Ok(())
    }

    async fn load_integration_error_task_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::IntegrationErrorTask);
        if ids.is_empty() {
            return Ok(());
        }
        for task in self
            .db
            .integration_error_tasks()
            .find_many(doc! { "id": { "$in": ids } }, executor)
            .await?
        {
            facts.insert(
                (ObjectKind::IntegrationErrorTask, task.base.id.clone()),
                ObjectFact::new(
                    task.base.id.clone(),
                    "集成异常处理",
                    task.owner_user_id
                        .unwrap_or_else(|| SYSTEM_OBJECT_OWNER.to_string()),
                ),
            );
        }
        Ok(())
    }

    async fn load_reconciliation_difference_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::ReconciliationDifference);
        if ids.is_empty() {
            return Ok(());
        }
        for difference in self
            .db
            .reconciliation_differences()
            .find_many(doc! { "id": { "$in": ids } }, executor)
            .await?
        {
            facts.insert(
                (ObjectKind::ReconciliationDifference, difference.base.id.clone()),
                ObjectFact::new(
                    difference.base.id.clone(),
                    format!("对账差异：{}", difference.difference_type),
                    SYSTEM_OBJECT_OWNER,
                ),
            );
        }
        Ok(())
    }

    async fn load_master_mapping_task_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::MasterMappingTask);
        if ids.is_empty() {
            return Ok(());
        }
        for task in self
            .db
            .master_mapping_tasks()
            .find_many(doc! { "id": { "$in": ids } }, executor)
            .await?
        {
            facts.insert(
                (ObjectKind::MasterMappingTask, task.base.id.clone()),
                ObjectFact::new(
                    task.base.id.clone(),
                    format!("{}映射任务", task.mapping_type.label()),
                    task.owner_user_id
                        .unwrap_or_else(|| SYSTEM_OBJECT_OWNER.to_string()),
                ),
            );
        }
        Ok(())
    }

    /// 批量读取 W26 供应商履约订单事实，并冻结订单乐观锁版本用于任务对象校验。
    async fn load_supplier_fulfillment_order_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierFulfillmentOrder);
        if ids.is_empty() {
            return Ok(());
        }
        for order in self
            .db
            .supplier_fulfillment_orders()
            .find_many(doc! { "id": { "$in": ids } }, executor)
            .await?
        {
            facts.insert(
                (ObjectKind::SupplierFulfillmentOrder, order.base.id.clone()),
                ObjectFact {
                    root_document_id: order.mall_order_id.to_string(),
                    label: format!("供应商履约订单 {}", order.fulfillment_order_no),
                    created_by: SYSTEM_OBJECT_OWNER.to_string(),
                    subject_versions: vec![order.base.version.to_string()],
                    counterparty_label: None,
                    impact_summary: None,
                    brief_source: None,
                    subject_briefs: HashMap::new(),
                },
            );
        }
        Ok(())
    }

    /// 批量读取 W21 供应商供给事实；未建模的供应商外部商品继续失败关闭。
    async fn load_supplier_offering_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierOffering);
        if ids.is_empty() {
            return Ok(());
        }
        let offerings = self
            .db
            .supplier_offerings()
            .find_many(doc! { "id": { "$in": ids } }, executor)
            .await?;
        let offering_ids = offerings
            .iter()
            .map(|offering| entities::ids::SupplierOfferingId::new(offering.base.id.clone()))
            .collect::<Vec<_>>();
        let availabilities = self
            .db
            .supplier_offering_availabilities()
            .find_by_offering_ids(&offering_ids, executor)
            .await?
            .into_iter()
            .map(|availability| (availability.supplier_offering_id.to_string(), availability))
            .collect::<HashMap<_, _>>();
        for offering in offerings {
            let availability = availabilities.get(&offering.base.id);
            let mut subject_versions = Vec::with_capacity(2);
            if offering.stable.status == OfferingStatus::Stopped {
                subject_versions.push(format!("offering:{}", offering.base.version));
            }
            if let Some(availability) = availability {
                if availability.availability_status == AvailabilityStatus::Stopped {
                    subject_versions.push(format!("availability:{}", availability.base.version));
                }
            }
            if subject_versions.is_empty() {
                continue;
            }
            facts.insert(
                (ObjectKind::SupplierOffering, offering.base.id.clone()),
                ObjectFact {
                    root_document_id: offering.base.id.clone(),
                    label: format!("供应商供给 {}", offering.supplier_sku_code),
                    created_by: offering.stable.created_by,
                    subject_versions,
                    counterparty_label: None,
                    impact_summary: None,
                    brief_source: None,
                    subject_briefs: HashMap::new(),
                },
            );
        }
        Ok(())
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
            let access = self.view_access(&fields, scope, actor, actor_access).await?;
            items.push(
                WorkItemView::from_fields(fields, queue_context_id.to_string()).with_access(
                    access.processing_state,
                    access.processing_blocker,
                    access.allowed_actions,
                    access.action_blockers,
                ),
            );
        }
        self.apply_party_names(&mut items).await?;
        Ok(items)
    }

    /// 查询当前用户有权查看的单条任务。
    ///
    /// # 错误
    /// 任务不存在或当前用户不在任一安全责任范围时返回错误。
    pub async fn work_item_detail(&self, id: &str, actor: &AuditActor) -> Result<WorkItemView> {
        let item = self.load(id).await?;
        let item_id = item.base.id.clone();
        let access = self.actor_access(actor).await?;
        let scope = detail_scope(&item, actor.id(), &access)?;
        let fields = self.authorized_fields_for_items(vec![item], &access).await?;
        let fields = fields
            .into_iter()
            .next()
            .ok_or_else(|| Error::NotFound("任务或业务对象不可见".to_string()))?;
        let queue_context_id = single_item_context_id(actor.id(), &item_id);
        let view_access = self.view_access(&fields, scope, actor, &access).await?;
        let mut view = WorkItemView::from_fields(fields, queue_context_id).with_access(
            view_access.processing_state,
            view_access.processing_blocker,
            view_access.allowed_actions,
            view_access.action_blockers,
        );
        self.apply_party_names(std::slice::from_mut(&mut view)).await?;
        Ok(view)
    }

    /// 从责任池原子建立本人责任。
    ///
    /// # 错误
    /// 非 POOL 开放任务、资格已失效、处理权冲突或任务版本陈旧时返回错误。
    pub async fn start_processing(
        &self,
        id: &str,
        req: StartProcessingRequest,
        actor: &AuditActor,
    ) -> Result<WorkItemMutationOutcome> {
        req.validate()?;
        let idempotency_key = required_text(&req.idempotency_key, "幂等键不能为空")?;
        let action = "work_item.start_processing";
        let expected_task_version = expected_task_version(&req.expected_task_version)?;
        let version = expected_task_version.to_string();
        let fingerprint = command_fingerprint(&[&version]);
        let audit_id = idempotency_audit_id(actor.id(), action, id, &idempotency_key);
        if let Some(item) = self.idempotent_replay(&audit_id, &fingerprint, id).await? {
            return self.applied_outcome(item, actor).await;
        }
        let current = self.load(id).await?;
        let authorization = self
            .assignment_authorization_snapshot(actor, actor.id(), &current, false)
            .await?;
        let outcome = self
            .start_processing_with_audit(
                current,
                expected_task_version,
                actor,
                action,
                audit_id,
                command_audit_message(&fingerprint, None),
                authorization,
            )
            .await?;
        let item = match outcome {
            StartProcessingOutcome::Started(item) => item,
            StartProcessingOutcome::AlreadyOwned(item) => item,
            StartProcessingOutcome::OwnershipConflict(item) => {
                return self
                    .conflict_outcome(&item.base.id, WorkItemConflictKind::Responsibility, actor)
                    .await;
            }
            StartProcessingOutcome::VersionConflict(item) => {
                return self
                    .conflict_outcome(&item.base.id, WorkItemConflictKind::Version, actor)
                    .await;
            }
            StartProcessingOutcome::NotStartable(None) => {
                return Err(Error::NotFound("任务不存在".to_string()));
            }
            StartProcessingOutcome::NotStartable(Some(item)) => {
                if item.base.version != expected_task_version {
                    return self
                        .conflict_outcome(&item.base.id, WorkItemConflictKind::Version, actor)
                        .await;
                }
                return Err(Error::BusinessLogicError(format!(
                    "当前任务不可开始处理（状态 {}，任务版本 {}）",
                    item.status.as_str(),
                    item.base.version
                )));
            }
        };
        self.applied_outcome(item, actor).await
    }

    /// 将本人负责的开放 POOL 任务退回团队。
    ///
    /// # 错误
    /// 非当前责任人、任务受阻、原因非法或任务版本陈旧时返回错误。
    pub async fn release_to_team(
        &self,
        id: &str,
        req: ReleaseToTeamRequest,
        actor: &AuditActor,
    ) -> Result<WorkItemMutationOutcome> {
        req.validate()?;
        let idempotency_key = required_text(&req.idempotency_key, "幂等键不能为空")?;
        let action = "work_item.release_to_team";
        let reason = required_text(&req.reason, "退回原因不能为空")?;
        let expected_task_version = expected_task_version(&req.expected_task_version)?;
        let version = expected_task_version.to_string();
        let fingerprint = command_fingerprint(&[&version, &reason]);
        let audit_id = idempotency_audit_id(actor.id(), action, id, &idempotency_key);
        if let Some(item) = self.idempotent_replay(&audit_id, &fingerprint, id).await? {
            return self.applied_outcome(item, actor).await;
        }
        let item = self.load(id).await?;
        if item.base.version != expected_task_version {
            return self
                .conflict_outcome(id, WorkItemConflictKind::Version, actor)
                .await;
        }
        if !item.is_owned_by(actor.id()) {
            return self
                .conflict_outcome(id, WorkItemConflictKind::Responsibility, actor)
                .await;
        }
        let authorization = self
            .assignment_authorization_snapshot(actor, actor.id(), &item, false)
            .await?;
        self.ensure_approval_not_blocked(&item).await?;
        let updated = self
            .release_with_assignment_policy_audit(
                item,
                expected_task_version,
                actor,
                action,
                audit_id,
                command_audit_message(&fingerprint, Some(&reason)),
                authorization,
            )
            .await?;
        match updated {
            WorkItemWriteOutcome::Updated(item) => self.applied_outcome(*item, actor).await,
            WorkItemWriteOutcome::VersionConflict => {
                self.conflict_outcome(id, WorkItemConflictKind::Version, actor)
                    .await
            }
        }
    }

    /// 受控转交开放任务。
    ///
    /// # 错误
    /// 缺少任务管理权限、目标资格无法证明、审批受阻或任务版本陈旧时返回错误。
    pub async fn reassign(
        &self,
        id: &str,
        req: ReassignWorkItemRequest,
        actor: &AuditActor,
    ) -> Result<WorkItemMutationOutcome> {
        req.validate()?;
        let managed_access = self.managed_access(actor).await?;
        let idempotency_key = required_text(&req.idempotency_key, "幂等键不能为空")?;
        let action = "work_item.reassign";
        let target_user_id = required_text(&req.target_user_id, "目标用户不能为空")?;
        let reason = required_text(&req.reason, "转交原因不能为空")?;
        let expected_task_version = expected_task_version(&req.expected_task_version)?;
        let version = expected_task_version.to_string();
        let fingerprint = command_fingerprint(&[&version, &target_user_id, &reason]);
        let audit_id = idempotency_audit_id(actor.id(), action, id, &idempotency_key);
        if let Some(item) = self.idempotent_replay(&audit_id, &fingerprint, id).await? {
            return self.applied_outcome(item, actor).await;
        }
        let item = self.load(id).await?;
        if item.base.version != expected_task_version {
            return self
                .conflict_outcome(id, WorkItemConflictKind::Version, actor)
                .await;
        }
        ensure_item_in_managed_scope(&item, &managed_access)?;
        let authorization = self
            .assignment_authorization_snapshot(actor, &target_user_id, &item, true)
            .await?;
        let updated = self
            .reassign_with_assignment_policy_audit(
                item,
                expected_task_version,
                target_user_id,
                actor,
                action,
                audit_id,
                command_audit_message(&fingerprint, Some(&reason)),
                authorization,
            )
            .await?;
        match updated {
            WorkItemWriteOutcome::Updated(item) => self.applied_outcome(*item, actor).await,
            WorkItemWriteOutcome::VersionConflict => {
                self.conflict_outcome(id, WorkItemConflictKind::Version, actor)
                    .await
            }
        }
    }

    /// 关闭重复、误派或已有有效替代任务。
    ///
    /// # 错误
    /// 缺少管理权限、任务类型禁止通用关闭、原因非法或版本陈旧时返回错误。
    pub async fn close(
        &self,
        id: &str,
        req: CloseWorkItemRequest,
        actor: &AuditActor,
    ) -> Result<WorkItemMutationOutcome> {
        req.validate()?;
        let managed_access = self.managed_access(actor).await?;
        let idempotency_key = required_text(&req.idempotency_key, "幂等键不能为空")?;
        let action = "work_item.close";
        let reason_code = required_text(&req.reason_code, "关闭原因代码不能为空")?;
        let replacement_id = req
            .replacement_work_item_id
            .as_deref()
            .map(|value| required_text(value, "替代任务ID不能为空"))
            .transpose()?;
        let close_reason = w29_close_reason(&reason_code, req.comment.as_deref(), replacement_id.as_deref())?;
        let expected_task_version = expected_task_version(&req.expected_task_version)?;
        let version = expected_task_version.to_string();
        let fingerprint = command_fingerprint(&[
            &version,
            &close_reason,
            replacement_id.as_deref().unwrap_or_default(),
        ]);
        let audit_id = idempotency_audit_id(actor.id(), action, id, &idempotency_key);
        if let Some(item) = self.idempotent_replay(&audit_id, &fingerprint, id).await? {
            return self.applied_outcome(item, actor).await;
        }
        let item = self.load(id).await?;
        if item.base.version != expected_task_version {
            return self
                .conflict_outcome(id, WorkItemConflictKind::Version, actor)
                .await;
        }
        ensure_item_in_managed_scope(&item, &managed_access)?;
        self.ensure_object_participation(actor, &item).await?;
        if !is_w29_closable(&item) {
            return Err(Error::BusinessLogicError(
                "只有 W29 登记的异常任务允许受控关闭".to_string(),
            ));
        }
        if let Some(replacement_id) = replacement_id.as_deref() {
            self.ensure_w29_replacement(&item, replacement_id, actor, &managed_access)
                .await?;
        }
        let updated = self
            .close_with_domain_evidence(
                item,
                actor,
                &reason_code,
                replacement_id.as_deref(),
                action,
                audit_id,
                command_audit_message(&fingerprint, Some(&close_reason)),
                close_reason,
            )
            .await?;
        match updated {
            WorkItemWriteOutcome::Updated(item) => self.applied_outcome(*item, actor).await,
            WorkItemWriteOutcome::VersionConflict => {
                self.conflict_outcome(id, WorkItemConflictKind::Version, actor)
                    .await
            }
        }
    }

    async fn load(&self, id: &str) -> Result<WorkItem> {
        self.db
            .work_items()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("任务不存在".to_string()))
    }

    /// 校验重复关闭所引用的替代任务仍是同类、开放且位于当前管理范围。
    async fn ensure_w29_replacement(
        &self,
        current: &WorkItem,
        replacement_id: &str,
        actor: &AuditActor,
        access: &ActorAccess,
    ) -> Result<()> {
        if replacement_id == current.base.id {
            return Err(Error::ValidationError("替代任务不能引用自身".to_string()));
        }
        let replacement = self.load(replacement_id).await?;
        if replacement.status != WorkItemStatus::Open
            || !is_w29_closable(&replacement)
            || replacement.work_item_type != current.work_item_type
            || replacement.business_object_type != current.business_object_type
        {
            return Err(Error::ConflictError(
                "替代任务必须是同一 W29 对象类别的开放正式任务".to_string(),
            ));
        }
        ensure_item_in_managed_scope(&replacement, access)?;
        self.ensure_object_participation(actor, &replacement).await
    }

    async fn actor_access(&self, actor: &AuditActor) -> Result<ActorAccess> {
        self.actor_access_for(actor.kind(), actor.id()).await
    }

    async fn actor_access_for(
        &self,
        account_kind: entities::AccountKind,
        actor_id: &str,
    ) -> Result<ActorAccess> {
        let role_ids = self.rbac.role_ids(account_kind, actor_id).await?;
        let permissions = self.rbac.permissions(account_kind, actor_id).await?;
        let participant_document_ids = self
            .db
            .document_participants()
            .list_by_user(actor_id, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|participant| participant.document_id.to_string())
            .collect();
        let manage_permission = Permission::parse(MANAGE_PERMISSION).expect("固定权限合法");
        let has_manage_permission = permissions
            .iter()
            .any(|permission| permission.covers(&manage_permission));
        let manage_role_ids = self
            .roles_granting_permission(&role_ids, &manage_permission, has_manage_permission)
            .await?;
        let (organization_ids, responsibility_scopes) = self
            .actor_scope_access(actor_id, &role_ids, &manage_role_ids)
            .await?;
        Ok(ActorAccess {
            actor_id: actor_id.to_string(),
            permissions,
            participant_document_ids,
            can_manage: !manage_role_ids.is_empty(),
            organization_ids,
            responsibility_scopes,
        })
    }

    /// 定位实际授予指定权限的角色，使管理数据范围与权限来源关联。
    async fn roles_granting_permission(
        &self,
        role_ids: &[String],
        permission: &Permission,
        account_has_permission: bool,
    ) -> Result<Vec<String>> {
        if !account_has_permission {
            return Ok(Vec::new());
        }
        let mut granting_roles = Vec::new();
        for role_id in role_ids {
            if self.rbac.enforce(&format!("role:{role_id}"), permission).await? {
                granting_roles.push(role_id.clone());
            }
        }
        Ok(granting_roles)
    }

    /// 分别保留每个角色的组织授权，避免多角色范围交叉放大。
    async fn actor_scope_access(
        &self,
        actor_id: &str,
        role_ids: &[String],
        manage_role_ids: &[String],
    ) -> Result<(Vec<String>, Vec<(String, Option<String>)>)> {
        let user_scopes = self
            .db
            .data_scopes()
            .list_by_subject(DataScopeSubjectType::User, actor_id, &mut NoTransaction)
            .await?;
        let mut responsibility_scopes = Vec::new();
        let mut management_scopes = Vec::new();
        for role_id in role_ids {
            let role_scopes = self
                .db
                .data_scopes()
                .list_by_subject(DataScopeSubjectType::Role, role_id, &mut NoTransaction)
                .await?;
            responsibility_scopes.extend(responsibility_pairs(role_id, &role_scopes, &user_scopes));
            if manage_role_ids.contains(role_id) {
                management_scopes.extend(responsibility_pairs(role_id, &role_scopes, &user_scopes));
            }
        }
        responsibility_scopes.sort();
        responsibility_scopes.dedup();
        Ok((
            organizations_from_pairs(&management_scopes),
            responsibility_scopes,
        ))
    }

    fn scope_filter(
        &self,
        query: &dto::WorkItemListQuery,
        actor: &AuditActor,
        access: &ActorAccess,
    ) -> Result<WorkItemFilter> {
        let mut filter = WorkItemFilter {
            work_item_types: query.work_item_types.clone(),
            statuses: query.statuses.clone(),
            priorities: query.priorities.clone(),
            query: query.query.clone(),
            object_access_shapes: Some(object_access_shapes(access)),
            page: query.page,
            page_size: query.page_size,
            sort_by: Some(query.sort_by.to_string()),
            sort_ascending: query.sort_ascending,
            ..WorkItemFilter::default()
        };
        match query.scope {
            WorkItemScope::Mine => filter.owner_user_id = Some(actor.id().to_string()),
            WorkItemScope::Team => {
                ensure_team_access(access)?;
                filter.assignment_mode = Some(AssignmentMode::Pool);
                filter.unassigned_only = true;
                filter.responsibility_scopes = access.responsibility_scopes.clone();
            }
            WorkItemScope::Managed => {
                ensure_managed_access(access)?;
                filter.owner_organization_ids = organization_filter(access);
            }
            WorkItemScope::History => {
                filter.history_actor_id = Some(actor.id().to_string());
                if access.can_manage && !access.organization_ids.is_empty() {
                    filter.history_managed_organization_ids = Some(organization_filter(access));
                }
            }
        }
        Ok(filter)
    }

    async fn view_access(
        &self,
        item: &dto::WorkItemFields,
        scope: WorkItemScope,
        actor: &AuditActor,
        access: &ActorAccess,
    ) -> Result<ViewAccess> {
        if let Some(blocker) = self
            .processing_blocker(item.approval_step_instance_id.as_deref())
            .await?
        {
            return Ok(ViewAccess::blocked(blocker));
        }
        if scope == WorkItemScope::History || item.status != WorkItemStatus::Open {
            return Ok(ViewAccess::ready(Vec::new()));
        }
        let team_candidate_eligible = if scope == WorkItemScope::Team && item.owner_user_id.is_none() {
            match self.load(&item.id).await {
                Ok(current) => self.authoritative_team_candidate(actor, &current).await?,
                Err(Error::NotFound(_)) => false,
                Err(error) => return Err(error),
            }
        } else {
            false
        };
        let actions = allowed_actions(item, scope, actor.id(), access, team_candidate_eligible);
        Ok(ViewAccess::ready(actions))
    }

    async fn processing_blocker(&self, step_id: Option<&str>) -> Result<Option<ProcessingBlockerView>> {
        let Some(step_id) = step_id else {
            return Ok(None);
        };
        let Some(step) = self
            .db
            .approval_step_instances()
            .find_by_id(step_id, &mut NoTransaction)
            .await?
        else {
            return Ok(Some(blocker(
                "APPROVAL_STEP_MISSING",
                "审批责任记录不完整，请联系管理员。",
            )));
        };
        if step.status != ApprovalStepStatus::Blocked {
            return Ok(None);
        }
        Ok(Some(blocker(
            step.blocker_code.as_deref().unwrap_or("APPROVAL_BLOCKED"),
            "审批当前受阻，请等待管理员恢复。",
        )))
    }

    async fn ensure_approval_not_blocked(&self, item: &WorkItem) -> Result<()> {
        let Some(step_id) = item.approval_step_instance_id.as_deref() else {
            return Ok(());
        };
        let step = self
            .db
            .approval_step_instances()
            .find_by_id(step_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("审批责任记录不完整，请联系管理员。".to_string()))?;
        if step.status == ApprovalStepStatus::Blocked {
            return Err(Error::BusinessLogicError(
                "审批当前受阻，请等待管理员恢复。".to_string(),
            ));
        }
        Ok(())
    }

    /// 形成事务外的授权版本锚点；事务内仍会重新读取全部资格与业务事实。
    async fn assignment_authorization_snapshot(
        &self,
        actor: &AuditActor,
        assignee_id: &str,
        item: &WorkItem,
        require_manager: bool,
    ) -> Result<AssignmentAuthorizationSnapshot> {
        let policy = object_policy(item.work_item_type, &item.business_object_type)
            .ok_or_else(|| Error::Forbidden("任务类型未注册责任策略".to_string()))?;
        let read_permission = Permission::parse(policy.read_permission).expect("责任策略权限必须合法");
        let manage_permission = Permission::parse(MANAGE_PERMISSION).expect("固定权限合法");
        let adapter = MongoCasbinAdapter::new(self.db.clone());
        for _ in 0..AUTHORIZATION_SNAPSHOT_ATTEMPTS {
            let before = adapter.policy_revision(&mut NoTransaction).await?;
            let actor_role_ids =
                active_role_ids(&self.db, actor.kind(), actor.id(), &mut NoTransaction).await?;
            let actor_read_role_ids = self
                .roles_granting_permission(&actor_role_ids, &read_permission, true)
                .await?;
            let actor_manage_role_ids = self
                .roles_granting_permission(&actor_role_ids, &manage_permission, true)
                .await?;
            let assignee = self
                .db
                .accounts()
                .find_by_id(assignee_id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::Forbidden("目标账号不存在或已失效".to_string()))?;
            let assignee_role_ids =
                active_role_ids(&self.db, assignee.kind, assignee_id, &mut NoTransaction).await?;
            let assignee_read_role_ids = self
                .roles_granting_permission(&assignee_role_ids, &read_permission, true)
                .await?;
            let snapshot = AssignmentAuthorizationSnapshot {
                policy_revision: before,
                actor_kind: actor.kind(),
                assignee_kind: assignee.kind,
                read_permission: read_permission.clone(),
                actor_read_role_ids,
                actor_manage_role_ids,
                assignee_read_role_ids,
            };
            self.ensure_assignment_actor_access(
                actor.kind(),
                actor.id(),
                item,
                require_manager,
                &snapshot,
                &mut NoTransaction,
            )
            .await?;
            self.ensure_assignment_candidate(
                assignee_id,
                assignee.kind,
                item,
                &snapshot,
                item.owner_user_id.as_deref() == Some(assignee_id),
                &mut NoTransaction,
            )
            .await?;
            let after = adapter.policy_revision(&mut NoTransaction).await?;
            if before == after {
                return Ok(snapshot);
            }
        }
        Err(Error::Rbac(
            "授权策略持续变化，无法形成稳定的任务分派快照".to_string(),
        ))
    }

    /// 在调用方事务快照中重验操作人权限、管理范围与对象参与权。
    async fn ensure_assignment_actor_access(
        &self,
        actor_kind: entities::AccountKind,
        actor_id: &str,
        item: &WorkItem,
        require_manager: bool,
        authorization: &AssignmentAuthorizationSnapshot,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db
            .accounts()
            .find_by_id(actor_id, executor)
            .await?
            .filter(|account| account.kind == actor_kind && account.can_login())
            .ok_or_else(|| Error::Forbidden("操作账号不存在、已停用或身份已变化".to_string()))?;
        let access = self
            .assignment_access_for_executor(
                actor_kind,
                actor_id,
                &authorization.read_permission,
                &authorization.actor_read_role_ids,
                &authorization.actor_manage_role_ids,
                executor,
            )
            .await?;
        if require_manager {
            ensure_managed_access(&access)?;
            ensure_item_in_managed_scope(item, &access)?;
        }
        self.ensure_item_access_with_executor(item, &access, executor)
            .await
            .map_err(|_| Error::Forbidden("当前账号无权处理该业务对象".to_string()))
    }

    /// 在调用方事务快照中重验目标账号资格、对象参与权与审批岗位分离。
    async fn ensure_assignment_candidate(
        &self,
        user_id: &str,
        expected_kind: entities::AccountKind,
        item: &WorkItem,
        authorization: &AssignmentAuthorizationSnapshot,
        allow_current_owner: bool,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db
            .accounts()
            .find_by_id(user_id, executor)
            .await?
            .filter(|account| account.kind == expected_kind && account.can_login())
            .ok_or_else(|| Error::Forbidden("目标账号不存在或已失效".to_string()))?;
        let eligible = ApprovalAssigneeResolver::new(self.db.clone())
            .user_is_eligible_for_assignment(user_id, &item.owner_role, &item.owner_organization_id, executor)
            .await?;
        if !eligible {
            return Err(Error::Forbidden(
                "目标账号已不具备该任务角色或组织范围资格".to_string(),
            ));
        }
        let access = self
            .assignment_access_for_executor(
                expected_kind,
                user_id,
                &authorization.read_permission,
                &authorization.assignee_read_role_ids,
                &[],
                executor,
            )
            .await?;
        self.ensure_item_access_with_executor(item, &access, executor)
            .await
            .map_err(|_| Error::Forbidden("目标账号不具备该业务对象的参与权或读取权".to_string()))?;
        self.ensure_assignment_separation(user_id, item, allow_current_owner, executor)
            .await
    }

    /// 审批任务在形成个人责任前排除启动人、既往责任人、前序和当前决定人。
    async fn ensure_assignment_separation(
        &self,
        user_id: &str,
        item: &WorkItem,
        allow_current_owner: bool,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        match assignment_separation_policy(item.work_item_type) {
            AssignmentSeparationPolicy::ApprovalHistory => {
                self.ensure_approval_assignment_separation(user_id, item, allow_current_owner, executor)
                    .await
            }
            AssignmentSeparationPolicy::DomainActors => {
                let excluded = self.domain_assignment_actors(item, executor).await?;
                if excluded.iter().any(|actor_id| actor_id == user_id) {
                    return Err(Error::Forbidden("目标账号违反业务岗位分离约束".to_string()));
                }
                Ok(())
            }
            AssignmentSeparationPolicy::RoleAndParticipation => Ok(()),
            AssignmentSeparationPolicy::FailClosed => {
                Err(Error::Forbidden("任务类型未注册可证明的岗位分离策略".to_string()))
            }
        }
    }

    /// 审批步骤按实例启动人、业务提交人、责任历史和决定历史排除候选人。
    async fn ensure_approval_assignment_separation(
        &self,
        user_id: &str,
        item: &WorkItem,
        allow_current_owner: bool,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let step_id = item
            .approval_step_instance_id
            .as_deref()
            .ok_or_else(|| Error::Forbidden("审批任务缺少审批步骤责任事实".to_string()))?;
        let policy = object_policy(item.work_item_type, &item.business_object_type)
            .ok_or_else(|| Error::Forbidden("审批任务类型未注册责任策略".to_string()))?;
        let keys = HashSet::from([(policy.kind, item.business_object_id.clone())]);
        let facts = self.load_object_facts(&keys, executor).await?;
        let fact = facts
            .get(&(policy.kind, item.business_object_id.clone()))
            .ok_or_else(|| Error::Forbidden("审批业务对象事实缺失".to_string()))?;
        let step = self
            .db
            .approval_step_instances()
            .find_by_id(step_id, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("审批步骤责任事实缺失，请刷新".to_string()))?;
        let instance = self
            .db
            .approval_instances()
            .find_by_id(step.approval_instance_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::ConflictError("审批实例责任事实缺失，请刷新".to_string()))?;
        if instance.status != ApprovalInstanceStatus::Running
            || step.status != ApprovalStepStatus::Active
            || instance.current_step_instance_id.as_ref().map(AsRef::as_ref) != Some(step_id)
        {
            return Err(Error::ConflictError("审批步骤已不再可分派，请刷新".to_string()));
        }
        let steps = self
            .db
            .approval_step_instances()
            .list_by_instance(&step.approval_instance_id, executor)
            .await?;
        let separated = approval_assignment_separated(
            user_id,
            &instance.started_by,
            &fact.created_by,
            &item.responsibility_actor_ids,
            item.owner_user_id.as_deref(),
            allow_current_owner,
            &steps,
        );
        if !separated {
            return Err(Error::Forbidden("目标账号违反审批岗位分离约束".to_string()));
        }
        Ok(())
    }

    /// 读取非审批正式决定任务的权威提交人、经办人及历史决定人。
    async fn domain_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<HashSet<String>> {
        let actors = match item.work_item_type {
            WorkItemType::LowMarginManagerConfirmation => {
                self.low_margin_assignment_actors(item, executor).await?
            }
            WorkItemType::ProcurementConfirmation => {
                self.procurement_assignment_actors(item, executor).await?
            }
            WorkItemType::PurchaseOrderReview => {
                self.purchase_review_assignment_actors(item, executor).await?
            }
            WorkItemType::SalesChangeImpactReview | WorkItemType::SalesChangeFinanceReview => {
                self.sales_change_assignment_actors(item, executor).await?
            }
            WorkItemType::CardFundsReview | WorkItemType::CardFundsDeltaReview => {
                self.card_funds_assignment_actors(item, executor).await?
            }
            WorkItemType::InventoryAdjustmentReview => {
                self.inventory_assignment_actors(item, executor).await?
            }
            WorkItemType::SupplierSettlementReview => {
                self.settlement_assignment_actors(item, executor).await?
            }
            _ => {
                return Err(Error::Forbidden("任务类型未注册权威业务岗位分离事实".to_string()));
            }
        };
        non_empty_assignment_actors(actors)
    }

    async fn low_margin_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let submission = self
            .db
            .sales_order_submissions()
            .find_by_id(&item.subject_version, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("低毛利提交事实缺失".to_string()))?;
        let confirmation = self
            .db
            .low_margin_manager_confirmations()
            .find_one(
                doc! {
                    "sales_order_id": &item.business_object_id,
                    "low_margin_submission_id": &item.subject_version,
                },
                executor,
            )
            .await?
            .ok_or_else(|| Error::Forbidden("低毛利上级确认事实缺失".to_string()))?;
        Ok(optional_actors([
            Some(submission.submitted_by),
            Some(confirmation.requested_by),
            confirmation.decided_by,
        ]))
    }

    async fn procurement_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let confirmation = self
            .db
            .procurement_confirmations()
            .find_by_id(&item.business_object_id, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("采购确认事实缺失".to_string()))?;
        if confirmation.submission_id.as_ref() != item.subject_version {
            return Err(Error::Forbidden("采购确认提交版本与任务不一致".to_string()));
        }
        let submission = self
            .db
            .sales_order_submissions()
            .find_by_id(confirmation.submission_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::Forbidden("销售提交事实缺失".to_string()))?;
        Ok(optional_actors([
            Some(submission.submitted_by),
            Some(confirmation.stable.created_by),
            confirmation.handled_by,
        ]))
    }

    async fn purchase_review_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let submission = self
            .db
            .purchase_order_submissions()
            .find_by_id(&item.subject_version, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("采购提交事实缺失".to_string()))?;
        if submission.purchase_order_id.as_ref() != item.business_object_id {
            return Err(Error::Forbidden("采购提交与任务对象不一致".to_string()));
        }
        let submitted_by = submission
            .submitted_by
            .ok_or_else(|| Error::Forbidden("采购提交人事实缺失".to_string()))?;
        Ok(optional_actors([Some(submitted_by), submission.reviewed_by]))
    }

    async fn sales_change_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let current = self
            .db
            .sales_change_reviews()
            .find_by_id(&item.business_object_id, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("销售变更复核事实缺失".to_string()))?;
        if current.sales_change_submission_id.as_ref() != item.subject_version {
            return Err(Error::Forbidden("销售变更提交版本与任务不一致".to_string()));
        }
        let submission = self
            .db
            .sales_change_submissions()
            .find_by_id(&item.subject_version, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("销售变更提交事实缺失".to_string()))?;
        let reviews = self
            .db
            .sales_change_reviews()
            .find_many(
                doc! { "sales_change_submission_id": &item.subject_version },
                executor,
            )
            .await?;
        let mut actors = vec![submission.submitted_by, current.stable.created_by];
        actors.extend(reviews.into_iter().filter_map(|review| review.reviewer_id));
        Ok(actors)
    }

    async fn card_funds_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        if item.approval_step_instance_id.is_some() || item.business_object_type != "receivable_account" {
            return Err(Error::Forbidden("卡券票款任务责任事实不合法".to_string()));
        }
        let account = self
            .db
            .receivable_accounts()
            .find_by_id(&item.business_object_id, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("应收子账事实缺失".to_string()))?;
        let expected_status = match item.work_item_type {
            WorkItemType::CardFundsReview => entities::receivable::AccountReviewStatus::OpeningPending,
            WorkItemType::CardFundsDeltaReview => entities::receivable::AccountReviewStatus::SyncDeltaPending,
            _ => return Err(Error::Forbidden("任务类型不是卡券票款复核".to_string())),
        };
        if account.review_status != expected_status {
            return Err(Error::Forbidden("应收子账已不在当前票款复核状态".to_string()));
        }
        self.ensure_current_card_funds_subject(item, &account, executor)
            .await?;
        let account_id = entities::ids::ReceivableAccountId::new(account.base.id.clone());
        let entries = self
            .db
            .receivable_entries()
            .find_entries_by_account(&account_id, executor)
            .await?;
        let entry_ids = entries
            .iter()
            .map(|entry| entities::ids::ReceivableEntryId::new(entry.base.id.clone()))
            .collect::<Vec<_>>();
        let receipt_allocations = self
            .db
            .receipt_allocations()
            .find_allocations_by_entries(&entry_ids, executor)
            .await?;
        let invoice_allocations = self
            .db
            .sales_invoice_allocations()
            .find_allocations_by_accounts(&[account_id], executor)
            .await?;
        let receipt_ids = receipt_allocations
            .into_iter()
            .map(|allocation| allocation.customer_receipt_id.to_string())
            .collect::<HashSet<_>>();
        let invoice_ids = invoice_allocations
            .into_iter()
            .map(|allocation| allocation.invoice_id.to_string())
            .collect::<HashSet<_>>();
        let receipts = self
            .card_funds_receipts(&receipt_ids, &account.counterparty_party_id, executor)
            .await?;
        let invoices = self
            .card_funds_invoices(&invoice_ids, &account.counterparty_party_id, executor)
            .await?;
        let reviews = self
            .db
            .receivable_funds_reviews()
            .find_reviews_by_account(
                &entities::ids::ReceivableAccountId::new(account.base.id.clone()),
                executor,
            )
            .await?;
        let mut actors = optional_actors([Some(account.stable.created_by), account.reviewed_by]);
        actors.extend(reviews.into_iter().map(|review| review.reviewed_by));
        actors.extend(
            self.card_funds_audit_actors(
                "customer_receipt",
                &receipts,
                &["customer_receipt.create", "customer_receipt.post:"],
                &["customer_receipt.post:"],
                executor,
            )
            .await?,
        );
        actors.extend(
            self.card_funds_audit_actors(
                "invoice",
                &invoices,
                &["invoice.create", "invoice.post", "invoice.red_issue"],
                &["invoice.post", "invoice.red_issue"],
                executor,
            )
            .await?,
        );
        Ok(actors)
    }

    /// 锁定应收来源销售单的当前正式版本，拒绝把旧版本任务重新形成个人责任。
    async fn ensure_current_card_funds_subject(
        &self,
        item: &WorkItem,
        account: &entities::receivable::ReceivableAccount,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let order = self
            .db
            .sales_orders()
            .find_by_id(account.sales_order_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::Forbidden("应收子账来源销售单事实缺失".to_string()))?;
        let revision_id = order
            .stable
            .current_revision_id
            .ok_or_else(|| Error::Forbidden("应收子账来源销售单缺少当前正式版本".to_string()))?;
        if revision_id != item.subject_version {
            return Err(Error::Forbidden("票款复核任务已不是当前销售版本".to_string()));
        }
        self.db
            .sales_order_revisions()
            .find_by_id(&revision_id, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("应收子账来源销售单当前正式版本事实缺失".to_string()))?;
        Ok(())
    }

    /// 读取当前票款快照引用的正式回款；缺失、未过账或主体不一致均失败关闭。
    async fn card_funds_receipts(
        &self,
        ids: &HashSet<String>,
        party_id: &entities::ids::PartyId,
        executor: &mut dyn Executor,
    ) -> Result<HashSet<String>> {
        if ids.is_empty() {
            return Ok(HashSet::new());
        }
        let receipts = self
            .db
            .customer_receipts()
            .find_many(
                doc! { "id": { "$in": ids.iter().cloned().collect::<Vec<_>>() } },
                executor,
            )
            .await?;
        if receipts.len() != ids.len()
            || receipts.iter().any(|receipt| {
                !matches!(
                    receipt.status,
                    entities::receivable::CustomerReceiptStatus::Posted
                        | entities::receivable::CustomerReceiptStatus::Reversed
                ) || &receipt.counterparty_party_id != party_id
            })
        {
            return Err(Error::Forbidden(
                "票款复核引用的回款事实缺失、未正式过账或往来主体不一致".to_string(),
            ));
        }
        Ok(receipts.into_iter().map(|receipt| receipt.base.id).collect())
    }

    /// 读取当前票款快照引用的正式销项发票；缺失、未登记或主体不一致均失败关闭。
    async fn card_funds_invoices(
        &self,
        ids: &HashSet<String>,
        party_id: &entities::ids::PartyId,
        executor: &mut dyn Executor,
    ) -> Result<HashSet<String>> {
        if ids.is_empty() {
            return Ok(HashSet::new());
        }
        let invoices = self
            .db
            .invoices()
            .find_many(
                doc! { "id": { "$in": ids.iter().cloned().collect::<Vec<_>>() } },
                executor,
            )
            .await?;
        if invoices.len() != ids.len()
            || invoices.iter().any(|invoice| {
                invoice.invoice_direction != entities::receivable::InvoiceDirection::Sales
                    || !matches!(
                        invoice.stable.status(),
                        entities::receivable::InvoiceStatus::Registered
                            | entities::receivable::InvoiceStatus::RedInvoiced
                    )
                    || &invoice.party_id != party_id
            })
        {
            return Err(Error::Forbidden(
                "票款复核引用的发票事实缺失、未正式登记或往来主体不一致".to_string(),
            ));
        }
        Ok(invoices.into_iter().map(|invoice| invoice.base.id).collect())
    }

    /// 从成功审计证明每个票款事实已正式登记，并返回其全部登记经办人。
    async fn card_funds_audit_actors(
        &self,
        resource_type: &str,
        resource_ids: &HashSet<String>,
        operator_actions: &[&str],
        formal_actions: &[&str],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        if resource_ids.is_empty() {
            return Ok(Vec::new());
        }
        let audits = self
            .db
            .audit_logs()
            .find_many(
                doc! {
                    "resource_type": resource_type,
                    "resource_id": { "$in": resource_ids.iter().cloned().collect::<Vec<_>>() },
                    "success": true,
                },
                executor,
            )
            .await?;
        audited_fact_operator_actors(
            resource_type,
            resource_ids,
            &audits,
            operator_actions,
            formal_actions,
        )
    }

    async fn inventory_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let adjustment = self
            .db
            .stock_adjustments()
            .find_by_id(&item.business_object_id, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("库存调整事实缺失".to_string()))?;
        Ok(optional_actors([
            Some(adjustment.prepared_by),
            adjustment.reviewed_by,
            adjustment.finance_reviewed_by,
        ]))
    }

    async fn settlement_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let statement = self
            .db
            .supplier_settlement_statements()
            .find_by_id(&item.business_object_id, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("供应商结算事实缺失".to_string()))?;
        if statement.subject_hash != item.subject_version {
            return Err(Error::Forbidden("供应商结算版本与任务不一致".to_string()));
        }
        Ok(optional_actors([
            Some(statement.prepared_by),
            statement.reviewed_by,
        ]))
    }

    /// 事务内按 MongoDB 权威角色、范围、对象和审批事实构造访问快照。
    async fn assignment_access_for_executor(
        &self,
        account_kind: entities::AccountKind,
        actor_id: &str,
        read_permission: &Permission,
        read_role_ids: &[String],
        manage_role_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<ActorAccess> {
        let role_ids = active_role_ids(&self.db, account_kind, actor_id, executor).await?;
        let active_read_roles = intersect_role_ids(&role_ids, read_role_ids);
        let active_manage_roles = intersect_role_ids(&role_ids, manage_role_ids);
        let mut permissions = Vec::with_capacity(2);
        if !active_read_roles.is_empty() {
            permissions.push(read_permission.clone());
        }
        if !active_manage_roles.is_empty() {
            permissions.push(Permission::parse(MANAGE_PERMISSION).expect("固定权限合法"));
        }
        let participant_document_ids = self
            .db
            .document_participants()
            .list_by_user(actor_id, executor)
            .await?
            .into_iter()
            .map(|participant| participant.document_id.to_string())
            .collect();
        let (organization_ids, responsibility_scopes) = self
            .scope_access_for_executor(actor_id, &role_ids, &active_manage_roles, executor)
            .await?;
        Ok(ActorAccess {
            actor_id: actor_id.to_string(),
            permissions,
            participant_document_ids,
            can_manage: !active_manage_roles.is_empty(),
            organization_ids,
            responsibility_scopes,
        })
    }

    /// 分别保留事务内每个角色的数据范围，禁止角色与用户范围交叉放大。
    async fn scope_access_for_executor(
        &self,
        actor_id: &str,
        role_ids: &[String],
        manage_role_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<(Vec<String>, Vec<(String, Option<String>)>)> {
        let user_scopes = self
            .db
            .data_scopes()
            .list_by_subject(DataScopeSubjectType::User, actor_id, executor)
            .await?;
        let mut responsibility_scopes = Vec::new();
        let mut management_scopes = Vec::new();
        for role_id in role_ids {
            let role_scopes = self
                .db
                .data_scopes()
                .list_by_subject(DataScopeSubjectType::Role, role_id, executor)
                .await?;
            responsibility_scopes.extend(responsibility_pairs(role_id, &role_scopes, &user_scopes));
            if manage_role_ids.contains(role_id) {
                management_scopes.extend(responsibility_pairs(role_id, &role_scopes, &user_scopes));
            }
        }
        responsibility_scopes.sort();
        responsibility_scopes.dedup();
        Ok((
            organizations_from_pairs(&management_scopes),
            responsibility_scopes,
        ))
    }

    /// 使用调用方 executor 读取固定注册表对象事实并重验参与权。
    async fn ensure_item_access_with_executor(
        &self,
        item: &WorkItem,
        access: &ActorAccess,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let policy = object_policy(item.work_item_type, &item.business_object_type)
            .ok_or_else(|| Error::Forbidden("任务类型未注册责任策略".to_string()))?;
        let keys = HashSet::from([(policy.kind, item.business_object_id.clone())]);
        let facts = self.load_object_facts(&keys, executor).await?;
        if authorized_item_fields(item.clone(), access, &facts).is_none() {
            return Err(Error::Forbidden("业务对象不可访问".to_string()));
        }
        Ok(())
    }

    /// 在领域决定事务内按当前账号、有效角色、读取权限、数据范围和对象参与事实重验访问。
    pub(crate) async fn ensure_domain_decision_access(
        &self,
        actor: &AuditActor,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db
            .accounts()
            .find_by_id(actor.id(), executor)
            .await?
            .filter(|account| account.kind == actor.kind() && account.can_login())
            .ok_or_else(|| Error::Forbidden("操作账号不存在、已停用或身份已变化".to_string()))?;
        let policy = object_policy(item.work_item_type, &item.business_object_type)
            .ok_or_else(|| Error::Forbidden("任务类型未注册责任策略".to_string()))?;
        let read_permission = Permission::parse(policy.read_permission).expect("责任策略权限必须合法");
        let policy_revision = MongoCasbinAdapter::new(self.db.clone())
            .policy_revision(executor)
            .await?;
        let role_ids = active_role_ids(&self.db, actor.kind(), actor.id(), executor).await?;
        let read_role_ids = self
            .roles_granting_permission(&role_ids, &read_permission, true)
            .await?;
        if read_role_ids.is_empty() {
            return Err(Error::Forbidden(
                "当前账号已不具备任务业务对象读取权限".to_string(),
            ));
        }
        let access = self
            .assignment_access_for_executor(
                actor.kind(),
                actor.id(),
                &read_permission,
                &read_role_ids,
                &[],
                executor,
            )
            .await?;
        self.ensure_item_access_with_executor(item, &access, executor)
            .await
            .map_err(|_| Error::Forbidden("当前账号不具备任务业务对象的参与权或读取权".to_string()))?;
        ensure_policy_revision(&self.db, policy_revision, executor).await
    }

    async fn managed_access(&self, actor: &AuditActor) -> Result<ActorAccess> {
        let access = self.actor_access(actor).await?;
        if access.can_manage && !access.organization_ids.is_empty() {
            return Ok(access);
        }
        Err(Error::Forbidden("当前账号没有任务责任管理权限".to_string()))
    }

    /// 在同一 MongoDB 事务内执行单文档原子开始处理并写入审计。
    #[allow(clippy::too_many_arguments)]
    async fn start_processing_with_audit(
        &self,
        current: WorkItem,
        expected_task_version: u64,
        actor: &AuditActor,
        action: &str,
        audit_id: String,
        audit_message: String,
        authorization: AssignmentAuthorizationSnapshot,
    ) -> Result<StartProcessingOutcome> {
        let item_id = current.base.id;
        let owner_role = current.owner_role;
        let owner_organization_id = current.owner_organization_id;
        let actor_id = actor.id().to_string();
        let replay_audit_id = audit_id.clone();
        let replay_item_id = item_id.clone();
        let replay_fingerprint = audit_command_fingerprint(&audit_message)
            .expect("服务端审计消息必须携带指纹")
            .to_string();
        let audit = actor.clone().resource_log_with_id(
            audit_id,
            action,
            "work_item",
            item_id.clone(),
            Some(audit_message),
        )?;
        let actor_kind = actor.kind();
        let rbac = self.rbac.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    ensure_policy_revision(&db, authorization.policy_revision, session).await?;
                    let current = db
                        .work_items()
                        .find_by_id(&item_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("任务不存在".to_string()))?;
                    let allow_current_owner = current.owner_user_id.as_deref() == Some(actor_id.as_str());
                    ensure_assignment_policy_in_transaction(
                        &db,
                        &rbac,
                        actor_kind,
                        &actor_id,
                        &actor_id,
                        &current,
                        allow_current_owner,
                        &authorization,
                        false,
                        session,
                    )
                    .await?;
                    let outcome = db
                        .work_items()
                        .start_processing(
                            &item_id,
                            expected_task_version,
                            StartProcessingEligibility {
                                owner_role: &owner_role,
                                owner_organization_id: &owner_organization_id,
                            },
                            &actor_id,
                            Instant::now(),
                            session,
                        )
                        .await?;
                    if let StartProcessingOutcome::Started(assigned)
                    | StartProcessingOutcome::AlreadyOwned(assigned) = &outcome
                    {
                        ensure_assignment_policy_in_transaction(
                            &db,
                            &rbac,
                            actor_kind,
                            &actor_id,
                            &actor_id,
                            assigned,
                            false,
                            &authorization,
                            true,
                            session,
                        )
                        .await?;
                        ensure_policy_revision(&db, authorization.policy_revision, session).await?;
                        db.audit_logs().create(&audit, session).await?;
                    }
                    Ok::<StartProcessingOutcome, Error>(outcome)
                })
            })
            .await;
        match result {
            Ok(outcome) => Ok(outcome),
            Err(error) => match self
                .idempotent_replay(&replay_audit_id, &replay_fingerprint, &replay_item_id)
                .await?
            {
                Some(item) => Ok(StartProcessingOutcome::AlreadyOwned(item)),
                None => Err(error),
            },
        }
    }

    /// 在同一事务内重验管理人、目标责任人及全部业务事实后执行转交与审计。
    #[allow(clippy::too_many_arguments)]
    async fn reassign_with_assignment_policy_audit(
        &self,
        item: WorkItem,
        expected_task_version: u64,
        target_user_id: String,
        actor: &AuditActor,
        action: &str,
        audit_id: String,
        audit_message: String,
        authorization: AssignmentAuthorizationSnapshot,
    ) -> Result<WorkItemWriteOutcome> {
        let replay_audit_id = audit_id.clone();
        let replay_item_id = item.base.id.clone();
        let replay_fingerprint = audit_command_fingerprint(&audit_message)
            .expect("服务端审计消息必须携带指纹")
            .to_string();
        let audit = actor.clone().resource_log_with_id(
            audit_id,
            action,
            "work_item",
            item.base.id.clone(),
            Some(audit_message),
        )?;
        let item_id = item.base.id;
        let actor_id = actor.id().to_string();
        let actor_kind = actor.kind();
        let rbac = self.rbac.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    ensure_policy_revision(&db, authorization.policy_revision, session).await?;
                    let mut current = db
                        .work_items()
                        .find_by_id(&item_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("任务不存在".to_string()))?;
                    if current.base.version != expected_task_version {
                        return Err(WorkItemWriteError::VersionConflict);
                    }
                    let allow_current_owner =
                        current.owner_user_id.as_deref() == Some(target_user_id.as_str());
                    ensure_assignment_policy_in_transaction(
                        &db,
                        &rbac,
                        actor_kind,
                        &actor_id,
                        &target_user_id,
                        &current,
                        true,
                        &authorization,
                        allow_current_owner,
                        session,
                    )
                    .await?;
                    current.reassign(target_user_id.clone(), Instant::now())?;
                    db.work_items()
                        .update(&mut current, session)
                        .await
                        .map_err(work_item_update_error)?;
                    ensure_assignment_policy_in_transaction(
                        &db,
                        &rbac,
                        actor_kind,
                        &actor_id,
                        &target_user_id,
                        &current,
                        true,
                        &authorization,
                        true,
                        session,
                    )
                    .await?;
                    ensure_policy_revision(&db, authorization.policy_revision, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<WorkItem, WorkItemWriteError>(current)
                })
            })
            .await;
        match result {
            Ok(item) => Ok(WorkItemWriteOutcome::Updated(Box::new(item))),
            Err(WorkItemWriteError::VersionConflict) => Ok(WorkItemWriteOutcome::VersionConflict),
            Err(WorkItemWriteError::Service(error)) => match self
                .idempotent_replay(&replay_audit_id, &replay_fingerprint, &replay_item_id)
                .await?
            {
                Some(item) => Ok(WorkItemWriteOutcome::Updated(Box::new(item))),
                None => Err(error),
            },
        }
    }

    /// 在同一事务内重验当前责任人的账号、授权、参与权和岗位分离后退回责任池。
    #[allow(clippy::too_many_arguments)]
    async fn release_with_assignment_policy_audit(
        &self,
        item: WorkItem,
        expected_task_version: u64,
        actor: &AuditActor,
        action: &str,
        audit_id: String,
        audit_message: String,
        authorization: AssignmentAuthorizationSnapshot,
    ) -> Result<WorkItemWriteOutcome> {
        let replay_audit_id = audit_id.clone();
        let replay_item_id = item.base.id.clone();
        let replay_fingerprint = audit_command_fingerprint(&audit_message)
            .expect("服务端审计消息必须携带指纹")
            .to_string();
        let audit = actor.clone().resource_log_with_id(
            audit_id,
            action,
            "work_item",
            item.base.id.clone(),
            Some(audit_message),
        )?;
        let item_id = item.base.id;
        let actor_id = actor.id().to_string();
        let actor_kind = actor.kind();
        let rbac = self.rbac.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    ensure_policy_revision(&db, authorization.policy_revision, session).await?;
                    let mut current = db
                        .work_items()
                        .find_by_id(&item_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("任务不存在".to_string()))?;
                    if current.base.version != expected_task_version {
                        return Err(WorkItemWriteError::VersionConflict);
                    }
                    if !current.is_owned_by(&actor_id) {
                        return Err(WorkItemWriteError::Service(Error::ConflictError(
                            "任务当前责任人已变化".to_string(),
                        )));
                    }
                    if let Some(step_id) = current.approval_step_instance_id.as_deref() {
                        let step = db
                            .approval_step_instances()
                            .find_by_id(step_id, session)
                            .await?
                            .ok_or_else(|| {
                                Error::BusinessLogicError("审批责任记录不完整，请联系管理员。".to_string())
                            })?;
                        if step.status == ApprovalStepStatus::Blocked {
                            return Err(WorkItemWriteError::Service(Error::BusinessLogicError(
                                "审批当前受阻，请等待管理员恢复。".to_string(),
                            )));
                        }
                    }
                    ensure_assignment_policy_in_transaction(
                        &db,
                        &rbac,
                        actor_kind,
                        &actor_id,
                        &actor_id,
                        &current,
                        false,
                        &authorization,
                        true,
                        session,
                    )
                    .await?;
                    current.release_to_pool(Instant::now())?;
                    db.work_items()
                        .update(&mut current, session)
                        .await
                        .map_err(work_item_update_error)?;
                    WorkItemService::new(db.clone(), rbac.clone())
                        .ensure_assignment_actor_access(
                            actor_kind,
                            &actor_id,
                            &current,
                            false,
                            &authorization,
                            session,
                        )
                        .await?;
                    ensure_policy_revision(&db, authorization.policy_revision, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<WorkItem, WorkItemWriteError>(current)
                })
            })
            .await;
        match result {
            Ok(item) => Ok(WorkItemWriteOutcome::Updated(Box::new(item))),
            Err(WorkItemWriteError::VersionConflict) => Ok(WorkItemWriteOutcome::VersionConflict),
            Err(WorkItemWriteError::Service(error)) => match self
                .idempotent_replay(&replay_audit_id, &replay_fingerprint, &replay_item_id)
                .await?
            {
                Some(item) => Ok(WorkItemWriteOutcome::Updated(Box::new(item))),
                None => Err(error),
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn close_with_domain_evidence(
        &self,
        mut item: WorkItem,
        actor: &AuditActor,
        reason_code: &str,
        replacement_work_item_id: Option<&str>,
        action: &str,
        audit_id: String,
        audit_message: String,
        close_reason: String,
    ) -> Result<WorkItemWriteOutcome> {
        let closed_at = Instant::now();
        item.close(actor.id(), WorkItemCloseData { close_reason }, closed_at)?;
        let replay_audit_id = audit_id.clone();
        let replay_item_id = item.base.id.clone();
        let replay_fingerprint = audit_command_fingerprint(&audit_message)
            .expect("服务端审计消息必须携带指纹")
            .to_string();
        let audit = actor.clone().resource_log_with_id(
            audit_id.clone(),
            action,
            "work_item",
            item.base.id.clone(),
            Some(audit_message),
        )?;
        let actor_id = actor.id().to_string();
        let reason_code = reason_code.to_string();
        let replacement_work_item_id = replacement_work_item_id.map(ToString::to_string);
        let db = self.db.clone();
        let client = db.client().clone();
        let result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    close_w29_domain_object(
                        &db,
                        &item,
                        &reason_code,
                        replacement_work_item_id.as_deref(),
                        &actor_id,
                        &audit_id,
                        closed_at,
                        session,
                    )
                    .await?;
                    db.work_items()
                        .update(&mut item, session)
                        .await
                        .map_err(work_item_update_error)?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<WorkItem, WorkItemWriteError>(item)
                })
            })
            .await;
        match result {
            Ok(item) => Ok(WorkItemWriteOutcome::Updated(Box::new(item))),
            Err(WorkItemWriteError::VersionConflict) => Ok(WorkItemWriteOutcome::VersionConflict),
            Err(WorkItemWriteError::Service(error)) => match self
                .idempotent_replay(&replay_audit_id, &replay_fingerprint, &replay_item_id)
                .await?
            {
                Some(item) => Ok(WorkItemWriteOutcome::Updated(Box::new(item))),
                None => Err(error),
            },
        }
    }

    /// 读取已完成的同一幂等命令，并拒绝相同键混用不同请求。
    async fn idempotent_replay(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        item_id: &str,
    ) -> Result<Option<WorkItem>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        if audit.resource_id.as_deref() != Some(item_id) {
            return Err(Error::Internal("幂等审计资源与命令不一致".to_string()));
        }
        if audit.message.as_deref().and_then(audit_command_fingerprint) != Some(expected_fingerprint) {
            return Err(Error::ConflictError("幂等键已用于不同的命令内容".to_string()));
        }
        self.load(item_id).await.map(Some)
    }

    /// 将成功写入的实体重新按当前 actor 授权投影。
    async fn applied_outcome(&self, item: WorkItem, actor: &AuditActor) -> Result<WorkItemMutationOutcome> {
        self.mutation_view(item, actor)
            .await
            .map(WorkItemMutationOutcome::Applied)
    }

    /// 冲突后重新读取任务并形成权限安全的最新投影。
    ///
    /// 最新任务不再可见或已经删除时固定返回空摘要；授权与对象读取的其他
    /// 基础设施错误继续失败，禁止退化为未经裁剪的实体。
    async fn conflict_outcome(
        &self,
        item_id: &str,
        kind: WorkItemConflictKind,
        actor: &AuditActor,
    ) -> Result<WorkItemMutationOutcome> {
        let Some(item) = self
            .db
            .work_items()
            .find_by_id(item_id, &mut NoTransaction)
            .await?
        else {
            return Ok(WorkItemMutationOutcome::Conflict(WorkItemConflict::new(
                kind, None,
            )));
        };
        let current_work_item = match self.mutation_view(item, actor).await {
            Ok(view) => Some(view),
            Err(Error::Forbidden(_) | Error::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        Ok(WorkItemMutationOutcome::Conflict(WorkItemConflict::new(
            kind,
            current_work_item,
        )))
    }

    async fn mutation_view(&self, item: WorkItem, actor: &AuditActor) -> Result<WorkItemView> {
        let access = self.actor_access(actor).await?;
        let scope = detail_scope(&item, actor.id(), &access)?;
        let item_id = item.base.id.clone();
        let fields = self
            .authorized_fields_for_items(vec![item], &access)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| Error::Forbidden("当前账号无权查看该业务对象".to_string()))?;
        let view_access = self.view_access(&fields, scope, actor, &access).await?;
        let mut view = WorkItemView::from_fields(fields, single_item_context_id(actor.id(), &item_id))
            .with_access(
                view_access.processing_state,
                view_access.processing_blocker,
                view_access.allowed_actions,
                view_access.action_blockers,
            );
        self.apply_party_names(std::slice::from_mut(&mut view)).await?;
        Ok(view)
    }
}

/// 在任务责任事务内重放全部固定分派策略。
#[allow(clippy::too_many_arguments)]
async fn ensure_assignment_policy_in_transaction(
    db: &Database,
    rbac: &SharedRbacService,
    actor_kind: entities::AccountKind,
    actor_id: &str,
    assignee_id: &str,
    item: &WorkItem,
    require_manager: bool,
    authorization: &AssignmentAuthorizationSnapshot,
    allow_current_owner: bool,
    executor: &mut dyn Executor,
) -> Result<()> {
    if actor_kind != authorization.actor_kind {
        return Err(Error::Forbidden("操作账号身份已变化".to_string()));
    }
    let service = WorkItemService::new(db.clone(), rbac.clone());
    service
        .ensure_assignment_actor_access(
            actor_kind,
            actor_id,
            item,
            require_manager,
            authorization,
            executor,
        )
        .await?;
    service
        .ensure_assignment_candidate(
            assignee_id,
            authorization.assignee_kind,
            item,
            authorization,
            allow_current_owner,
            executor,
        )
        .await
}

/// 事务内校验 Casbin 持久化策略仍与事务外稳定授权快照一致。
async fn ensure_policy_revision(
    db: &Database,
    expected_revision: u64,
    executor: &mut dyn Executor,
) -> Result<()> {
    let actual = MongoCasbinAdapter::new(db.clone())
        .policy_revision(executor)
        .await?;
    if actual != expected_revision {
        return Err(Error::Forbidden("任务分派期间授权策略已变化，请重试".to_string()));
    }
    Ok(())
}

/// 从事务内 Casbin `g` 授权事实与启用角色形成角色集合。
async fn active_role_ids(
    db: &Database,
    account_kind: entities::AccountKind,
    account_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    let subject = crate::iam::subject(account_kind, account_id);
    let mut role_ids = MongoCasbinAdapter::new(db.clone())
        .subject_roles(&subject, executor)
        .await?
        .into_iter()
        .filter_map(|role_key| role_key.strip_prefix("role:").map(str::to_string))
        .collect::<Vec<_>>();
    role_ids.sort();
    role_ids.dedup();
    if role_ids.is_empty() {
        return Ok(role_ids);
    }
    let enabled = db
        .roles()
        .enabled_roles(&role_ids, executor)
        .await?
        .into_iter()
        .map(|role| role.base.id)
        .collect::<HashSet<_>>();
    role_ids.retain(|role_id| enabled.contains(role_id));
    Ok(role_ids)
}

fn intersect_role_ids(active: &[String], authorized: &[String]) -> Vec<String> {
    active
        .iter()
        .filter(|role_id| authorized.contains(role_id))
        .cloned()
        .collect()
}

fn approval_assignment_separated(
    candidate_id: &str,
    started_by: &str,
    submitted_by: &str,
    responsibility_actor_ids: &[String],
    current_owner_user_id: Option<&str>,
    allow_current_owner: bool,
    steps: &[entities::approval::ApprovalStepInstance],
) -> bool {
    if candidate_id == started_by || candidate_id == submitted_by {
        return false;
    }
    if responsibility_actor_ids.iter().any(|actor_id| {
        actor_id == candidate_id && !(allow_current_owner && current_owner_user_id == Some(candidate_id))
    }) {
        return false;
    }
    !steps
        .iter()
        .filter_map(|step| step.decided_by.as_deref())
        .any(|decided_by| decided_by == candidate_id)
}

#[allow(clippy::too_many_arguments)]
async fn close_w29_domain_object(
    db: &Database,
    item: &WorkItem,
    reason_code: &str,
    replacement_work_item_id: Option<&str>,
    actor_id: &str,
    audit_id: &str,
    closed_at: Instant,
    executor: &mut dyn Executor,
) -> Result<()> {
    let evidence_reference =
        w29_domain_evidence_reference(&item.base.id, reason_code, replacement_work_item_id, audit_id)?;
    if let Some(replacement_work_item_id) = replacement_work_item_id {
        if replacement_work_item_id == item.base.id.as_str() {
            return Err(Error::ValidationError("替代任务不能引用自身".to_string()));
        }
        let replacement = db
            .work_items()
            .find_by_id(replacement_work_item_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("替代任务不存在".to_string()))?;
        if replacement.status != WorkItemStatus::Open
            || !is_w29_closable(&replacement)
            || replacement.work_item_type != item.work_item_type
            || replacement.business_object_type != item.business_object_type
        {
            return Err(Error::ConflictError(
                "替代任务必须在关闭事务中仍是同一 W29 对象类别的开放正式任务".to_string(),
            ));
        }
    }
    match item.business_object_type.as_str() {
        "integration_error_task" => {
            let mut task = db
                .integration_error_tasks()
                .find_by_id(&item.business_object_id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("集成异常任务不存在".to_string()))?;
            let registered_type = if task.error_class == ErrorClass::ResultUnknown {
                WorkItemType::IntegrationResultUnknown
            } else {
                WorkItemType::BusinessException
            };
            if item.work_item_type != registered_type {
                return Err(Error::ConflictError(
                    "任务类型与集成异常分类不一致，请刷新".to_string(),
                ));
            }
            task.transition(
                ErrorTaskStatus::Closed,
                Some(ResolutionType::Close),
                Some(evidence_reference),
                closed_at,
            )?;
            db.integration_error_tasks().update(&mut task, executor).await?;
            Ok(())
        }
        "reconciliation_difference" => {
            let difference_id = ReconciliationDifferenceId::new(item.business_object_id.clone());
            db.reconciliation_differences()
                .find_by_id(&item.business_object_id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("对账差异不存在".to_string()))?;
            let latest = db
                .reconciliation_difference_resolutions()
                .find_latest_by_difference(&difference_id, executor)
                .await?;
            if latest
                .as_ref()
                .is_some_and(|resolution| resolution.resulting_status.is_terminal())
            {
                return Err(Error::ConflictError("对账差异已经关闭或形成正式结论".to_string()));
            }
            let resolution_no = latest
                .map_or(Ok(1), |resolution| {
                    resolution.resolution_no.checked_add(1).ok_or(())
                })
                .map_err(|()| Error::ConflictError("差异决定序号已达上限".to_string()))?;
            let resolution_action = match reason_code {
                "DUPLICATE" => ResolutionAction::CloseDuplicate,
                "MISROUTED" => ResolutionAction::CloseMisrouted,
                _ => {
                    return Err(Error::ValidationError(
                        "关闭原因只允许 DUPLICATE 或 MISROUTED".to_string(),
                    ));
                }
            };
            let resolution = ReconciliationDifferenceResolution::new_close_evidence(
                ReconciliationDifferenceResolutionId::new(format!("w29-close-{}", stable_digest(audit_id))),
                difference_id,
                resolution_no,
                resolution_action,
                evidence_reference,
                actor_id.to_string(),
                closed_at,
            )?;
            db.reconciliation_difference_resolutions()
                .create(&resolution, executor)
                .await?;
            Ok(())
        }
        _ => Err(Error::BusinessLogicError(
            "只有 W29 登记的异常对象允许受控关闭".to_string(),
        )),
    }
}

struct ActorAccess {
    actor_id: String,
    permissions: Vec<Permission>,
    participant_document_ids: HashSet<String>,
    organization_ids: Vec<String>,
    responsibility_scopes: Vec<(String, Option<String>)>,
    can_manage: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ObjectKind {
    SalesOrder,
    ProcurementConfirmation,
    PurchaseOrder,
    SalesChangeReview,
    ReceivableAccount,
    StockAdjustment,
    SupplierSettlement,
    LegacyImportBatch,
    IntegrationErrorTask,
    ReconciliationDifference,
    MasterMappingTask,
    SupplierFulfillmentOrder,
    SupplierOffering,
}

#[derive(Debug, Clone, Copy)]
struct ObjectPolicy {
    kind: ObjectKind,
    work_item_type: WorkItemType,
    business_object_type: &'static str,
    read_permission: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssignmentSeparationPolicy {
    ApprovalHistory,
    DomainActors,
    RoleAndParticipation,
    FailClosed,
}

#[derive(Debug, Clone, Default)]
struct SubjectBrief {
    counterparty_label: Option<String>,
    impact_summary: Option<String>,
    brief_source: Option<brief::ObjectBriefSource>,
}

#[derive(Debug, Clone)]
struct ObjectFact {
    root_document_id: String,
    label: String,
    created_by: String,
    /// 生产者合同允许的权威版本；空集合表示该领域没有通用锁版本约束。
    subject_versions: Vec<String>,
    counterparty_label: Option<String>,
    impact_summary: Option<String>,
    brief_source: Option<brief::ObjectBriefSource>,
    subject_briefs: HashMap<String, SubjectBrief>,
}

impl ObjectFact {
    /// 构造只有身份标题的对象事实。
    ///
    /// # 参数
    /// * `root_document_id` - 工作面根对象 ID
    /// * `label` - 面向用户的对象标题
    /// * `created_by` - 对象创建人，用于参与权判断
    ///
    /// # 返回
    /// 返回无往来方、无影响覆盖的对象事实。
    ///
    /// # 错误
    /// 无。
    fn new(
        root_document_id: impl Into<String>,
        label: impl Into<String>,
        created_by: impl Into<String>,
    ) -> Self {
        Self {
            root_document_id: root_document_id.into(),
            label: label.into(),
            created_by: created_by.into(),
            subject_versions: Vec::new(),
            counterparty_label: None,
            impact_summary: None,
            brief_source: None,
            subject_briefs: HashMap::new(),
        }
    }
}

type ObjectFactMap = HashMap<(ObjectKind, String), ObjectFact>;

const SYSTEM_OBJECT_OWNER: &str = "__system__";

const OBJECT_POLICIES: [ObjectPolicy; 20] = [
    ObjectPolicy {
        kind: ObjectKind::SalesOrder,
        work_item_type: WorkItemType::LowMarginManagerConfirmation,
        business_object_type: "sales_order",
        read_permission: "sales_order:detail",
    },
    ObjectPolicy {
        kind: ObjectKind::ProcurementConfirmation,
        work_item_type: WorkItemType::ProcurementConfirmation,
        business_object_type: "procurement_confirmation",
        read_permission: "procurement_confirmation:detail",
    },
    ObjectPolicy {
        kind: ObjectKind::PurchaseOrder,
        work_item_type: WorkItemType::PurchaseOrderReview,
        business_object_type: "purchase_order",
        read_permission: "purchase_order:detail",
    },
    ObjectPolicy {
        kind: ObjectKind::SalesChangeReview,
        work_item_type: WorkItemType::SalesChangeImpactReview,
        business_object_type: "sales_change_review",
        read_permission: "sales_change_order:detail",
    },
    ObjectPolicy {
        kind: ObjectKind::SalesChangeReview,
        work_item_type: WorkItemType::SalesChangeFinanceReview,
        business_object_type: "sales_change_review",
        read_permission: "sales_change_order:detail",
    },
    ObjectPolicy {
        kind: ObjectKind::SalesOrder,
        work_item_type: WorkItemType::CardSalesManagerApproval,
        business_object_type: "sales_order",
        read_permission: "sales_order:detail",
    },
    ObjectPolicy {
        kind: ObjectKind::SalesOrder,
        work_item_type: WorkItemType::CardSalesOperationApproval,
        business_object_type: "sales_order",
        read_permission: "sales_order:detail",
    },
    ObjectPolicy {
        kind: ObjectKind::ReceivableAccount,
        work_item_type: WorkItemType::CardFundsReview,
        business_object_type: "receivable_account",
        read_permission: "receivable_account:detail",
    },
    ObjectPolicy {
        kind: ObjectKind::ReceivableAccount,
        work_item_type: WorkItemType::CardFundsDeltaReview,
        business_object_type: "receivable_account",
        read_permission: "receivable_account:detail",
    },
    ObjectPolicy {
        kind: ObjectKind::StockAdjustment,
        work_item_type: WorkItemType::InventoryAdjustmentReview,
        business_object_type: "stock_adjustment",
        read_permission: "stock_adjustment:detail",
    },
    ObjectPolicy {
        kind: ObjectKind::SupplierSettlement,
        work_item_type: WorkItemType::SupplierSettlementReview,
        business_object_type: "supplier_settlement_statement",
        read_permission: "supplier_settlement_statement:detail",
    },
    ObjectPolicy {
        kind: ObjectKind::LegacyImportBatch,
        work_item_type: WorkItemType::ImportBusinessConfirmation,
        business_object_type: "LEGACY_IMPORT_BATCH",
        read_permission: "legacy_import_batch:detail",
    },
    ObjectPolicy {
        kind: ObjectKind::IntegrationErrorTask,
        work_item_type: WorkItemType::IntegrationResultUnknown,
        business_object_type: "integration_error_task",
        read_permission: "integration_error_task:detail",
    },
    ObjectPolicy {
        kind: ObjectKind::IntegrationErrorTask,
        work_item_type: WorkItemType::BusinessException,
        business_object_type: "integration_error_task",
        read_permission: "integration_error_task:detail",
    },
    ObjectPolicy {
        kind: ObjectKind::ReconciliationDifference,
        work_item_type: WorkItemType::BusinessException,
        business_object_type: "reconciliation_difference",
        read_permission: "reconciliation_difference:detail",
    },
    ObjectPolicy {
        kind: ObjectKind::ReconciliationDifference,
        work_item_type: WorkItemType::IntegrationResultUnknown,
        business_object_type: "reconciliation_difference",
        read_permission: "reconciliation_difference:detail",
    },
    ObjectPolicy {
        kind: ObjectKind::MasterMappingTask,
        work_item_type: WorkItemType::BusinessException,
        business_object_type: "MASTER_MAPPING_TASK",
        read_permission: "master_mapping_task:list",
    },
    ObjectPolicy {
        kind: ObjectKind::SupplierFulfillmentOrder,
        work_item_type: WorkItemType::IntegrationResultUnknown,
        business_object_type: "SUPPLIER_FULFILLMENT_ORDER",
        read_permission: "supplier_fulfillment_order:detail",
    },
    ObjectPolicy {
        kind: ObjectKind::SupplierFulfillmentOrder,
        work_item_type: WorkItemType::BusinessException,
        business_object_type: "SUPPLIER_FULFILLMENT_ORDER",
        read_permission: "supplier_fulfillment_order:detail",
    },
    ObjectPolicy {
        kind: ObjectKind::SupplierOffering,
        work_item_type: WorkItemType::BusinessException,
        business_object_type: "SUPPLIER_OFFERING",
        read_permission: "supplier_offering:list",
    },
];

fn object_policy(work_item_type: WorkItemType, business_object_type: &str) -> Option<&'static ObjectPolicy> {
    OBJECT_POLICIES.iter().find(|policy| {
        policy.work_item_type == work_item_type && policy.business_object_type == business_object_type
    })
}

fn assignment_separation_policy(work_item_type: WorkItemType) -> AssignmentSeparationPolicy {
    match work_item_type {
        WorkItemType::CardSalesManagerApproval | WorkItemType::CardSalesOperationApproval => {
            AssignmentSeparationPolicy::ApprovalHistory
        }
        WorkItemType::ProcurementConfirmation
        | WorkItemType::LowMarginManagerConfirmation
        | WorkItemType::PurchaseOrderReview
        | WorkItemType::SalesChangeImpactReview
        | WorkItemType::SalesChangeFinanceReview
        | WorkItemType::CardFundsReview
        | WorkItemType::CardFundsDeltaReview
        | WorkItemType::InventoryAdjustmentReview
        | WorkItemType::SupplierSettlementReview => AssignmentSeparationPolicy::DomainActors,
        WorkItemType::ImportBusinessConfirmation
        | WorkItemType::IntegrationResultUnknown
        | WorkItemType::BusinessException => AssignmentSeparationPolicy::RoleAndParticipation,
        WorkItemType::OwnershipMigrationSalesConfirmation
        | WorkItemType::OwnershipMigrationFinanceConfirmation
        | WorkItemType::FinanceCorrectionReview => AssignmentSeparationPolicy::FailClosed,
        WorkItemType::DocumentApproval => AssignmentSeparationPolicy::ApprovalHistory,
    }
}

fn optional_actors<const N: usize>(actors: [Option<String>; N]) -> Vec<String> {
    actors.into_iter().flatten().collect()
}

/// 按资源逐一证明正式票款审计，并返回创建、过账或红冲等经办人。
fn audited_fact_operator_actors(
    resource_type: &str,
    resource_ids: &HashSet<String>,
    audits: &[entities::AuditLog],
    operator_actions: &[&str],
    formal_actions: &[&str],
) -> Result<Vec<String>> {
    let matches_action =
        |action: &str, prefixes: &[&str]| prefixes.iter().any(|prefix| action.starts_with(prefix));
    let mut actors = Vec::new();
    for resource_id in resource_ids {
        let facts = audits
            .iter()
            .filter(|audit| {
                audit.success
                    && audit.resource_type == resource_type
                    && audit.resource_id.as_deref() == Some(resource_id.as_str())
            })
            .collect::<Vec<_>>();
        if !facts
            .iter()
            .any(|audit| matches_action(&audit.action, formal_actions))
        {
            return Err(Error::Forbidden(
                "无法从审计事实证明票款已经正式登记，任务分派失败关闭".to_string(),
            ));
        }
        actors.extend(
            facts
                .into_iter()
                .filter(|audit| matches_action(&audit.action, operator_actions))
                .map(|audit| audit.actor_id.clone()),
        );
    }
    Ok(actors)
}

fn non_empty_assignment_actors(actors: Vec<String>) -> Result<HashSet<String>> {
    let actors = actors
        .into_iter()
        .map(|actor| actor.trim().to_string())
        .filter(|actor| !actor.is_empty() && actor != SYSTEM_OBJECT_OWNER)
        .collect::<HashSet<_>>();
    if actors.is_empty() {
        return Err(Error::Forbidden(
            "任务岗位分离所需的权威责任人事实缺失".to_string(),
        ));
    }
    Ok(actors)
}

fn object_ids(keys: &HashSet<(ObjectKind, String)>, kind: ObjectKind) -> Vec<String> {
    keys.iter()
        .filter(|(candidate, _)| *candidate == kind)
        .map(|(_, id)| id.clone())
        .collect()
}

fn object_access_shapes(access: &ActorAccess) -> Vec<(WorkItemType, String)> {
    OBJECT_POLICIES
        .iter()
        .filter(|policy| has_permission(access, policy.read_permission))
        .map(|policy| (policy.work_item_type, policy.business_object_type.to_string()))
        .collect()
}

fn has_permission(access: &ActorAccess, permission: &str) -> bool {
    let required = Permission::parse(permission).expect("对象注册表权限必须合法");
    access
        .permissions
        .iter()
        .any(|permission| permission.covers(&required))
}

fn authorized_fields(
    rows: Vec<database::WorkItemRow>,
    access: &ActorAccess,
    facts: &ObjectFactMap,
) -> Vec<dto::WorkItemFields> {
    rows.into_iter()
        .filter_map(|row| {
            let policy = object_policy(row.work_item_type, &row.business_object_type)?;
            let fact = facts.get(&(policy.kind, row.business_object_id.clone()))?;
            if !has_permission(access, policy.read_permission)
                || !has_object_participation(access, &row.owner_role, &row.owner_organization_id, fact)
                || !subject_version_matches(fact, &row.subject_version)
            {
                return None;
            }
            let mut fields = dto::WorkItemFields::from(row);
            apply_object_display(&mut fields, fact);
            Some(fields)
        })
        .collect()
}

fn authorized_item_fields(
    item: WorkItem,
    access: &ActorAccess,
    facts: &ObjectFactMap,
) -> Option<dto::WorkItemFields> {
    let policy = object_policy(item.work_item_type, &item.business_object_type)?;
    let fact = facts.get(&(policy.kind, item.business_object_id.clone()))?;
    if !has_permission(access, policy.read_permission)
        || !has_object_participation(access, &item.owner_role, &item.owner_organization_id, fact)
        || !subject_version_matches(fact, &item.subject_version)
    {
        return None;
    }
    let mut fields = dto::WorkItemFields::from(item);
    apply_object_display(&mut fields, fact);
    Some(fields)
}

/// 把对象事实中的标题、往来方和影响写回任务投影字段。
///
/// # 参数
/// * `fields` - 待覆盖的任务字段
/// * `fact` - 已授权对象事实
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
fn apply_object_display(fields: &mut dto::WorkItemFields, fact: &ObjectFact) {
    fields.business_object_label = fact.label.clone();
    fields.root_business_object_id = fact.root_document_id.clone();
    let subject = fact.subject_briefs.get(&fields.subject_version);
    apply_subject_display(fields, fact, subject);
}

/// 按任务针对的提交版本覆盖往来方、影响和事项简报。
///
/// # 参数
/// * `fields` - 待覆盖的任务字段
/// * `fact` - 对象级默认展示
/// * `subject` - 与 `subject_version` 对应的提交展示；缺失时回退对象默认值
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
fn apply_subject_display(
    fields: &mut dto::WorkItemFields,
    fact: &ObjectFact,
    subject: Option<&SubjectBrief>,
) {
    fields.counterparty_label = subject
        .and_then(|item| item.counterparty_label.clone())
        .or_else(|| fact.counterparty_label.clone());
    if let Some(impact) = subject
        .and_then(|item| item.impact_summary.clone())
        .or_else(|| fact.impact_summary.clone())
    {
        fields.impact_summary = Some(impact);
    }
    fields.brief_source = subject
        .and_then(|item| item.brief_source.clone())
        .or_else(|| fact.brief_source.clone());
}

fn subject_version_matches(fact: &ObjectFact, actual: &str) -> bool {
    fact.subject_versions.is_empty() || fact.subject_versions.iter().any(|expected| expected == actual)
}

fn has_object_participation(
    access: &ActorAccess,
    owner_role: &str,
    owner_organization_id: &str,
    fact: &ObjectFact,
) -> bool {
    fact.created_by == access.actor_id
        || access.participant_document_ids.contains(&fact.root_document_id)
        || covers_responsibility(access, owner_role, owner_organization_id)
        || (access.can_manage && covers_organization(access, owner_organization_id))
}

struct ViewAccess {
    processing_state: ProcessingState,
    processing_blocker: Option<ProcessingBlockerView>,
    allowed_actions: Vec<WorkItemAllowedAction>,
    action_blockers: Vec<ProcessingBlockerView>,
}

impl ViewAccess {
    fn ready(allowed_actions: Vec<WorkItemAllowedAction>) -> Self {
        Self {
            processing_state: ProcessingState::Ready,
            processing_blocker: None,
            allowed_actions,
            action_blockers: Vec::new(),
        }
    }

    fn blocked(blocker: ProcessingBlockerView) -> Self {
        Self {
            processing_state: ProcessingState::ApprovalBlocked,
            processing_blocker: Some(blocker.clone()),
            allowed_actions: Vec::new(),
            action_blockers: vec![blocker],
        }
    }
}

fn counts_as_processable_stat(scope: WorkItemScope, access: &ViewAccess) -> bool {
    if access.processing_state != ProcessingState::Ready {
        return false;
    }
    let required_action = match scope {
        WorkItemScope::Mine => WorkItemAllowedAction::Process,
        WorkItemScope::Team => WorkItemAllowedAction::StartProcessing,
        WorkItemScope::Managed | WorkItemScope::History => return false,
    };
    access.allowed_actions.contains(&required_action)
}

fn allowed_actions(
    item: &dto::WorkItemFields,
    scope: WorkItemScope,
    actor_id: &str,
    access: &ActorAccess,
    team_candidate_eligible: bool,
) -> Vec<WorkItemAllowedAction> {
    let mut actions = vec![WorkItemAllowedAction::View];
    if item.owner_user_id.as_deref() == Some(actor_id)
        && (covers_responsibility(access, &item.owner_role, &item.owner_organization_id)
            || item.status != WorkItemStatus::Open)
    {
        actions.push(WorkItemAllowedAction::Process);
        if item.assignment_mode == AssignmentMode::Pool {
            actions.push(WorkItemAllowedAction::ReleaseToTeam);
        }
    }
    if scope == WorkItemScope::Team && item.owner_user_id.is_none() && team_candidate_eligible {
        actions.push(WorkItemAllowedAction::StartProcessing);
    }
    if access.can_manage && scope == WorkItemScope::Managed {
        actions.push(WorkItemAllowedAction::Reassign);
        if is_w29_fields_closable(item) {
            actions.push(WorkItemAllowedAction::Close);
        }
    }
    actions
}

fn retain_team_eligible_fields(
    fields: Vec<dto::WorkItemFields>,
    eligible_ids: &HashSet<String>,
) -> Vec<dto::WorkItemFields> {
    fields
        .into_iter()
        .filter(|field| eligible_ids.contains(&field.id))
        .collect()
}

fn is_assignment_candidate_denial(error: &Error) -> bool {
    matches!(
        error,
        Error::NotFound(_)
            | Error::ValidationError(_)
            | Error::BusinessLogicError(_)
            | Error::ConflictError(_)
            | Error::Forbidden(_)
            | Error::Logic(_)
    )
}

fn ensure_team_access(access: &ActorAccess) -> Result<()> {
    if access.responsibility_scopes.is_empty() {
        return Err(Error::Forbidden("当前账号没有可证明的团队任务范围".to_string()));
    }
    Ok(())
}

fn ensure_managed_access(access: &ActorAccess) -> Result<()> {
    if !access.can_manage || access.organization_ids.is_empty() {
        return Err(Error::Forbidden("当前账号没有任务责任管理范围".to_string()));
    }
    Ok(())
}

fn ensure_item_in_managed_scope(item: &WorkItem, access: &ActorAccess) -> Result<()> {
    if covers_organization(access, &item.owner_organization_id) {
        return Ok(());
    }
    Err(Error::Forbidden("任务不在当前账号的责任管理范围内".to_string()))
}

fn organization_filter(access: &ActorAccess) -> Vec<String> {
    if access
        .organization_ids
        .iter()
        .any(|organization_id| organization_id == "*")
    {
        return Vec::new();
    }
    access.organization_ids.to_vec()
}

#[derive(Clone)]
enum OrganizationCoverage {
    All,
    Targets(Vec<String>),
}

fn organizations_from_scopes(scopes: &[DataScope]) -> Vec<String> {
    if scopes
        .iter()
        .any(|scope| scope.scope_type == DataScopeType::Company)
    {
        return vec!["*".to_string()];
    }
    let mut organizations = scopes
        .iter()
        .filter(|scope| {
            matches!(
                scope.scope_type,
                DataScopeType::Organization | DataScopeType::Team
            )
        })
        .flat_map(|scope| scope.scope_targets.clone())
        .collect::<Vec<_>>();
    organizations.sort();
    organizations.dedup();
    organizations
}

fn organizations_from_pairs(pairs: &[(String, Option<String>)]) -> Vec<String> {
    if pairs.iter().any(|(_, organization_id)| organization_id.is_none()) {
        return vec!["*".to_string()];
    }
    let mut organizations = pairs
        .iter()
        .filter_map(|(_, organization_id)| organization_id.clone())
        .collect::<Vec<_>>();
    organizations.sort();
    organizations.dedup();
    organizations
}

fn responsibility_pairs(
    role_id: &str,
    role_scopes: &[DataScope],
    user_scopes: &[DataScope],
) -> Vec<(String, Option<String>)> {
    let Some(role_coverage) = organization_coverage(role_scopes, false) else {
        return Vec::new();
    };
    let Some(user_coverage) = organization_coverage(user_scopes, true) else {
        return Vec::new();
    };
    intersect_coverage(role_coverage, user_coverage)
        .into_iter()
        .map(|organization_id| (role_id.to_string(), organization_id))
        .collect()
}

fn organization_coverage(scopes: &[DataScope], empty_is_all: bool) -> Option<OrganizationCoverage> {
    if scopes
        .iter()
        .any(|scope| scope.scope_type == DataScopeType::Company)
    {
        return Some(OrganizationCoverage::All);
    }
    let targets = organizations_from_scopes(scopes);
    if !targets.is_empty() {
        return Some(OrganizationCoverage::Targets(targets));
    }
    empty_is_all.then_some(OrganizationCoverage::All)
}

fn intersect_coverage(role: OrganizationCoverage, user: OrganizationCoverage) -> Vec<Option<String>> {
    match (role, user) {
        (OrganizationCoverage::All, OrganizationCoverage::All) => vec![None],
        (OrganizationCoverage::Targets(targets), OrganizationCoverage::All)
        | (OrganizationCoverage::All, OrganizationCoverage::Targets(targets)) => {
            targets.into_iter().map(Some).collect()
        }
        (OrganizationCoverage::Targets(role), OrganizationCoverage::Targets(user)) => role
            .into_iter()
            .filter(|organization_id| user.contains(organization_id))
            .map(Some)
            .collect(),
    }
}

fn covers_responsibility(access: &ActorAccess, role: &str, organization_id: &str) -> bool {
    access
        .responsibility_scopes
        .iter()
        .any(|(allowed_role, allowed_organization)| {
            allowed_role == role
                && allowed_organization
                    .as_deref()
                    .is_none_or(|allowed| allowed == organization_id)
        })
}

fn detail_scope(item: &WorkItem, actor_id: &str, access: &ActorAccess) -> Result<WorkItemScope> {
    if item.is_terminal()
        && (has_personal_history_access(item, actor_id)
            || (access.can_manage && covers_organization(access, &item.owner_organization_id)))
    {
        return Ok(WorkItemScope::History);
    }
    if item.is_owned_by(actor_id) {
        return Ok(WorkItemScope::Mine);
    }
    if item.status == WorkItemStatus::Open
        && item.assignment_mode == AssignmentMode::Pool
        && item.owner_user_id.is_none()
        && covers_responsibility(access, &item.owner_role, &item.owner_organization_id)
    {
        return Ok(WorkItemScope::Team);
    }
    if item.status == WorkItemStatus::Open
        && access.can_manage
        && covers_organization(access, &item.owner_organization_id)
    {
        return Ok(WorkItemScope::Managed);
    }
    Err(Error::Forbidden("当前账号无权查看该任务".to_string()))
}

fn has_personal_history_access(item: &WorkItem, actor_id: &str) -> bool {
    item.responsibility_actor_ids.iter().any(|id| id == actor_id)
        || item.completed_by.as_deref() == Some(actor_id)
        || item.closed_by.as_deref() == Some(actor_id)
}

fn covers_organization(access: &ActorAccess, organization_id: &str) -> bool {
    access
        .organization_ids
        .iter()
        .any(|id| id == "*" || id == organization_id)
}

fn apply_due_filter(filter: &mut WorkItemFilter, due: Option<WorkItemDueFilter>) -> Result<()> {
    let Some(due) = due else {
        return Ok(());
    };
    let (start, tomorrow) = business_day_bounds()?;
    match due {
        WorkItemDueFilter::Today => {
            filter.due_from = Some(start);
            filter.due_before = Some(tomorrow);
        }
        WorkItemDueFilter::Overdue => {
            filter.due_before = Some(Instant::now());
        }
    }
    Ok(())
}

fn business_day_bounds() -> Result<(Instant, Instant)> {
    business_day_bounds_at(Utc::now().timestamp())
}

fn business_day_bounds_at(now_unix_secs: i64) -> Result<(Instant, Instant)> {
    let timezone = FixedOffset::east_opt(8 * 60 * 60)
        .ok_or_else(|| Error::Internal("无法形成 Asia/Shanghai 时区".to_string()))?;
    let now = timezone
        .timestamp_opt(now_unix_secs, 0)
        .single()
        .ok_or_else(|| Error::Internal("无法读取统计时点".to_string()))?;
    let start = timezone
        .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .single()
        .ok_or_else(|| Error::Internal("无法形成业务日边界".to_string()))?;
    Ok((
        Instant::from_unix_secs(start.timestamp()),
        Instant::from_unix_secs((start + chrono::Duration::days(1)).timestamp()),
    ))
}

fn count_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn queue_context_id(actor_id: &str, query: &dto::WorkItemListQuery, access: &ActorAccess) -> String {
    stable_digest(&format!(
        "queue|{actor_id}|{}|{:?}|{:?}|{:?}|{:?}|{:?}|{}|{}|{:?}|{:?}|{}",
        query.scope.as_str(),
        query.work_item_types,
        query.statuses,
        query.due,
        query.priorities,
        query.query,
        query.sort_by,
        query.sort_ascending,
        access.responsibility_scopes,
        access.organization_ids,
        access.can_manage,
    ))
}

fn single_item_context_id(actor_id: &str, work_item_id: &str) -> String {
    stable_digest(&format!("queue-single|{actor_id}|{work_item_id}"))
}

fn ensure_queue_context(provided: &Option<String>, expected: &str) -> Result<()> {
    if provided.as_deref().is_none_or(|provided| provided == expected) {
        return Ok(());
    }
    Err(Error::ConflictError("队列上下文已变化，请刷新队列".to_string()))
}

fn idempotency_audit_id(actor_id: &str, action: &str, item_id: &str, key: &str) -> String {
    format!(
        "{IDEMPOTENCY_AUDIT_PREFIX}{}",
        stable_digest(&format!("{actor_id}|{action}|{item_id}|{key}"))
    )
}

fn command_fingerprint(parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

fn command_audit_message(fingerprint: &str, reason: Option<&str>) -> String {
    match reason {
        Some(reason) => format!("{COMMAND_FINGERPRINT_PREFIX}{fingerprint}; reason={reason}"),
        None => format!("{COMMAND_FINGERPRINT_PREFIX}{fingerprint}"),
    }
}

fn audit_command_fingerprint(message: &str) -> Option<&str> {
    message
        .strip_prefix(COMMAND_FINGERPRINT_PREFIX)
        .and_then(|value| value.split(';').next())
        .filter(|value| value.len() == 64)
}

fn stable_digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn required_text(value: &str, message: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::ValidationError(message.to_string()));
    }
    Ok(value.to_string())
}

fn expected_task_version(value: &str) -> Result<u64> {
    let value = value.trim();
    let version = value
        .parse::<u64>()
        .map_err(|_| Error::ValidationError("任务版本必须为正整数字符串".to_string()))?;
    if version == 0 {
        return Err(Error::ValidationError("任务版本必须为正整数字符串".to_string()));
    }
    Ok(version)
}

fn is_w29_closable(item: &WorkItem) -> bool {
    is_w29_shape(
        item.work_item_type,
        &item.business_object_type,
        item.approval_step_instance_id.is_some(),
    )
}

fn is_w29_fields_closable(item: &dto::WorkItemFields) -> bool {
    is_w29_shape(
        item.work_item_type,
        &item.business_object_type,
        item.approval_step_instance_id.is_some(),
    )
}

fn is_w29_shape(work_item_type: WorkItemType, business_object_type: &str, has_approval_step: bool) -> bool {
    !has_approval_step
        && matches!(
            (work_item_type, business_object_type),
            (
                WorkItemType::IntegrationResultUnknown | WorkItemType::BusinessException,
                "integration_error_task" | "reconciliation_difference"
            )
        )
}

fn w29_close_reason(
    reason_code: &str,
    comment: Option<&str>,
    replacement_id: Option<&str>,
) -> Result<String> {
    let comment = comment.map(str::trim).filter(|value| !value.is_empty());
    match reason_code.trim() {
        "DUPLICATE" => {
            let replacement_id = replacement_id
                .ok_or_else(|| Error::ValidationError("DUPLICATE 必须提供替代任务".to_string()))?;
            Ok(match comment {
                Some(comment) => format!("DUPLICATE replacement={replacement_id}: {comment}"),
                None => format!("DUPLICATE replacement={replacement_id}"),
            })
        }
        "MISROUTED" => {
            if replacement_id.is_some() {
                return Err(Error::ValidationError("MISROUTED 不得提供替代任务".to_string()));
            }
            let comment =
                comment.ok_or_else(|| Error::ValidationError("MISROUTED 必须填写原因说明".to_string()))?;
            Ok(format!("MISROUTED: {comment}"))
        }
        _ => Err(Error::ValidationError(
            "关闭原因只允许 DUPLICATE 或 MISROUTED".to_string(),
        )),
    }
}

fn w29_domain_evidence_reference(
    work_item_id: &str,
    reason_code: &str,
    replacement_work_item_id: Option<&str>,
    audit_id: &str,
) -> Result<String> {
    match reason_code {
        "DUPLICATE" => {
            let replacement_work_item_id = replacement_work_item_id
                .ok_or_else(|| Error::ValidationError("DUPLICATE 必须提供替代任务".to_string()))?;
            Ok(format!(
                "work_item:{work_item_id};replacement_work_item:{replacement_work_item_id};audit_log:{audit_id}"
            ))
        }
        "MISROUTED" => Ok(format!("work_item:{work_item_id};audit_log:{audit_id}")),
        _ => Err(Error::ValidationError(
            "关闭原因只允许 DUPLICATE 或 MISROUTED".to_string(),
        )),
    }
}

fn blocker(code: &str, message: &str) -> ProcessingBlockerView {
    ProcessingBlockerView {
        code: code.to_string(),
        message: message.to_string(),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct AuthorizedPage<T> {
    items: Vec<T>,
    total: i64,
}

/// 有界累加授权行：统计完整授权总数，仅保留请求页。
struct AuthorizedPageCollector<T> {
    start: u64,
    end: u64,
    total: u64,
    items: Vec<T>,
}

impl<T> AuthorizedPageCollector<T> {
    fn new(page: u64, page_size: u32) -> Result<Self> {
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

    fn extend(&mut self, authorized: impl IntoIterator<Item = T>) {
        for item in authorized {
            let position = self.total;
            self.total = self.total.saturating_add(1);
            if position >= self.start && position < self.end {
                self.items.push(item);
            }
        }
    }

    fn finish(self) -> AuthorizedPage<T> {
        AuthorizedPage {
            items: self.items,
            total: i64::try_from(self.total).unwrap_or(i64::MAX),
        }
    }
}

fn next_candidate_offset(current: u64, batch_len: usize) -> Result<u64> {
    let batch_len = u64::try_from(batch_len)
        .map_err(|_| Error::Internal("责任队列候选批次大小超出支持范围".to_string()))?;
    current
        .checked_add(batch_len)
        .ok_or_else(|| Error::Internal("责任队列候选偏移溢出".to_string()))
}

#[cfg(test)]
mod tests {
    use super::{
        allowed_actions, approval_assignment_separated, assignment_separation_policy,
        audit_command_fingerprint, audited_fact_operator_actors, authorized_fields, authorized_item_fields,
        business_day_bounds_at, command_audit_message, counts_as_processable_stat, detail_scope,
        expected_task_version, is_assignment_candidate_denial, is_w29_shape, non_empty_assignment_actors,
        object_access_shapes, object_policy, retain_team_eligible_fields, stable_digest, w29_close_reason,
        w29_domain_evidence_reference, ActorAccess, AssignmentSeparationPolicy, AuthorizedPage,
        AuthorizedPageCollector, Error, ObjectFact, ObjectFactMap, ObjectKind, ViewAccess,
        AUTHORIZED_SCAN_BATCH_SIZE,
    };
    use super::{ProcessingBlockerView, WorkItemAllowedAction, WorkItemScope};
    use entities::{
        approval::{ApprovalDecision, ApprovalStepInstance, ApprovalStepInstanceData, ApprovalStepStatus},
        common::time::Instant,
        ids::{ApprovalInstanceId, ApprovalStepInstanceId, WorkItemId},
        work_item::{
            AssignmentMode, AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemStatus,
            WorkItemType,
        },
        AccountKind, AuditLog, AuditLogData, Permission,
    };
    use std::collections::{HashMap, HashSet};

    fn w13_access() -> ActorAccess {
        ActorAccess {
            actor_id: "finance-user".to_string(),
            permissions: vec![Permission::parse("receivable_account:detail").unwrap()],
            participant_document_ids: HashSet::from(["sales-order-1".to_string()]),
            organization_ids: vec!["company".to_string()],
            responsibility_scopes: vec![("role-finance".to_string(), Some("company".to_string()))],
            can_manage: false,
        }
    }

    fn audit(resource_type: &str, resource_id: &str, action: &str, actor_id: &str) -> AuditLog {
        AuditLog::new(
            format!("audit-{resource_type}-{resource_id}-{action}-{actor_id}"),
            AuditLogData {
                actor_id: actor_id.to_string(),
                actor_account: actor_id.to_string(),
                actor_type: AccountKind::Admin,
                action: action.to_string(),
                resource_type: resource_type.to_string(),
                resource_id: Some(resource_id.to_string()),
                success: true,
                message: None,
            },
        )
        .unwrap()
    }

    fn w13_facts() -> ObjectFactMap {
        HashMap::from([(
            (ObjectKind::ReceivableAccount, "account-1".to_string()),
            ObjectFact::new("sales-order-1", "卡券应收子账 2", "sales-user"),
        )])
    }

    fn w13_delta_row() -> database::WorkItemRow {
        database::WorkItemRow {
            id: "wi-w13-delta".to_string(),
            work_item_type: WorkItemType::CardFundsDeltaReview,
            approval_step_instance_id: None,
            business_object_type: "receivable_account".to_string(),
            business_object_id: "account-1".to_string(),
            subject_version: "revision-2".to_string(),
            status: WorkItemStatus::Open,
            assignment_mode: AssignmentMode::Pool,
            owner_role: "role-finance".to_string(),
            owner_organization_id: "company".to_string(),
            owner_user_id: None,
            responsibility_actor_ids: Vec::new(),
            assignment_source: AssignmentSource::SystemRule,
            assigned_at: None,
            started_at: None,
            current_assignment_at: None,
            last_activity_at: None,
            priority: WorkItemPriority::High,
            due_at: None,
            reason_code: Some("CARD_FUNDS_DELTA_REVIEW".to_string()),
            impact_summary: Some("同步差额待复核".to_string()),
            completed_at: None,
            completed_by: None,
            closed_at: None,
            closed_by: None,
            close_reason: None,
            version: 1,
            created_at: 100,
            updated_at: 100,
        }
    }

    fn w13_delta_item() -> WorkItem {
        WorkItem::new_at(
            WorkItemId::new("wi-w13-delta-detail"),
            WorkItemData {
                work_item_type: WorkItemType::CardFundsDeltaReview,
                approval_step_instance_id: None,
                business_object_type: "receivable_account".to_string(),
                business_object_id: "account-1".to_string(),
                subject_version: "revision-2".to_string(),
                assignment_mode: AssignmentMode::Pool,
                owner_role: "role-finance".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: None,
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::High,
                due_at: None,
                reason_code: Some("CARD_FUNDS_DELTA_REVIEW".to_string()),
                impact_summary: Some("同步差额待复核".to_string()),
            },
            Instant::from_unix_secs(100),
        )
        .unwrap()
    }

    #[test]
    fn idempotency_digest_never_contains_raw_key() {
        let digest = stable_digest("actor|action|item|secret-request-key");
        assert_eq!(digest.len(), 64);
        assert!(!digest.contains("secret-request-key"));
    }

    #[test]
    fn close_reason_enforces_w29_evidence() {
        assert_eq!(
            w29_close_reason("DUPLICATE", Some("已有有效替代"), Some("wi-2")).unwrap(),
            "DUPLICATE replacement=wi-2: 已有有效替代"
        );
        assert!(w29_close_reason("DUPLICATE", None, None).is_err());
        assert!(w29_close_reason("MISROUTED", None, None).is_err());
        assert_eq!(
            w29_close_reason("MISROUTED", Some("对象类型登记错误"), None).unwrap(),
            "MISROUTED: 对象类型登记错误"
        );
    }

    #[test]
    fn w29_close_registry_excludes_other_business_exception_workspaces() {
        assert!(is_w29_shape(
            WorkItemType::IntegrationResultUnknown,
            "integration_error_task",
            false,
        ));
        assert!(is_w29_shape(
            WorkItemType::BusinessException,
            "reconciliation_difference",
            false,
        ));
        assert!(!is_w29_shape(
            WorkItemType::BusinessException,
            "MASTER_MAPPING_TASK",
            false,
        ));
        assert!(!is_w29_shape(
            WorkItemType::BusinessException,
            "SUPPLIER_OFFERING",
            false,
        ));
        assert!(!is_w29_shape(
            WorkItemType::BusinessException,
            "SUPPLIER_FULFILLMENT_ORDER",
            false,
        ));
        assert!(!is_w29_shape(
            WorkItemType::BusinessException,
            "integration_error_task",
            true,
        ));
    }

    #[test]
    fn w29_business_exception_integration_error_object_policy_is_registered() {
        let policy = object_policy(WorkItemType::BusinessException, "integration_error_task").unwrap();

        assert_eq!(policy.kind, ObjectKind::IntegrationErrorTask);
        assert_eq!(policy.read_permission, "integration_error_task:detail");
    }

    #[test]
    fn w13_delta_policy_authorizes_list_and_stats_projection() {
        let access = w13_access();
        let facts = w13_facts();
        let policy = object_policy(WorkItemType::CardFundsDeltaReview, "receivable_account").unwrap();

        assert_eq!(policy.kind, ObjectKind::ReceivableAccount);
        assert_eq!(policy.read_permission, "receivable_account:detail");
        assert!(object_access_shapes(&access).contains(&(
            WorkItemType::CardFundsDeltaReview,
            "receivable_account".to_string(),
        )));
        let fields = authorized_fields(vec![w13_delta_row()], &access, &facts);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].business_object_label, "卡券应收子账 2");
    }

    #[test]
    fn w13_delta_policy_authorizes_detail_projection() {
        let fields = authorized_item_fields(w13_delta_item(), &w13_access(), &w13_facts()).unwrap();

        assert_eq!(fields.work_item_type, WorkItemType::CardFundsDeltaReview);
        assert_eq!(fields.business_object_label, "卡券应收子账 2");
    }

    #[test]
    fn current_owner_does_not_bypass_revoked_object_participation() {
        let mut item = w13_delta_item();
        item.reassign("finance-user", Instant::from_unix_secs(101))
            .unwrap();
        let access = ActorAccess {
            actor_id: "finance-user".to_string(),
            permissions: vec![Permission::parse("receivable_account:detail").unwrap()],
            participant_document_ids: HashSet::new(),
            organization_ids: Vec::new(),
            responsibility_scopes: Vec::new(),
            can_manage: false,
        };

        assert_eq!(item.owner_user_id.as_deref(), Some("finance-user"));
        assert!(authorized_item_fields(item, &access, &w13_facts()).is_none());
    }

    #[test]
    fn w29_domain_evidence_uses_only_fixed_relations() {
        assert_eq!(
            w29_domain_evidence_reference("wi-1", "DUPLICATE", Some("wi-2"), "audit-1").unwrap(),
            "work_item:wi-1;replacement_work_item:wi-2;audit_log:audit-1"
        );
        assert_eq!(
            w29_domain_evidence_reference("wi-1", "MISROUTED", None, "audit-2").unwrap(),
            "work_item:wi-1;audit_log:audit-2"
        );
        assert!(w29_domain_evidence_reference("wi-1", "DUPLICATE", None, "audit-3").is_err());
    }

    #[test]
    fn audit_message_keeps_only_request_fingerprint_and_safe_reason() {
        let fingerprint = super::command_fingerprint(&["3", "user-1"]);
        let message = command_audit_message(&fingerprint, Some("主管转交"));

        assert_eq!(audit_command_fingerprint(&message), Some(fingerprint.as_str()));
        assert!(message.contains("主管转交"));
        assert!(!message.contains("raw-idempotency-key"));
    }

    #[test]
    fn business_day_bounds_use_fixed_asia_shanghai_timezone() {
        let (previous_start, previous_end) = business_day_bounds_at(1_722_441_599).unwrap();
        assert_eq!(previous_start.unix_secs(), 1_722_355_200);
        assert_eq!(previous_end.unix_secs(), 1_722_441_600);

        let (start, end) = business_day_bounds_at(1_722_441_600).unwrap();
        assert_eq!(start.unix_secs(), 1_722_441_600);
        assert_eq!(end.unix_secs(), 1_722_528_000);
    }

    #[test]
    fn blocked_items_never_enter_processable_stats() {
        let blocked = ViewAccess::blocked(ProcessingBlockerView {
            code: "APPROVAL_BLOCKED".to_string(),
            message: "审批当前受阻".to_string(),
        });
        assert!(!counts_as_processable_stat(WorkItemScope::Mine, &blocked));
        assert!(!counts_as_processable_stat(WorkItemScope::Team, &blocked));

        let mine = ViewAccess::ready(vec![WorkItemAllowedAction::Process]);
        let team = ViewAccess::ready(vec![WorkItemAllowedAction::StartProcessing]);
        assert!(counts_as_processable_stat(WorkItemScope::Mine, &mine));
        assert!(counts_as_processable_stat(WorkItemScope::Team, &team));
        assert!(!counts_as_processable_stat(WorkItemScope::Managed, &mine));
        assert!(!counts_as_processable_stat(WorkItemScope::History, &team));
    }

    #[test]
    fn former_responsibility_actor_can_open_terminal_history_detail() {
        let mut item = WorkItem::new_at(
            WorkItemId::new("wi-history"),
            WorkItemData {
                work_item_type: WorkItemType::ImportBusinessConfirmation,
                approval_step_instance_id: None,
                business_object_type: "LEGACY_IMPORT_BATCH".to_string(),
                business_object_id: "batch-1".to_string(),
                subject_version: "v1".to_string(),
                assignment_mode: AssignmentMode::Direct,
                owner_role: "role-sales".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: Some("alice".to_string()),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            Instant::from_unix_secs(100),
        )
        .unwrap();
        item.reassign("bob", Instant::from_unix_secs(110)).unwrap();
        item.complete_by_domain_command("bob", Instant::from_unix_secs(120))
            .unwrap();
        let access = |actor_id: &str| ActorAccess {
            actor_id: actor_id.to_string(),
            permissions: Vec::new(),
            participant_document_ids: HashSet::new(),
            organization_ids: Vec::new(),
            responsibility_scopes: Vec::new(),
            can_manage: false,
        };

        assert_eq!(
            detail_scope(&item, "alice", &access("alice")).unwrap(),
            WorkItemScope::History
        );
        assert!(detail_scope(&item, "charlie", &access("charlie")).is_err());
    }

    #[test]
    fn expected_task_version_accepts_only_positive_integer_strings() {
        assert_eq!(expected_task_version(" 7 ").unwrap(), 7);
        assert!(expected_task_version("0").is_err());
        assert!(expected_task_version("1.0").is_err());
        assert!(expected_task_version("latest").is_err());
    }

    #[test]
    fn authorized_pagination_reaches_later_batch_and_counts_full_total() {
        let access = w13_access();
        let facts = w13_facts();
        let mut collector = AuthorizedPageCollector::new(1, 2).unwrap();
        let first_candidate_batch = (0..AUTHORIZED_SCAN_BATCH_SIZE.get())
            .map(|index| {
                let mut row = w13_delta_row();
                row.id = format!("denied-{index}");
                row.business_object_id = format!("missing-{index}");
                row
            })
            .collect();
        let later_candidate_batch = (0..3)
            .map(|index| {
                let mut row = w13_delta_row();
                row.id = format!("allowed-{index}");
                row
            })
            .collect();

        collector.extend(authorized_fields(first_candidate_batch, &access, &facts));
        collector.extend(authorized_fields(later_candidate_batch, &access, &facts));

        let page = collector.finish();
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].id, "allowed-0");
        assert_eq!(page.items[1].id, "allowed-1");
        assert_eq!(page.total, 3);
    }

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
    fn team_queue_excludes_sod_conflicts_before_total_and_start_action() {
        let access = w13_access();
        let facts = w13_facts();
        let rows = ["submitter-task", "history-task", "eligible-task"]
            .into_iter()
            .map(|id| {
                let mut row = w13_delta_row();
                row.id = id.to_string();
                row
            })
            .collect();
        let fields = authorized_fields(rows, &access, &facts);
        assert_eq!(fields.len(), 3, "三条候选均具备读取与对象参与权");

        let eligible_ids = fields
            .iter()
            .filter(|field| match field.id.as_str() {
                "submitter-task" => approval_assignment_separated(
                    access.actor_id.as_str(),
                    "starter",
                    access.actor_id.as_str(),
                    &[],
                    None,
                    false,
                    &[],
                ),
                "history-task" => approval_assignment_separated(
                    access.actor_id.as_str(),
                    "starter",
                    "other-submitter",
                    std::slice::from_ref(&access.actor_id),
                    None,
                    false,
                    &[],
                ),
                "eligible-task" => approval_assignment_separated(
                    access.actor_id.as_str(),
                    "starter",
                    "other-submitter",
                    &[],
                    None,
                    false,
                    &[],
                ),
                _ => false,
            })
            .map(|field| field.id.clone())
            .collect::<HashSet<_>>();
        let denied = fields[0].clone();
        let eligible = fields[2].clone();
        let mut collector = AuthorizedPageCollector::new(1, 10).unwrap();
        collector.extend(retain_team_eligible_fields(fields, &eligible_ids));

        let page = collector.finish();
        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, "eligible-task");
        assert!(
            !allowed_actions(&denied, WorkItemScope::Team, &access.actor_id, &access, false)
                .contains(&WorkItemAllowedAction::StartProcessing)
        );
        assert!(
            allowed_actions(&eligible, WorkItemScope::Team, &access.actor_id, &access, true)
                .contains(&WorkItemAllowedAction::StartProcessing)
        );
    }

    #[test]
    fn missing_authoritative_assignment_facts_fail_closed_as_candidate_denial() {
        assert!(is_assignment_candidate_denial(&Error::Forbidden(
            "审批业务对象事实缺失".to_string()
        )));
        assert!(is_assignment_candidate_denial(&Error::ConflictError(
            "审批步骤责任事实缺失".to_string()
        )));
    }

    #[test]
    fn approval_assignment_excludes_submitter_starter_history_and_decider() {
        let mut decided_step = ApprovalStepInstance::new(
            ApprovalStepInstanceId::new("step-1"),
            ApprovalStepInstanceData {
                approval_instance_id: ApprovalInstanceId::new("approval-1"),
                step_key: "manager".to_string(),
                sequence_no: 1,
                initial_status: ApprovalStepStatus::Active,
                external_activity_id: None,
            },
        )
        .unwrap();
        decided_step
            .decide(
                ApprovalDecision::Approve,
                None,
                "previous-decider",
                Instant::from_unix_secs(100),
            )
            .unwrap();
        let steps = vec![decided_step];
        let history = vec!["former-owner".to_string()];

        assert!(!approval_assignment_separated(
            "starter",
            "starter",
            "submitter",
            &history,
            None,
            false,
            &steps,
        ));
        assert!(!approval_assignment_separated(
            "submitter",
            "starter",
            "submitter",
            &history,
            None,
            false,
            &steps,
        ));
        assert!(!approval_assignment_separated(
            "former-owner",
            "starter",
            "submitter",
            &history,
            None,
            false,
            &steps,
        ));
        assert!(!approval_assignment_separated(
            "previous-decider",
            "starter",
            "submitter",
            &history,
            None,
            false,
            &steps,
        ));
        assert!(approval_assignment_separated(
            "next-owner",
            "starter",
            "submitter",
            &history,
            None,
            false,
            &steps,
        ));
    }

    #[test]
    fn assignment_postcheck_allows_only_the_new_current_owner() {
        let history = vec!["candidate".to_string()];
        assert!(!approval_assignment_separated(
            "candidate",
            "starter",
            "submitter",
            &history,
            Some("candidate"),
            false,
            &[],
        ));
        assert!(approval_assignment_separated(
            "candidate",
            "starter",
            "submitter",
            &history,
            Some("candidate"),
            true,
            &[],
        ));
        assert!(!approval_assignment_separated(
            "candidate",
            "starter",
            "submitter",
            &history,
            Some("other-owner"),
            true,
            &[],
        ));
    }

    #[test]
    fn formal_decision_task_types_use_fixed_assignment_separation_policies() {
        for work_item_type in [
            WorkItemType::LowMarginManagerConfirmation,
            WorkItemType::ProcurementConfirmation,
            WorkItemType::PurchaseOrderReview,
            WorkItemType::SalesChangeImpactReview,
            WorkItemType::SalesChangeFinanceReview,
            WorkItemType::CardFundsReview,
            WorkItemType::CardFundsDeltaReview,
            WorkItemType::InventoryAdjustmentReview,
            WorkItemType::SupplierSettlementReview,
        ] {
            assert_eq!(
                assignment_separation_policy(work_item_type),
                AssignmentSeparationPolicy::DomainActors
            );
        }
        assert_eq!(
            assignment_separation_policy(WorkItemType::CardSalesManagerApproval),
            AssignmentSeparationPolicy::ApprovalHistory
        );
        assert_eq!(
            assignment_separation_policy(WorkItemType::FinanceCorrectionReview),
            AssignmentSeparationPolicy::FailClosed
        );
    }

    #[test]
    fn domain_assignment_actor_facts_fail_closed_when_empty() {
        assert!(non_empty_assignment_actors(Vec::new()).is_err());
        assert!(non_empty_assignment_actors(vec!["  ".to_string(), "__system__".to_string()]).is_err());
        assert_eq!(
            non_empty_assignment_actors(vec![" submitter ".to_string(), "submitter".to_string()]).unwrap(),
            HashSet::from(["submitter".to_string()])
        );
    }

    #[test]
    fn card_funds_assignment_excludes_all_receipt_and_invoice_operators() {
        let receipt_ids = HashSet::from(["receipt-1".to_string()]);
        let receipt_audits = vec![
            audit(
                "customer_receipt",
                "receipt-1",
                "customer_receipt.create",
                "receipt-creator",
            ),
            audit(
                "customer_receipt",
                "receipt-1",
                "customer_receipt.post:receipt-1",
                "receipt-poster",
            ),
        ];
        let invoice_ids = HashSet::from(["blue-1".to_string(), "red-1".to_string()]);
        let invoice_audits = vec![
            audit("invoice", "blue-1", "invoice.create", "invoice-creator"),
            audit("invoice", "blue-1", "invoice.post", "invoice-poster"),
            audit("invoice", "red-1", "invoice.red_issue", "red-issuer"),
        ];

        let mut actors = audited_fact_operator_actors(
            "customer_receipt",
            &receipt_ids,
            &receipt_audits,
            &["customer_receipt.create", "customer_receipt.post:"],
            &["customer_receipt.post:"],
        )
        .unwrap();
        actors.extend(
            audited_fact_operator_actors(
                "invoice",
                &invoice_ids,
                &invoice_audits,
                &["invoice.create", "invoice.post", "invoice.red_issue"],
                &["invoice.post", "invoice.red_issue"],
            )
            .unwrap(),
        );

        assert_eq!(
            actors.into_iter().collect::<HashSet<_>>(),
            HashSet::from([
                "receipt-creator".to_string(),
                "receipt-poster".to_string(),
                "invoice-creator".to_string(),
                "invoice-poster".to_string(),
                "red-issuer".to_string(),
            ])
        );
    }

    #[test]
    fn card_funds_assignment_fails_closed_without_formal_audit_for_every_fact() {
        let receipt_ids = HashSet::from(["receipt-1".to_string(), "receipt-2".to_string()]);
        let audits = vec![
            audit(
                "customer_receipt",
                "receipt-1",
                "customer_receipt.post:receipt-1",
                "poster-1",
            ),
            audit(
                "customer_receipt",
                "receipt-2",
                "customer_receipt.create",
                "creator-2",
            ),
        ];

        assert!(matches!(
            audited_fact_operator_actors(
                "customer_receipt",
                &receipt_ids,
                &audits,
                &["customer_receipt.create", "customer_receipt.post:"],
                &["customer_receipt.post:"],
            ),
            Err(Error::Forbidden(_))
        ));
    }
}
