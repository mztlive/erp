//! D03 人工任务责任查询与责任动作编排。
//!
//! 查询范围由认证身份、RBAC 角色与数据范围形成；客户端不能提交任意责任人或
//! 组织扩大范围。正式业务决定继续由各任务类型的强类型命令与审批运行时完成。

use std::{
    collections::{HashMap, HashSet},
    num::NonZeroU32,
};

use database::{
    AccessControlExt, BpmExt, DocumentRegistryExt, Executor, IntegrationOpsExt, InventoryExt,
    LegacyImportExt, MallSyncExt, MongoCasbinAdapter, NoTransaction, PurchaseOrderExt, ReceivableExt,
    SalesOrderExt, SalesReviewExt, SupplierFulfillmentExt, SupplierOfferingExt, SupplierSettlementExt,
    Transactional, WorkItemExt,
};
use entities::supplier_offering::{AvailabilityStatus, OfferingStatus};
use entities::{
    access_control::{DataScope, DataScopeSubjectType, OrganizationCoverage, ResponsibilityScopeSet},
    common::time::Instant,
    integration_ops::{
        ErrorClass, ErrorTaskStatus, IntegrationErrorTask, ReconciliationDifference,
        ReconciliationDifferenceId, ReconciliationDifferenceResolution, ReconciliationDifferenceResolutionId,
        ResolutionType, W29CloseDecision, W29EvidenceReference,
    },
    work_item::{
        AvailableWorkItemAccount, FulfillmentResponsibilityKey, QueueContextField, QueueContextIdentity,
        WorkItem, WorkItemAssignmentSeparationPolicy, WorkItemBriefObjectKind, WorkItemBriefRelation,
        WorkItemCloseData, WorkItemStatus, WorkItemSubjectVersions, WorkItemType,
    },
    CommandFingerprint, CommandReceipt, Permission, PermissionSet,
};
use mongodb::Database;
use validator::Validate;

use crate::{
    audit::{AuditActor, CommandReceiptServiceExt as _},
    errors::{Error, ErrorCode, Result},
    iam::SharedRbacService,
};

mod brief;
mod change_order_brief;
mod dto;
mod finance_responsibility;
mod fulfillment_operation_brief;
mod fulfillment_queue;
mod funds_document_brief;
mod inventory_settlement_brief;
mod party_names;
mod presentation;
mod procurement_brief;
mod purchase_review_brief;
mod sales_order_brief;

pub use dto::{
    CloseWorkItemRequest, ProcessingBlockerView, ProcessingState, ReassignWorkItemRequest,
    WorkItemAllowedAction, WorkItemApprovalContextView, WorkItemConflict, WorkItemConflictKind,
    WorkItemDueFilter, WorkItemFamily, WorkItemFamilyCountsView, WorkItemListParams, WorkItemMutationOutcome,
    WorkItemPageView, WorkItemPartyView, WorkItemReassignCandidateView, WorkItemScope, WorkItemSort,
    WorkItemStatsParams, WorkItemStatsView, WorkItemView,
};
pub(crate) use finance_responsibility::ResolvedFinanceResponsibility;
pub use finance_responsibility::{
    CreateFinanceResponsibilityRuleRequest, FinanceResponsibilityOwnerOptionView,
    FinanceResponsibilityRuleView, UpdateFinanceResponsibilityRuleRequest,
};
pub use fulfillment_queue::{
    FulfillmentQueueGateFilter, FulfillmentQueueGateState, FulfillmentQueueItemView,
    FulfillmentQueueListParams, FulfillmentQueueMetricView, FulfillmentQueueOperationType,
    FulfillmentQueuePageView, FulfillmentQueueWarehouseView,
};
type WorkItemFilter = <mongodb::Database as WorkItemExt>::WorkItemFilter;

const MANAGE_PERMISSION: &str = "work_item:manage";
const REASSIGN_PERMISSION: &str = "work_item:reassign";
const CLOSE_PERMISSION: &str = "work_item:close";
const IDEMPOTENCY_AUDIT_PREFIX: &str = "work-item-command-";
const REASSIGN_VERSION_CONFLICT: &str = "任务版本已变化";
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
    assignee_permissions: Vec<Permission>,
}

struct FocusedQueueContext<'a> {
    page_size: u32,
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
    /// 返回个人、今日到期、超期、异常、任务族计数及服务端统计时点。
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
        let family_items = if params.family.is_none() && params.work_item_type.is_none() {
            assigned.clone()
        } else {
            let mut family_query = query.clone();
            family_query.work_item_types = registered_work_item_types();
            let family_items = self
                .stats_fields_for_open_scope(&family_query, WorkItemScope::Mine, actor, &access)
                .await?;
            self.processable_stats_fields(family_items, WorkItemScope::Mine, actor, &access)
                .await?
        };
        let as_of = Instant::now();
        let (today_start, tomorrow_start) = business_day_bounds()?;
        Ok(WorkItemStatsView {
            assigned: count_u64(assigned.len()),
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
            family_counts: family_counts_for_types(family_items.iter().map(|item| item.work_item_type)),
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
                    .map(|policy| (policy.object_kind, row.business_object_id.clone()))
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
        self.authorized_stat_fields(filter, access).await
    }

    async fn stats_fields_for_open_scope(
        &self,
        query: &dto::WorkItemListQuery,
        scope: WorkItemScope,
        actor: &AuditActor,
        access: &ActorAccess,
    ) -> Result<Vec<dto::WorkItemFields>> {
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
            fields.extend(authorized);
            candidate_offset = next_candidate_offset(candidate_offset, candidate_count)?;
            if candidate_count < AUTHORIZED_SCAN_BATCH_SIZE.get() as usize {
                break;
            }
        }
        Ok(fields)
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
                    .map(|policy| (policy.object_kind, item.business_object_id.clone()))
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
        self.load_fulfillment_operation_facts(keys, &mut facts, executor)
            .await?;
        self.load_purchase_change_facts(keys, &mut facts, executor)
            .await?;
        self.load_sales_change_review_facts(keys, &mut facts, executor)
            .await?;
        self.load_receivable_account_facts(keys, &mut facts, executor)
            .await?;
        self.load_payable_account_facts(keys, &mut facts, executor)
            .await?;
        self.load_customer_receipt_facts(keys, &mut facts, executor)
            .await?;
        self.load_customer_refund_facts(keys, &mut facts, executor)
            .await?;
        self.load_receipt_reversal_facts(keys, &mut facts, executor)
            .await?;
        self.load_supplier_payment_facts(keys, &mut facts, executor)
            .await?;
        self.load_supplier_refund_facts(keys, &mut facts, executor)
            .await?;
        self.load_payment_reversal_facts(keys, &mut facts, executor)
            .await?;
        self.load_independent_object_facts(keys, &mut facts, executor)
            .await?;
        self.load_master_mapping_task_facts(keys, &mut facts, executor)
            .await?;
        Ok(facts)
    }

    /// 装载库存、结算、导入、集成和供应侧对象事实。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入已注册独立对象的事实。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
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
            .list_work_item_brief_entities_by_ids(&ids, executor)
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
            .list_work_item_brief_entities_by_ids(&ids, executor)
            .await?
        {
            let owner = task
                .owner_user_id
                .clone()
                .unwrap_or_else(|| SYSTEM_OBJECT_OWNER.to_string());
            let mut fact = ObjectFact::new(
                task.base.id.clone(),
                format!("集成异常 · {}", task.error_class.label()),
                owner,
            );
            fact.impact_summary = Some(integration_error_impact(&task).to_string());
            fact.brief_source = Some(integration_error_brief_source(&task));
            facts.insert((ObjectKind::IntegrationErrorTask, task.base.id.clone()), fact);
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
            .list_work_item_brief_entities_by_ids(&ids, executor)
            .await?
        {
            let mut fact = ObjectFact::new(
                difference.base.id.clone(),
                format!("业务异常 · {}", difference.difference_type),
                SYSTEM_OBJECT_OWNER,
            );
            fact.impact_summary =
                Some("需核对两侧不可变证据后处理差异，不得直接改写正式业务事实".to_string());
            fact.brief_source = Some(reconciliation_difference_brief_source(&difference));
            facts.insert(
                (ObjectKind::ReconciliationDifference, difference.base.id.clone()),
                fact,
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
            .list_work_item_brief_entities_by_ids(&ids, executor)
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
            .list_work_item_brief_entities_by_ids(&ids, executor)
            .await?
        {
            facts.insert(
                (ObjectKind::SupplierFulfillmentOrder, order.base.id.clone()),
                ObjectFact {
                    root_document_id: order.mall_order_id.to_string(),
                    label: format!("供应商履约订单 {}", order.fulfillment_order_no),
                    created_by: SYSTEM_OBJECT_OWNER.to_string(),
                    subject_versions: WorkItemSubjectVersions::constrained(vec![order
                        .base
                        .version
                        .to_string()])?,
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
            .list_work_item_brief_entities_by_ids(&ids, executor)
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
                    subject_versions: WorkItemSubjectVersions::constrained(subject_versions)?,
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
    async fn apply_approval_contexts(&self, items: &mut [WorkItemView]) -> Result<()> {
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

    /// 查询当前开放非审批任务可转交的具体账号。
    ///
    /// # 参数
    /// * `id` - 工作项稳定 ID
    /// * `actor` - 已通过鉴权且具有责任管理范围的操作人
    ///
    /// # 返回
    /// 返回当前仍有效、具备完整执行权限且满足任务责任约束的账号。
    ///
    /// # 错误
    /// 任务不存在、不是开放非审批任务、操作人不在管理范围，或授权事实读取失败时返回错误。
    ///
    /// # 关键业务约束
    /// 采购单责任任务的候选人必须同时能够执行该采购单全部开放履约任务；列表只作交互提示，
    /// 最终转交命令仍须在写事务内重验全部账号、授权与业务事实。
    pub async fn reassign_candidates(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<Vec<WorkItemReassignCandidateView>> {
        let managed_access = self.managed_access(actor).await?;
        let item = self.load(id).await?;
        ensure_generic_work_item_mutation(&item)?;
        ensure_item_in_managed_scope(&item, &managed_access)?;

        let purchase_order_id = purchase_order_fulfillment_responsibility_id(&item)?;
        let cascade_tasks = if let Some(purchase_order_id) = purchase_order_id.as_deref() {
            let (_, tasks) =
                load_purchase_order_fulfillment_scope(&self.db, &item, purchase_order_id, &mut NoTransaction)
                    .await?;
            Some(tasks)
        } else {
            None
        };

        let accounts = self
            .db
            .accounts()
            .list_by_kind(entities::AccountKind::Admin, &mut NoTransaction)
            .await?;
        let mut candidates = Vec::new();
        for account in accounts {
            if item.owner_user_id.as_deref() == Some(account.base.id.as_str())
                || AvailableWorkItemAccount::from_account(&account).is_err()
            {
                continue;
            }
            let authorization = match self
                .assignment_authorization_snapshot(actor, &account.base.id, &item, true)
                .await
            {
                Ok(authorization) => authorization,
                Err(Error::Forbidden(_)) => continue,
                Err(error) => return Err(error),
            };
            if let Some(tasks) = cascade_tasks.as_deref() {
                match ensure_fulfillment_tasks_candidate(
                    self,
                    tasks,
                    &account.base.id,
                    &authorization.assignee_permissions,
                    &mut NoTransaction,
                )
                .await
                {
                    Ok(()) => {}
                    Err(Error::Forbidden(_)) => continue,
                    Err(error) => return Err(error),
                }
            }
            let login_account = account.secret.account().to_string();
            candidates.push(WorkItemReassignCandidateView {
                user_id: account.base.id,
                display_name: account.name,
                account: login_account,
            });
        }
        candidates.sort_by(|left, right| {
            left.display_name
                .cmp(&right.display_name)
                .then_with(|| left.account.cmp(&right.account))
                .then_with(|| left.user_id.cmp(&right.user_id))
        });
        Ok(candidates)
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
        let managed_access = self.managed_access(actor).await?;
        let item = self.load(id).await?;
        ensure_generic_work_item_mutation(&item)?;
        req.validate()?;
        let idempotency_key = required_text(&req.idempotency_key, "幂等键不能为空")?;
        let action = "work_item.reassign";
        let target_user_id = required_text(&req.target_user_id, "目标用户不能为空")?;
        let reason = required_text(&req.reason, "转交原因不能为空")?;
        let expected_task_version = expected_task_version(&req.expected_task_version)?;
        let version = expected_task_version.to_string();
        let receipt = CommandReceipt::from_resource_parts(
            IDEMPOTENCY_AUDIT_PREFIX,
            actor.id(),
            action,
            "work_item",
            id,
            &idempotency_key,
            [version, target_user_id.clone(), reason.clone()],
        )?;
        if let Some(replayed) = self.idempotent_replay(&receipt, id).await? {
            ensure_generic_work_item_mutation(&replayed)?;
            return self.applied_outcome(replayed, actor).await;
        }
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
            .reassign_with_assignment_policy_audit(AssignmentPolicyAuditInput {
                item,
                expected_task_version,
                target_user_id,
                actor,
                receipt,
                audit_detail: reason,
                authorization,
            })
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
        let managed_access = self.managed_access(actor).await?;
        let item = self.load(id).await?;
        ensure_generic_work_item_mutation(&item)?;
        req.validate()?;
        let idempotency_key = required_text(&req.idempotency_key, "幂等键不能为空")?;
        let action = "work_item.close";
        let reason_code = required_text(&req.reason_code, "关闭原因代码不能为空")?;
        let replacement_id = req
            .replacement_work_item_id
            .as_deref()
            .map(|value| required_text(value, "替代任务ID不能为空"))
            .transpose()?;
        let decision =
            W29CloseDecision::new(&reason_code, req.comment.as_deref(), replacement_id.as_deref())?;
        let expected_task_version = expected_task_version(&req.expected_task_version)?;
        let version = expected_task_version.to_string();
        let receipt = CommandReceipt::from_resource_parts(
            IDEMPOTENCY_AUDIT_PREFIX,
            actor.id(),
            action,
            "work_item",
            id,
            &idempotency_key,
            [
                version,
                decision.close_reason().to_string(),
                decision
                    .replacement_work_item_id()
                    .unwrap_or_default()
                    .to_string(),
            ],
        )?;
        if let Some(replayed) = self.idempotent_replay(&receipt, id).await? {
            ensure_generic_work_item_mutation(&replayed)?;
            return self.applied_outcome(replayed, actor).await;
        }
        if item.base.version != expected_task_version {
            return self
                .conflict_outcome(id, WorkItemConflictKind::Version, actor)
                .await;
        }
        ensure_item_in_managed_scope(&item, &managed_access)?;
        self.ensure_object_participation(actor, &item).await?;
        if !item.is_w29_closable() {
            return Err(Error::BusinessLogicError(
                "只有 W29 登记的异常任务允许受控关闭".to_string(),
            ));
        }
        if let Some(replacement_id) = decision.replacement_work_item_id() {
            self.ensure_w29_replacement(&item, replacement_id, actor, &managed_access)
                .await?;
        }
        let updated = self
            .close_with_domain_evidence(CloseDomainEvidenceInput {
                item,
                actor,
                decision,
                receipt,
            })
            .await?;
        match updated {
            WorkItemWriteOutcome::Updated(item) => self.applied_outcome(*item, actor).await,
            WorkItemWriteOutcome::VersionConflict => {
                self.conflict_outcome(id, WorkItemConflictKind::Version, actor)
                    .await
            }
        }
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
    async fn load(&self, id: &str) -> Result<WorkItem> {
        self.db
            .work_items()
            .find_work_item(id, &mut NoTransaction)
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
        if !replacement.is_w29_replacement_for(current) {
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
            .document_ids_by_user(actor_id, &mut NoTransaction)
            .await?
            .into_iter()
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
        let user_subject_ids = vec![actor_id.to_string()];
        let user_scopes = self
            .db
            .data_scopes()
            .list_by_subjects(DataScopeSubjectType::User, &user_subject_ids, &mut NoTransaction)
            .await?;
        let role_scopes = self
            .db
            .data_scopes()
            .list_by_subjects(DataScopeSubjectType::Role, role_ids, &mut NoTransaction)
            .await?
            .into_iter()
            .fold(HashMap::<String, Vec<DataScope>>::new(), |mut grouped, scope| {
                grouped.entry(scope.subject_id.clone()).or_default().push(scope);
                grouped
            });
        let mut responsibility_scopes = Vec::new();
        let mut management_scopes = Vec::new();
        for role_id in role_ids {
            let role_scopes = role_scopes.get(role_id).map(Vec::as_slice).unwrap_or_default();
            let pairs = responsibility_scope_for_role(role_id, role_scopes, &user_scopes);
            responsibility_scopes.extend(pairs.iter().cloned());
            if manage_role_ids.contains(role_id) {
                management_scopes.extend(pairs);
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
        if let Some(blocker) = self.processing_blocker(None::<&str>).await? {
            return Ok(ViewAccess::blocked(blocker));
        }
        if scope == WorkItemScope::History || item.status != WorkItemStatus::Open {
            return Ok(ViewAccess::ready(Vec::new()));
        }
        let actions = allowed_actions(item, scope, actor.id(), access);
        Ok(ViewAccess::ready(actions))
    }

    async fn processing_blocker(&self, _step_id: Option<&str>) -> Result<Option<ProcessingBlockerView>> {
        Ok(None)
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
                .find_work_item_account(assignee_id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::Forbidden("目标账号不存在或已失效".to_string()))?;
            let assignee = AvailableWorkItemAccount::from_account(&assignee)
                .map_err(|_| Error::Forbidden("目标账号不存在或已失效".to_string()))?;
            let assignee_role_ids =
                active_role_ids(&self.db, assignee.kind(), assignee_id, &mut NoTransaction).await?;
            let assignee_permissions = self.rbac.permissions(assignee.kind(), assignee_id).await?;
            let assignee_read_role_ids = self
                .roles_granting_permission(&assignee_role_ids, &read_permission, true)
                .await?;
            let snapshot = AssignmentAuthorizationSnapshot {
                policy_revision: before,
                actor_kind: actor.kind(),
                assignee_kind: assignee.kind(),
                read_permission: read_permission.clone(),
                actor_read_role_ids,
                actor_manage_role_ids,
                assignee_read_role_ids,
                assignee_permissions,
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
                assignee.kind(),
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
        let account = self
            .db
            .accounts()
            .find_work_item_account(actor_id, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("操作账号不存在、已停用或身份已变化".to_string()))?;
        AvailableWorkItemAccount::from_account_kind(&account, actor_kind)
            .map_err(|_| Error::Forbidden("操作账号不存在、已停用或身份已变化".to_string()))?;
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

    /// 在调用方事务快照中重验目标账号资格、对象访问权与岗位分离。
    ///
    /// # 参数
    /// * `user_id` - 待接收任务的具体账号 ID
    /// * `expected_kind` - 事务外授权快照冻结的账号类型
    /// * `item` - 待转交任务
    /// * `authorization` - 事务外形成的稳定授权快照
    /// * `allow_current_owner` - 是否允许目标账号保持为当前负责人
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 目标账号仍有效、具备任务所需权限且满足岗位分离时返回 `Ok(())`。
    ///
    /// # 错误
    /// 账号失效、权限撤销、对象版本变化或岗位分离不满足时返回错误。
    async fn ensure_assignment_candidate(
        &self,
        user_id: &str,
        expected_kind: entities::AccountKind,
        item: &WorkItem,
        authorization: &AssignmentAuthorizationSnapshot,
        allow_current_owner: bool,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let account = self
            .db
            .accounts()
            .find_work_item_account(user_id, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("目标账号不存在或已失效".to_string()))?;
        AvailableWorkItemAccount::from_account_kind(&account, expected_kind)
            .map_err(|_| Error::Forbidden("目标账号不存在或已失效".to_string()))?;
        let mut access = self
            .assignment_access_for_executor(
                expected_kind,
                user_id,
                &authorization.read_permission,
                &authorization.assignee_read_role_ids,
                &[],
                executor,
            )
            .await?;
        if item.work_item_type.requires_full_execution_permissions() {
            // 执行任务除注册表读取权限外，还要求目标工作面使用的完整操作权限。
            // 该快照由同一 policy revision 形成，外层授权事务会以该 revision
            // 做 CAS 后才允许提交。
            access.permissions = authorization.assignee_permissions.clone();
        }
        self.ensure_assignment_candidate_access_with_executor(item, &access, executor)
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
        match item.work_item_type.assignment_separation_policy() {
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
        let _ = (user_id, item, allow_current_owner, executor);
        Err(Error::Forbidden(
            "单据审批任务不得通过通用责任入口改派".to_string(),
        ))
    }

    /// 读取非审批正式决定任务的权威提交人、经办人及历史决定人。
    async fn domain_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<HashSet<String>> {
        let actors = match item.work_item_type {
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

    /// 读取采购审核提交人和既往审核人用于岗位分离。
    ///
    /// # 参数
    /// * `item` - 采购审核工作项
    /// * `executor` - 当前责任变更事务执行器
    ///
    /// # 返回
    /// 提交与任务对象一致时返回提交人和可选审核人。
    ///
    /// # 错误
    /// 提交缺失、对象关系不一致或提交人缺失时返回错误。
    async fn purchase_review_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let submission = self
            .db
            .purchase_order_submissions()
            .find_work_item_purchase_submission(&item.subject_version, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("采购提交事实缺失".to_string()))?;
        if !item.matches_business_object("purchase_order", submission.purchase_order_id.as_ref()) {
            return Err(Error::Forbidden("采购提交与任务对象不一致".to_string()));
        }
        let submitted_by = submission
            .submitted_by
            .ok_or_else(|| Error::Forbidden("采购提交人事实缺失".to_string()))?;
        Ok(optional_actors([Some(submitted_by), submission.reviewed_by]))
    }

    /// 读取销售变更提交人用于岗位分离。
    ///
    /// # 参数
    /// * `item` - 销售变更复核工作项
    /// * `executor` - 当前责任变更事务执行器
    ///
    /// # 返回
    /// 提交与任务对象一致时返回提交人账号 ID。
    ///
    /// # 错误
    /// 提交缺失、对象关系不一致或仓储查询失败时返回错误。
    async fn sales_change_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let submission = self
            .db
            .sales_change_submissions()
            .find_work_item_sales_change_submission(&item.subject_version, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("销售变更提交事实缺失".to_string()))?;
        if !item.matches_business_object("sales_change_review", submission.sales_change_order_id.as_ref()) {
            return Err(Error::Forbidden("销售变更提交与任务对象不一致".to_string()));
        }
        Ok(vec![submission.submitted_by])
    }

    /// 读取卡券票款任务的权威经办、登记与复核账号用于岗位分离。
    ///
    /// # 参数
    /// * `item` - 卡券票款复核工作项
    /// * `executor` - 当前责任变更事务执行器
    ///
    /// # 返回
    /// 返回应收、回款、发票与复核事实中的全部责任账号。
    ///
    /// # 错误
    /// 对象类型、状态、版本或票款正式事实无法证明时返回错误。
    async fn card_funds_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        if item.business_object_type != "receivable_account" {
            return Err(Error::Forbidden("卡券票款任务责任事实不合法".to_string()));
        }
        let account = self
            .db
            .receivable_accounts()
            .find_work_item_receivable_account(&item.business_object_id, executor)
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
            .find_work_item_sales_order(account.sales_order_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::Forbidden("应收子账来源销售单事实缺失".to_string()))?;
        let revision_id = order
            .stable
            .current_revision_id
            .ok_or_else(|| Error::Forbidden("应收子账来源销售单缺少当前正式版本".to_string()))?;
        if !item.matches_subject_version(&revision_id) {
            return Err(Error::Forbidden("票款复核任务已不是当前销售版本".to_string()));
        }
        self.db
            .sales_order_revisions()
            .find_work_item_sales_order_revision(&revision_id, executor)
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
            .list_work_item_brief_entities_by_ids(&ids.iter().cloned().collect::<Vec<_>>(), executor)
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
            .list_work_item_brief_entities_by_ids(&ids.iter().cloned().collect::<Vec<_>>(), executor)
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
        let resource_id_list = resource_ids.iter().cloned().collect::<Vec<_>>();
        let audits = self
            .db
            .audit_logs()
            .list_successful_work_item_fact_audits(resource_type, &resource_id_list, executor)
            .await?;
        audited_fact_operator_actors(
            resource_type,
            resource_ids,
            &audits,
            operator_actions,
            formal_actions,
        )
    }

    /// 读取库存调整任务的制单人与既往复核人用于岗位分离。
    ///
    /// # 参数
    /// * `item` - 库存调整复核工作项
    /// * `executor` - 当前责任变更事务执行器
    ///
    /// # 返回
    /// 返回存在的制单、业务复核与财务复核账号 ID。
    ///
    /// # 错误
    /// 库存调整事实缺失或仓储查询失败时返回错误。
    async fn inventory_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let adjustment = self
            .db
            .stock_adjustments()
            .find_work_item_stock_adjustment(&item.business_object_id, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("库存调整事实缺失".to_string()))?;
        Ok(optional_actors([
            Some(adjustment.prepared_by),
            adjustment.reviewed_by,
            adjustment.finance_reviewed_by,
        ]))
    }

    /// 读取供应商结算任务的制单人与既往复核人用于岗位分离。
    ///
    /// # 参数
    /// * `item` - 供应商结算复核工作项
    /// * `executor` - 当前责任变更事务执行器
    ///
    /// # 返回
    /// 任务版本仍匹配时返回存在的制单与复核账号 ID。
    ///
    /// # 错误
    /// 结算事实缺失、版本不一致或仓储查询失败时返回错误。
    async fn settlement_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let statement = self
            .db
            .supplier_settlement_statements()
            .find_work_item_supplier_settlement(&item.business_object_id, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("供应商结算事实缺失".to_string()))?;
        if !item.matches_subject_version(&statement.subject_hash) {
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
            .document_ids_by_user(actor_id, executor)
            .await?
            .into_iter()
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
        let user_subject_ids = vec![actor_id.to_string()];
        let user_scopes = self
            .db
            .data_scopes()
            .list_by_subjects(DataScopeSubjectType::User, &user_subject_ids, executor)
            .await?;
        let role_scopes = self
            .db
            .data_scopes()
            .list_by_subjects(DataScopeSubjectType::Role, role_ids, executor)
            .await?
            .into_iter()
            .fold(HashMap::<String, Vec<DataScope>>::new(), |mut grouped, scope| {
                grouped.entry(scope.subject_id.clone()).or_default().push(scope);
                grouped
            });
        let mut responsibility_scopes = Vec::new();
        let mut management_scopes = Vec::new();
        for role_id in role_ids {
            let role_scopes = role_scopes.get(role_id).map(Vec::as_slice).unwrap_or_default();
            let pairs = responsibility_scope_for_role(role_id, role_scopes, &user_scopes);
            responsibility_scopes.extend(pairs.iter().cloned());
            if manage_role_ids.contains(role_id) {
                management_scopes.extend(pairs);
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
        let keys = HashSet::from([(policy.object_kind, item.business_object_id.clone())]);
        let facts = self.load_object_facts(&keys, executor).await?;
        if authorized_item_fields(item.clone(), access, &facts).is_none() {
            return Err(Error::Forbidden("业务对象不可访问".to_string()));
        }
        Ok(())
    }

    /// 使用调用方 executor 重验转交目标的对象访问条件。
    ///
    /// # 参数
    /// * `item` - 待转交任务
    /// * `access` - 由当前角色与权限形成的目标账号访问快照
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 对象存在、版本匹配且目标账号满足任务类型的访问条件时返回 `Ok(())`。
    ///
    /// # 错误
    /// 对象未注册、不存在、版本变化或目标账号访问条件不足时返回错误。
    ///
    /// # 关键业务约束
    /// 供给分配任务按具体账号和 `purchase_order:create` 授权，不额外引入团队池或固定角色约束。
    async fn ensure_assignment_candidate_access_with_executor(
        &self,
        item: &WorkItem,
        access: &ActorAccess,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let policy = object_policy(item.work_item_type, &item.business_object_type)
            .ok_or_else(|| Error::Forbidden("任务类型未注册责任策略".to_string()))?;
        let keys = HashSet::from([(policy.object_kind, item.business_object_id.clone())]);
        let facts = self.load_object_facts(&keys, executor).await?;
        if !has_assignment_candidate_access(item, access, &facts) {
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
        let account = self
            .db
            .accounts()
            .find_work_item_account(actor.id(), executor)
            .await?
            .ok_or_else(|| Error::Forbidden("操作账号不存在、已停用或身份已变化".to_string()))?;
        AvailableWorkItemAccount::from_account_kind(&account, actor.kind())
            .map_err(|_| Error::Forbidden("操作账号不存在、已停用或身份已变化".to_string()))?;
        let policy = object_policy(item.work_item_type, &item.business_object_type)
            .ok_or_else(|| Error::Forbidden("任务类型未注册责任策略".to_string()))?;
        let read_permission = Permission::parse(policy.read_permission).expect("责任策略权限必须合法");
        let policy_revision = MongoCasbinAdapter::new(self.db.clone())
            .policy_revision(executor)
            .await?;
        let role_ids = active_role_ids(&self.db, actor.kind(), actor.id(), executor).await?;
        let execution_permissions =
            required_execution_permissions(item.work_item_type, &item.business_object_type)
                .ok_or_else(|| Error::Forbidden("任务类型未注册完整执行权限".to_string()))?;
        for permission in execution_permissions.as_slice() {
            let granting_roles = self
                .roles_granting_permission(&role_ids, permission, true)
                .await?;
            if granting_roles.is_empty() {
                return Err(Error::Forbidden(
                    "当前账号已不具备任务所需的完整执行权限".to_string(),
                ));
            }
        }
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
}

/// 转交命令的任务、目标责任人与审计输入。
///
/// # 用途
/// 将转交事务所需字段打包，供 [`WorkItemService::reassign_with_assignment_policy_audit`] 使用。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 事务内必须重验管理人、目标责任人与授权快照。
struct AssignmentPolicyAuditInput<'a> {
    /// 待转交任务。
    item: WorkItem,
    /// 期望任务版本。
    expected_task_version: u64,
    /// 目标责任人。
    target_user_id: String,
    /// 操作人。
    actor: &'a AuditActor,
    /// 强类型幂等命令收据。
    receipt: CommandReceipt,
    /// 权限安全的转交说明。
    audit_detail: String,
    /// 事务外冻结的授权快照。
    authorization: AssignmentAuthorizationSnapshot,
}

/// W29 关闭命令的领域证据与审计输入。
///
/// # 用途
/// 将关闭事务所需字段打包，供 [`WorkItemService::close_with_domain_evidence`] 使用。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 关闭必须在同一事务内写入领域证据与任务终态。
struct CloseDomainEvidenceInput<'a> {
    /// 待关闭任务。
    item: WorkItem,
    /// 操作人。
    actor: &'a AuditActor,
    /// 已规范化的 W29 关闭决策。
    decision: W29CloseDecision,
    /// 强类型幂等命令收据。
    receipt: CommandReceipt,
}

impl WorkItemService {
    /// 在同一事务内重验管理人、目标责任人及全部业务事实后执行转交与审计。
    ///
    /// # 用途
    /// 按授权快照转交任务并写入幂等审计。
    ///
    /// # 参数
    /// * `input` - 任务、目标责任人与审计字段
    ///
    /// # 返回
    /// 返回写入结果或版本冲突。
    ///
    /// # 错误
    /// 授权变化、版本冲突或仓储失败时返回错误。
    ///
    /// # 关键业务约束
    /// 事务内必须重验管理人和目标责任人，并以授权快照版本执行 policy CAS 后才可提交。
    async fn reassign_with_assignment_policy_audit(
        &self,
        input: AssignmentPolicyAuditInput<'_>,
    ) -> Result<WorkItemWriteOutcome> {
        let AssignmentPolicyAuditInput {
            item,
            expected_task_version,
            target_user_id,
            actor,
            receipt,
            audit_detail,
            authorization,
        } = input;
        let replay_receipt = receipt.clone();
        let replay_item_id = item.base.id.clone();
        let purchase_order_id = purchase_order_fulfillment_responsibility_id(&item)?;
        let source_user_id = item.owner_user_id.as_deref().unwrap_or("未指定").to_string();
        let selected_work_item_id = item.base.id.clone();
        let purchase_order_audit = purchase_order_id
            .as_ref()
            .map(|purchase_order_id| {
                actor.clone().resource_log_with_id(
                    format!("{}-purchase-order", receipt.id()),
                    "purchase_order.owner_reassign",
                    "purchase_order",
                    purchase_order_id.clone(),
                    Some(format!(
                        "source_user_id={source_user_id};target_user_id={target_user_id};cascade=open_fulfillment_tasks;selected_work_item_id={selected_work_item_id}"
                    )),
                )
            })
            .transpose()?;
        let audit = actor.clone().resource_log_with_id(
            receipt.id().to_string(),
            receipt.action(),
            receipt.resource_type(),
            item.base.id.clone(),
            Some(receipt.message(Some(&audit_detail))),
        )?;
        let item_id = item.base.id;
        let actor_id = actor.id().to_string();
        let actor_kind = actor.kind();
        let policy_revision = authorization.policy_revision;
        let policy_rbac = self.rbac.clone();
        let validation_rbac = policy_rbac.clone();
        let db = self.db.clone();
        let result = policy_rbac
            .run_authorized_policy_transaction(policy_revision, move |session| {
                Box::pin(async move {
                    let mut current = db
                        .work_items()
                        .find_work_item(&item_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("任务不存在".to_string()))?;
                    if current.base.version != expected_task_version {
                        return Err(Error::ConflictError(REASSIGN_VERSION_CONFLICT.to_string()));
                    }
                    let allow_current_owner =
                        current.owner_user_id.as_deref() == Some(target_user_id.as_str());
                    ensure_assignment_policy_in_transaction(
                        &db,
                        &validation_rbac,
                        AssignmentPolicyCheck {
                            actor_kind,
                            actor_id: &actor_id,
                            assignee_id: &target_user_id,
                            item: &current,
                            require_manager: true,
                            authorization: &authorization,
                            allow_current_owner,
                        },
                        session,
                    )
                    .await?;
                    current = if let Some(purchase_order_id) = purchase_order_id.as_deref() {
                        reassign_purchase_order_fulfillment_responsibility(
                            &db,
                            &validation_rbac,
                            current,
                            purchase_order_id,
                            &target_user_id,
                            &actor_id,
                            &authorization,
                            session,
                        )
                        .await?
                    } else {
                        current.reassign(target_user_id.clone(), Instant::now())?;
                        db.work_items()
                            .update(&mut current, session)
                            .await
                            .map_err(|error| match error {
                                database::Error::OptimisticLockingError => {
                                    Error::ConflictError(REASSIGN_VERSION_CONFLICT.to_string())
                                }
                                error => Error::from(error),
                            })?;
                        current
                    };
                    ensure_assignment_policy_in_transaction(
                        &db,
                        &validation_rbac,
                        AssignmentPolicyCheck {
                            actor_kind,
                            actor_id: &actor_id,
                            assignee_id: &target_user_id,
                            item: &current,
                            require_manager: true,
                            authorization: &authorization,
                            allow_current_owner: true,
                        },
                        session,
                    )
                    .await?;
                    if let Some(purchase_order_audit) = &purchase_order_audit {
                        db.audit_logs().create(purchase_order_audit, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok(current)
                })
            })
            .await;
        match result {
            Ok(item) => Ok(WorkItemWriteOutcome::Updated(Box::new(item))),
            Err(Error::ConflictError(message)) if message == REASSIGN_VERSION_CONFLICT => {
                Ok(WorkItemWriteOutcome::VersionConflict)
            }
            Err(error) => match self.idempotent_replay(&replay_receipt, &replay_item_id).await? {
                Some(item) => Ok(WorkItemWriteOutcome::Updated(Box::new(item))),
                None => Err(error),
            },
        }
    }

    /// 在同一事务内写入 W29 领域证据、关闭任务并登记审计。
    ///
    /// # 用途
    /// 将任务关闭与领域对象证据写入同一事务。
    ///
    /// # 参数
    /// * `input` - 任务、关闭原因与审计字段
    ///
    /// # 返回
    /// 返回写入结果或版本冲突。
    ///
    /// # 错误
    /// 替代任务非法、领域对象不存在或仓储失败时返回错误。
    ///
    /// # 关键业务约束
    /// 仅 W29 可关闭任务允许走此路径；替代任务必须是同类开放正式任务。
    async fn close_with_domain_evidence(
        &self,
        input: CloseDomainEvidenceInput<'_>,
    ) -> Result<WorkItemWriteOutcome> {
        let CloseDomainEvidenceInput {
            mut item,
            actor,
            decision,
            receipt,
        } = input;
        let closed_at = Instant::now();
        item.close(
            actor.id(),
            WorkItemCloseData {
                close_reason: decision.close_reason().to_string(),
            },
            closed_at,
        )?;
        let evidence_reference = decision.evidence_reference(&item.base.id, receipt.id())?;
        let replay_receipt = receipt.clone();
        let replay_item_id = item.base.id.clone();
        let audit = receipt.audit(actor.clone(), item.base.id.clone())?;
        let actor_id = actor.id().to_string();
        let receipt_id = receipt.id().to_string();
        let db = self.db.clone();
        let client = db.client().clone();
        let result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    close_w29_domain_object(
                        &db,
                        CloseW29DomainObjectInput {
                            item: &item,
                            decision: &decision,
                            evidence_reference: &evidence_reference,
                            actor_id: &actor_id,
                            receipt_id: &receipt_id,
                            closed_at,
                        },
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
            Err(WorkItemWriteError::Service(error)) => {
                match self.idempotent_replay(&replay_receipt, &replay_item_id).await? {
                    Some(item) => Ok(WorkItemWriteOutcome::Updated(Box::new(item))),
                    None => Err(error),
                }
            }
        }
    }

    /// 读取已完成的同一幂等命令，并拒绝相同键混用不同请求。
    async fn idempotent_replay(&self, receipt: &CommandReceipt, item_id: &str) -> Result<Option<WorkItem>> {
        let Some(resource_id) = receipt.committed_resource_id(&self.db).await? else {
            return Ok(None);
        };
        if resource_id != item_id {
            return Err(Error::Internal("幂等审计资源与命令不一致".to_string()));
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
            .find_work_item(item_id, &mut NoTransaction)
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
        let mut view = WorkItemView::from_fields(fields, single_item_context_id(actor.id(), &item_id))?
            .with_access(
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
}

/// 事务内分派策略重验输入。
///
/// # 用途
/// 将操作人、候选人与授权快照打包，供 [`ensure_assignment_policy_in_transaction`] 使用。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 操作人身份必须与授权快照一致。
struct AssignmentPolicyCheck<'a> {
    /// 操作人账号类型。
    actor_kind: entities::AccountKind,
    /// 操作人 ID。
    actor_id: &'a str,
    /// 目标责任人 ID。
    assignee_id: &'a str,
    /// 当前任务。
    item: &'a WorkItem,
    /// 是否要求管理人权限。
    require_manager: bool,
    /// 事务外冻结的授权快照。
    authorization: &'a AssignmentAuthorizationSnapshot,
    /// 是否允许候选人为当前责任人。
    allow_current_owner: bool,
}

/// 在任务责任事务内重放全部固定分派策略。
///
/// # 用途
/// 重验操作人授权与候选人资格后再允许写入。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `check` - 分派策略重验输入
/// * `executor` - 事务执行器
///
/// # 返回
/// 策略仍成立时返回 `Ok(())`。
///
/// # 错误
/// 身份变化、授权不足或候选人非法时返回错误。
///
/// # 关键业务约束
/// 必须在同一任务责任事务内调用。
async fn ensure_assignment_policy_in_transaction(
    db: &Database,
    rbac: &SharedRbacService,
    check: AssignmentPolicyCheck<'_>,
    executor: &mut dyn Executor,
) -> Result<()> {
    if check.actor_kind != check.authorization.actor_kind {
        return Err(Error::Forbidden("操作账号身份已变化".to_string()));
    }
    let service = WorkItemService::new(db.clone(), rbac.clone());
    service
        .ensure_assignment_actor_access(
            check.actor_kind,
            check.actor_id,
            check.item,
            check.require_manager,
            check.authorization,
            executor,
        )
        .await?;
    service
        .ensure_assignment_candidate(
            check.assignee_id,
            check.authorization.assignee_kind,
            check.item,
            check.authorization,
            check.allow_current_owner,
            executor,
        )
        .await
}

/// 解析采购单履约任务冻结的采购责任键。
///
/// # 参数
/// * `item` - 待转交工作项
///
/// # 返回
/// 非采购履约任务返回空；采购履约任务返回采购单 ID。
///
/// # 错误
/// 对象类型、责任角色、原因码或责任键不符合固定履约合同时返回错误。
fn purchase_order_fulfillment_responsibility_id(item: &WorkItem) -> Result<Option<String>> {
    let key = item.fulfillment_responsibility_key().map_err(|_| {
        Error::BusinessLogicError(
            "履约任务的对象、责任角色、原因码或责任键不一致，请联系管理员修复后重试".to_string(),
        )
    })?;
    Ok(match key {
        Some(FulfillmentResponsibilityKey::PurchaseOrder(id)) => Some(id),
        Some(
            FulfillmentResponsibilityKey::WarehouseReceipt(_)
            | FulfillmentResponsibilityKey::WarehouseShip(_),
        )
        | None => None,
    })
}

/// 原子变更采购单当前责任人与其全部开放采购履约任务。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 授权服务
/// * `selected` - 管理员本次选中的开放任务
/// * `purchase_order_id` - 责任键解析出的采购单 ID
/// * `target_user_id` - 新采购责任人
/// * `actor_id` - 管理员账号 ID
/// * `authorization` - 事务外冻结的授权快照
/// * `executor` - 当前事务执行器
///
/// # 返回
/// 返回已完成转交的选中任务。
///
/// # 错误
/// 采购单、任务集合或原责任不一致，目标缺少任一履约权限，或 CAS 写入失败时返回错误。
///
/// # 关键业务约束
/// 完成和关闭的历史任务保持不变；只有同一 `purchase_order:{id}` 下的开放履约任务级联。
#[allow(clippy::too_many_arguments)]
async fn reassign_purchase_order_fulfillment_responsibility(
    db: &Database,
    rbac: &SharedRbacService,
    selected: WorkItem,
    purchase_order_id: &str,
    target_user_id: &str,
    actor_id: &str,
    authorization: &AssignmentAuthorizationSnapshot,
    executor: &mut dyn Executor,
) -> Result<WorkItem> {
    let (mut order, mut tasks) =
        load_purchase_order_fulfillment_scope(db, &selected, purchase_order_id, executor).await?;
    ensure_fulfillment_tasks_candidate(
        &WorkItemService::new(db.clone(), rbac.clone()),
        &tasks,
        target_user_id,
        &authorization.assignee_permissions,
        executor,
    )
    .await
    .map_err(|_| {
        Error::Forbidden("目标账号缺少一个或多个开放履约任务所需权限，采购单责任未变更".to_string())
    })?;

    order.reassign_owner(target_user_id.to_string(), actor_id.to_string())?;
    db.purchase_orders()
        .update(&mut order, executor)
        .await
        .map_err(|error| match error {
            database::Error::OptimisticLockingError => {
                Error::ConflictError(REASSIGN_VERSION_CONFLICT.to_string())
            }
            error => Error::from(error),
        })?;

    let reassigned_at = Instant::now();
    let mut selected_after = None;
    for task in &mut tasks {
        task.reassign(target_user_id.to_string(), reassigned_at)?;
        db.work_items()
            .update(task, executor)
            .await
            .map_err(|error| match error {
                database::Error::OptimisticLockingError => {
                    Error::ConflictError(REASSIGN_VERSION_CONFLICT.to_string())
                }
                error => Error::from(error),
            })?;
        if task.base.id == selected.base.id {
            selected_after = Some(task.clone());
        }
    }
    selected_after.ok_or_else(|| Error::ConflictError("采购单开放履约任务已变化，请刷新后重试".to_string()))
}

/// 装载并校验采购单当前责任人与全部开放履约任务的一致范围。
async fn load_purchase_order_fulfillment_scope(
    db: &Database,
    selected: &WorkItem,
    purchase_order_id: &str,
    executor: &mut dyn Executor,
) -> Result<(entities::purchase_order::PurchaseOrder, Vec<WorkItem>)> {
    let responsibility_key = format!("purchase_order:{purchase_order_id}");
    let order = db
        .purchase_orders()
        .find_by_id(
            &entities::ids::PurchaseOrderId::new(purchase_order_id.to_string()),
            executor,
        )
        .await?
        .ok_or_else(|| Error::BusinessLogicError("履约任务引用的采购单不存在".to_string()))?;
    if matches!(
        order.stable.status,
        entities::purchase_order::PurchaseOrderStatus::Completed
            | entities::purchase_order::PurchaseOrderStatus::Voided
    ) {
        return Err(Error::BusinessLogicError(
            "已完成或已作废采购单不能变更责任人".to_string(),
        ));
    }
    let original_owner = order.current_owner_user_id()?.to_string();
    if selected.owner_user_id.as_deref() != Some(original_owner.as_str()) {
        return Err(Error::ConflictError(
            "采购单责任人与当前履约任务责任不一致，请刷新责任事实后重试".to_string(),
        ));
    }
    let tasks = db
        .work_items()
        .list_open_fulfillment_by_responsibility_key(&responsibility_key, executor)
        .await?;
    if tasks.is_empty() || !tasks.iter().any(|task| task.base.id == selected.base.id) {
        return Err(Error::ConflictError(
            "采购单开放履约任务已变化，请刷新后重试".to_string(),
        ));
    }
    if tasks.iter().any(|task| {
        task.owner_user_id.as_deref() != Some(original_owner.as_str())
            || task.responsibility_key() != Some(responsibility_key.as_str())
            || !matches!(
                purchase_order_fulfillment_responsibility_id(task),
                Ok(Some(task_purchase_order_id)) if task_purchase_order_id == purchase_order_id
            )
    }) {
        return Err(Error::ConflictError(
            "采购单开放履约任务责任身份不一致，请联系管理员处理后重试".to_string(),
        ));
    }
    Ok((order, tasks))
}

/// 校验目标账号可执行给定全部开放履约任务。
async fn ensure_fulfillment_tasks_candidate(
    service: &WorkItemService,
    tasks: &[WorkItem],
    target_user_id: &str,
    permissions: &[Permission],
    executor: &mut dyn Executor,
) -> Result<()> {
    let target_access = ActorAccess {
        actor_id: target_user_id.to_string(),
        permissions: permissions.to_vec(),
        participant_document_ids: HashSet::new(),
        organization_ids: Vec::new(),
        responsibility_scopes: Vec::new(),
        can_manage: false,
    };
    for task in tasks {
        service
            .ensure_assignment_candidate_access_with_executor(task, &target_access, executor)
            .await?;
    }
    Ok(())
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

#[cfg(test)]
fn approval_assignment_separated(
    candidate_id: &str,
    started_by: &str,
    submitted_by: &str,
    responsibility_actor_ids: &[String],
    current_owner_user_id: Option<&str>,
    allow_current_owner: bool,
    decided_by: &[&str],
) -> bool {
    if candidate_id == started_by || candidate_id == submitted_by {
        return false;
    }
    if responsibility_actor_ids.iter().any(|actor_id| {
        actor_id == candidate_id && !(allow_current_owner && current_owner_user_id == Some(candidate_id))
    }) {
        return false;
    }
    !decided_by.contains(&candidate_id)
}

/// W29 领域对象关闭输入。
///
/// # 用途
/// 将关闭证据字段打包，供 [`close_w29_domain_object`] 在事务内写入。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 替代任务不得引用自身，且必须仍是同类开放正式任务。
struct CloseW29DomainObjectInput<'a> {
    /// 被关闭的任务。
    item: &'a WorkItem,
    /// 已规范化的关闭决策。
    decision: &'a W29CloseDecision,
    /// 与关闭决策一致的领域证据引用。
    evidence_reference: &'a W29EvidenceReference,
    /// 操作人 ID。
    actor_id: &'a str,
    /// 命令收据主键。
    receipt_id: &'a str,
    /// 关闭时间。
    closed_at: Instant,
}

/// 事务内关闭 W29 领域对象并登记证据引用。
///
/// # 用途
/// 校验替代任务后把关闭证据写入对应领域对象。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 任务、原因与证据字段
/// * `executor` - 事务执行器
///
/// # 返回
/// 写入成功时返回 `Ok(())`。
///
/// # 错误
/// 替代任务非法、领域对象不存在或类型不一致时返回错误。
///
/// # 关键业务约束
/// 必须与任务关闭写入同一事务。
async fn close_w29_domain_object(
    db: &Database,
    input: CloseW29DomainObjectInput<'_>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let CloseW29DomainObjectInput {
        item,
        decision,
        evidence_reference,
        actor_id,
        receipt_id,
        closed_at,
    } = input;
    if let Some(replacement_work_item_id) = decision.replacement_work_item_id() {
        let replacement = db
            .work_items()
            .find_work_item(replacement_work_item_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("替代任务不存在".to_string()))?;
        if !replacement.is_w29_replacement_for(item) {
            return Err(Error::ConflictError(
                "替代任务必须在关闭事务中仍是同一 W29 对象类别的开放正式任务".to_string(),
            ));
        }
    }
    match item.business_object_type.as_str() {
        "integration_error_task" => {
            let mut task = db
                .integration_error_tasks()
                .find_work_item_integration_error_task(&item.business_object_id, executor)
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
                Some(evidence_reference.to_string()),
                closed_at,
            )?;
            db.integration_error_tasks().update(&mut task, executor).await?;
            Ok(())
        }
        "reconciliation_difference" => {
            let difference_id = ReconciliationDifferenceId::new(item.business_object_id.clone());
            db.reconciliation_differences()
                .find_work_item_reconciliation_difference(&item.business_object_id, executor)
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
            let resolution_no = W29CloseDecision::next_resolution_no(
                latest.as_ref().map(|resolution| resolution.resolution_no),
            )?;
            let resolution_id_digest = CommandFingerprint::from_parts([receipt_id.to_string()]);
            let resolution = ReconciliationDifferenceResolution::new_close_evidence(
                ReconciliationDifferenceResolutionId::new(format!(
                    "w29-close-{}",
                    resolution_id_digest.digest_hex()
                )),
                difference_id,
                resolution_no,
                decision.resolution_action(),
                evidence_reference.clone(),
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

/// 工作项简报事实装载使用的实体对象种类别名。
type ObjectKind = WorkItemBriefObjectKind;
/// 工作项责任形成使用的实体岗位分离策略别名。
type AssignmentSeparationPolicy = WorkItemAssignmentSeparationPolicy;

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
    /// 生产者合同允许的权威版本；无约束值对象表示该领域没有通用锁版本约束。
    subject_versions: WorkItemSubjectVersions,
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
            subject_versions: WorkItemSubjectVersions::unrestricted(),
            counterparty_label: None,
            impact_summary: None,
            brief_source: None,
            subject_briefs: HashMap::new(),
        }
    }
}

/// 组装集成错误任务的结构化简报。
///
/// # 参数
/// * `task` - 集成错误任务正式事实
///
/// # 返回
/// 返回错误分类、关联参考号、发生时间、重试证据、脱敏摘要和处理结果。
///
/// # 错误
/// 无。
fn integration_error_brief_source(task: &IntegrationErrorTask) -> brief::ObjectBriefSource {
    let occurred_at = base_created_at_datetime(task.base.created_at);
    let last_attempt_at = task.last_attempt_at.map(brief::format_instant_datetime);
    let attempt_count = format!("{} 次", task.attempt_count);
    let resolved_at = task.resolved_at.map(brief::format_instant_datetime);
    let resolution_type = task.resolution_type.map(|value| value.label().to_string());
    let reference = task
        .business_object_id
        .clone()
        .or_else(|| task.message_id.as_ref().map(ToString::to_string));
    let mut sections = Vec::new();
    brief::push_section(&mut sections, "错误分类", Some(task.error_class.label()), false);
    brief::push_section(&mut sections, "状态", Some(task.status.label()), false);
    brief::push_section(
        &mut sections,
        "业务对象参考号",
        task.business_object_id.as_deref(),
        false,
    );
    let message_id = task.message_id.as_ref().map(ToString::to_string);
    brief::push_section(&mut sections, "关联消息", message_id.as_deref(), false);
    brief::push_section(&mut sections, "发生时间", occurred_at.as_deref(), false);
    brief::push_section(&mut sections, "重试记录", Some(attempt_count.as_str()), false);
    brief::push_section(&mut sections, "最近尝试", last_attempt_at.as_deref(), false);
    brief::push_section(
        &mut sections,
        "错误摘要",
        task.last_attempt_summary.as_deref(),
        false,
    );
    brief::push_section(&mut sections, "责任角色", task.owner_role.as_deref(), false);
    brief::push_section(&mut sections, "责任人", task.owner_user_id.as_deref(), false);
    brief::push_section(
        &mut sections,
        "安全下一步",
        Some(integration_error_next_step(task.error_class)),
        false,
    );
    brief::push_section(&mut sections, "解决方式", resolution_type.as_deref(), false);
    brief::push_section(&mut sections, "处理证据", task.resolution.as_deref(), false);
    brief::push_section(&mut sections, "完成时间", resolved_at.as_deref(), false);
    brief::ObjectBriefSource {
        customer: None,
        amount_label: None,
        lines: Vec::new(),
        more_count: 0,
        submitter_name: None,
        list_summary: brief::join_list_summary([
            Some(task.error_class.label().to_string()),
            reference,
            Some(format!("重试 {attempt_count}")),
            task.last_attempt_summary.as_deref().and_then(brief::non_empty),
        ]),
        extra_sections: sections,
    }
}

/// 返回集成错误对业务处理的安全影响说明。
///
/// # 参数
/// * `task` - 集成错误任务正式事实
///
/// # 返回
/// 结果未知返回防重复写入说明，其余分类返回通用缺失或重复风险说明。
///
/// # 错误
/// 无。
fn integration_error_impact(task: &IntegrationErrorTask) -> &'static str {
    if task.error_class == ErrorClass::ResultUnknown {
        "外部结果尚未确认，盲目重试可能造成重复写入或重复履约"
    } else {
        "集成异常未处理可能造成业务事实缺失、延迟或上下游不一致"
    }
}

/// 按固定错误分类返回可执行且安全的下一步。
///
/// # 参数
/// * `error_class` - 错误分类
///
/// # 返回
/// 返回不泄露内部实现的处理指引。
///
/// # 错误
/// 无。
fn integration_error_next_step(error_class: ErrorClass) -> &'static str {
    match error_class {
        ErrorClass::CapabilityGap => "确认目标系统能力后转人工补偿或补齐能力",
        ErrorClass::MappingError => "修复映射并验证业务键后再重放",
        ErrorClass::BusinessRejected => "核对拒绝原因并修正业务输入后重新提交",
        ErrorClass::TransientFailure | ErrorClass::RateLimited => "核对最近尝试摘要，按原幂等业务键重试",
        ErrorClass::ResultUnknown => "先查询原请求结果，确认无结果后才允许重放",
        ErrorClass::AuthSignature => "修复鉴权或签名配置，验证通过后再重试",
        ErrorClass::OutOfOrder => "补齐前置事实并确认顺序后再重放",
    }
}

/// 组装对账差异的结构化业务异常简报。
///
/// # 参数
/// * `difference` - 不可变对账差异事实
///
/// # 返回
/// 返回异常对象、差异类型、发现时间与两侧证据引用。
///
/// # 错误
/// 无。
fn reconciliation_difference_brief_source(difference: &ReconciliationDifference) -> brief::ObjectBriefSource {
    let occurred_at = base_created_at_datetime(difference.base.created_at);
    let evidence_count = usize::from(difference.left_fact_reference.is_some())
        + usize::from(difference.right_fact_reference.is_some());
    let evidence_summary = format!("{evidence_count} 侧证据");
    let mut sections = Vec::new();
    brief::push_section(
        &mut sections,
        "异常对象",
        Some(difference.business_object_type.as_str()),
        false,
    );
    brief::push_section(
        &mut sections,
        "外部/业务参考号",
        Some(difference.business_object_id.as_str()),
        false,
    );
    brief::push_section(
        &mut sections,
        "差异类型",
        Some(difference.difference_type.as_str()),
        false,
    );
    brief::push_section(&mut sections, "发现时间", occurred_at.as_deref(), false);
    brief::push_section(
        &mut sections,
        "左侧证据",
        difference.left_fact_reference.as_deref(),
        false,
    );
    brief::push_section(
        &mut sections,
        "右侧证据",
        difference.right_fact_reference.as_deref(),
        false,
    );
    brief::push_section(
        &mut sections,
        "关闭条件",
        Some("两侧事实已核对，并引用正式处理结果或无需处理的证据"),
        false,
    );
    brief::ObjectBriefSource {
        customer: None,
        amount_label: None,
        lines: Vec::new(),
        more_count: 0,
        submitter_name: None,
        list_summary: brief::join_list_summary([
            Some(difference.business_object_type.clone()),
            Some(difference.business_object_id.clone()),
            Some(difference.difference_type.clone()),
            Some(evidence_summary),
        ]),
        extra_sections: sections,
    }
}

/// 把实体基础时间转换为业务时区展示；非法或测试零值不上屏。
///
/// # 参数
/// * `created_at` - 实体 Unix 秒级创建时间
///
/// # 返回
/// 返回分钟级时间；零值或超出 `i64` 时返回 `None`。
///
/// # 错误
/// 无。
fn base_created_at_datetime(created_at: u64) -> Option<String> {
    (created_at > 0)
        .then(|| i64::try_from(created_at).ok())
        .flatten()
        .map(Instant::from_unix_secs)
        .map(brief::format_instant_datetime)
}

#[cfg(test)]
mod integration_brief_tests {
    use entities::{
        common::time::Instant,
        ids::{IntegrationErrorTaskId, ReconciliationDifferenceId},
        integration_ops::{
            ErrorClass, IntegrationErrorTask, IntegrationErrorTaskData, ReconciliationDifference,
            ReconciliationDifferenceData,
        },
    };

    use super::{integration_error_brief_source, reconciliation_difference_brief_source};

    #[test]
    fn integration_brief_exposes_retry_and_redacted_error_evidence() {
        let mut task = IntegrationErrorTask::new(
            IntegrationErrorTaskId::new("integration-1"),
            IntegrationErrorTaskData {
                message_id: None,
                business_object_id: Some("EXT-2026-001".to_string()),
                error_class: ErrorClass::ResultUnknown,
                owner_role: Some("integration-operator".to_string()),
                owner_user_id: Some("operator-1".to_string()),
            },
        )
        .unwrap();
        task.attempt_count = 2;
        task.last_attempt_at = Some(Instant::from_unix_secs(1_787_457_600));
        task.last_attempt_summary = Some("目标系统超时，未取得业务结果".to_string());

        let brief = integration_error_brief_source(&task);

        assert!(brief
            .extra_sections
            .iter()
            .any(|section| { section.label == "业务对象参考号" && section.value == "EXT-2026-001" }));
        assert!(brief
            .extra_sections
            .iter()
            .any(|section| section.label == "重试记录" && section.value == "2 次"));
        assert!(brief.list_summary.contains("目标系统超时"));
    }

    #[test]
    fn reconciliation_brief_exposes_both_immutable_evidence_references() {
        let difference = ReconciliationDifference::new(
            ReconciliationDifferenceId::new("difference-1"),
            ReconciliationDifferenceData {
                business_object_type: "商城订单".to_string(),
                business_object_id: "MALL-1001".to_string(),
                difference_type: "金额不一致".to_string(),
                left_fact_reference: Some("mall-snapshot:7".to_string()),
                right_fact_reference: Some("erp-revision:9".to_string()),
            },
        )
        .unwrap();

        let brief = reconciliation_difference_brief_source(&difference);

        assert!(brief
            .extra_sections
            .iter()
            .any(|section| section.label == "左侧证据"));
        assert!(brief
            .extra_sections
            .iter()
            .any(|section| section.label == "右侧证据"));
        assert!(brief.list_summary.contains("2 侧证据"));
    }
}

type ObjectFactMap = HashMap<(ObjectKind, String), ObjectFact>;

const SYSTEM_OBJECT_OWNER: &str = "__system__";

/// 解析实体注册的工作项简报关系。
///
/// # 参数
/// * `work_item_type` - 工作项类型
/// * `business_object_type` - 工作项持久化的业务对象类型
///
/// # 返回
/// 已注册组合返回权威对象种类与读取权限；未注册组合返回 `None`。
///
/// # 错误
/// 无。
fn object_policy(
    work_item_type: WorkItemType,
    business_object_type: &str,
) -> Option<&'static WorkItemBriefRelation> {
    work_item_type.brief_relation(business_object_type)
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

/// 把审批节点执行与实例有界投影合并为工作台判断上下文。
///
/// # 参数
/// * `execution` - 工作项直接引用的审批节点执行
/// * `summary` - 同一审批实例的有界列表投影
///
/// # 返回
/// 返回当前任务节点、审批人、轮次、实例状态和最近驳回摘要。
///
/// # 错误
/// 无；两者属于同一实例的约束由调用方按实例 ID 建立。
fn approval_context_view(
    execution: &bpm::model::ApprovalNodeExecution,
    summary: &database::repository::bpm::ApprovalInstanceSummary,
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
fn remove_approval_decision_actions(actions: &mut Vec<WorkItemAllowedAction>) -> bool {
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

/// 从批量对象键中提取指定实体种类的稳定 ID。
///
/// # 参数
/// * `keys` - 工作项关系解析形成的对象键集合
/// * `kind` - 待装载的权威业务对象种类
///
/// # 返回
/// 返回该种类的对象 ID 集合。
///
/// # 错误
/// 无。
fn object_ids(keys: &HashSet<(ObjectKind, String)>, kind: ObjectKind) -> Vec<String> {
    keys.iter()
        .filter(|(candidate, _)| *candidate == kind)
        .map(|(_, id)| id.clone())
        .collect()
}

/// 根据实体注册关系和当前权限形成仓储候选对象形状。
///
/// # 参数
/// * `access` - 当前账号的权限与参与范围事实
///
/// # 返回
/// 返回当前账号具备读取权限的工作项类型和业务对象类型组合。
///
/// # 错误
/// 无；注册关系中的固定权限必须可解析。
fn object_access_shapes(access: &ActorAccess) -> Vec<(WorkItemType, String)> {
    WorkItemType::registered_brief_relations()
        .iter()
        .filter(|policy| has_permission(access, policy.read_permission))
        .map(|policy| (policy.work_item_type, policy.business_object_type.to_string()))
        .collect()
}

/// 判断当前访问快照是否覆盖实体关系要求的读取权限。
///
/// # 参数
/// * `access` - 当前账号权限快照
/// * `permission` - 实体关系注册的固定权限代码
///
/// # 返回
/// 任一已授予权限覆盖要求时返回 `true`。
///
/// # 错误
/// 无；固定权限代码无效属于程序错误并触发断言。
fn has_permission(access: &ActorAccess, permission: &str) -> bool {
    let required = Permission::parse(permission).expect("对象注册表权限必须合法");
    PermissionSet::new(access.permissions.clone()).covers_one(&required)
}

/// 按对象权限、参与关系与权威版本过滤工作项列表投影。
///
/// # 参数
/// * `rows` - 仓储返回的候选工作项行
/// * `access` - 当前账号访问事实
/// * `facts` - 已批量加载的业务对象事实
///
/// # 返回
/// 返回当前账号可见且已补齐对象展示字段的工作项。
///
/// # 错误
/// 无；未注册或无法证明访问权的候选项会失败关闭并被过滤。
fn authorized_fields(
    rows: Vec<database::WorkItemRow>,
    access: &ActorAccess,
    facts: &ObjectFactMap,
) -> Vec<dto::WorkItemFields> {
    rows.into_iter()
        .filter_map(|row| {
            let policy = object_policy(row.work_item_type, &row.business_object_type)?;
            let fact = facts.get(&(policy.object_kind, row.business_object_id.clone()))?;
            if !has_permission(access, policy.read_permission)
                || !has_item_participation(
                    row.work_item_type,
                    row.owner_user_id.as_deref(),
                    &row.owner_role,
                    &row.owner_organization_id,
                    access,
                    fact,
                )
                || !fact.subject_versions.accepts(&row.subject_version)
            {
                return None;
            }
            let mut fields = dto::WorkItemFields::from(row);
            apply_object_display(&mut fields, fact);
            Some(fields)
        })
        .collect()
}

/// 按对象权限、参与关系与权威版本形成单个工作项投影。
///
/// # 参数
/// * `item` - 待授权工作项实体
/// * `access` - 当前账号访问事实
/// * `facts` - 已加载的业务对象事实
///
/// # 返回
/// 授权通过时返回补齐对象展示字段的投影，否则返回 `None`。
///
/// # 错误
/// 无；未注册或无法证明访问权时失败关闭。
fn authorized_item_fields(
    item: WorkItem,
    access: &ActorAccess,
    facts: &ObjectFactMap,
) -> Option<dto::WorkItemFields> {
    let policy = object_policy(item.work_item_type, &item.business_object_type)?;
    let fact = facts.get(&(policy.object_kind, item.business_object_id.clone()))?;
    if !has_permission(access, policy.read_permission)
        || !has_item_participation(
            item.work_item_type,
            item.owner_user_id.as_deref(),
            &item.owner_role,
            &item.owner_organization_id,
            access,
            fact,
        )
        || !fact.subject_versions.accepts(&item.subject_version)
    {
        return None;
    }
    let mut fields = dto::WorkItemFields::from(item);
    apply_object_display(&mut fields, fact);
    Some(fields)
}

/// 判断转交目标是否满足任务类型要求的权限、参与关系和对象版本。
///
/// # 参数
/// * `item` - 待转交任务
/// * `access` - 目标账号访问事实
/// * `facts` - 已加载的业务对象事实
///
/// # 返回
/// 目标账号可接收任务时返回 `true`。
///
/// # 错误
/// 无；对象未注册或事实缺失时返回 `false`。
fn has_assignment_candidate_access(item: &WorkItem, access: &ActorAccess, facts: &ObjectFactMap) -> bool {
    let Some(policy) = object_policy(item.work_item_type, &item.business_object_type) else {
        return false;
    };
    let Some(fact) = facts.get(&(policy.object_kind, item.business_object_id.clone())) else {
        return false;
    };
    has_permission(access, policy.read_permission)
        && has_execution_permissions(item.work_item_type, &item.business_object_type, access)
        && (item.work_item_type.uses_explicit_owner_authorization()
            || has_object_participation(access, &item.owner_role, &item.owner_organization_id, fact))
        && fact.subject_versions.accepts(&item.subject_version)
}

/// 返回执行任务的完整权限；普通任务返回空集，未注册执行对象失败关闭。
fn required_execution_permissions(
    work_item_type: WorkItemType,
    business_object_type: &str,
) -> Option<PermissionSet> {
    if work_item_type == WorkItemType::BusinessException && business_object_type == "SUPPLIER_OFFERING" {
        return Some(PermissionSet::new([Permission::parse(
            "supplier_offering:resolve_supply_exception",
        )
        .expect("业务异常固定权限必须合法")]));
    }
    work_item_type.required_execution_permissions(business_object_type)
}

/// 判断账号是否覆盖执行任务在目标工作面所需的全部权限。
fn has_execution_permissions(
    work_item_type: WorkItemType,
    business_object_type: &str,
    access: &ActorAccess,
) -> bool {
    required_execution_permissions(work_item_type, business_object_type)
        .is_some_and(|required| PermissionSet::new(access.permissions.clone()).covers(&required))
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
    let preserve_task_impact = fields.work_item_type.uses_explicit_owner_authorization()
        && fields
            .impact_summary
            .as_deref()
            .is_some_and(|impact| !impact.trim().is_empty());
    if !preserve_task_impact {
        if let Some(impact) = subject
            .and_then(|item| item.impact_summary.clone())
            .or_else(|| fact.impact_summary.clone())
        {
            fields.impact_summary = Some(impact);
        }
    }
    fields.brief_source = subject
        .and_then(|item| item.brief_source.clone())
        .or_else(|| fact.brief_source.clone());
}

/// 判断账号是否满足工作项的参与条件。
///
/// # 参数
/// * `work_item_type` - 工作项类型
/// * `owner_user_id` - 当前具体负责人
/// * `owner_role` - 责任角色标识
/// * `owner_organization_id` - 责任组织 ID
/// * `access` - 当前账号访问事实
/// * `fact` - 业务对象事实
///
/// # 返回
/// 具备对象参与关系，或是供给分配任务的具体负责人时返回 `true`。
///
/// # 错误
/// 无；调用方必须另行验证对象权限。
fn has_item_participation(
    work_item_type: WorkItemType,
    owner_user_id: Option<&str>,
    owner_role: &str,
    owner_organization_id: &str,
    access: &ActorAccess,
    fact: &ObjectFact,
) -> bool {
    let is_explicit_owner =
        work_item_type.uses_explicit_owner_authorization() && owner_user_id == Some(access.actor_id.as_str());
    is_explicit_owner || has_object_participation(access, owner_role, owner_organization_id, fact)
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

/// 判断任务是否计入「当前真的能推进」的指标。
///
/// 单据审批任务由审批运行时直接指派，只会拿到 `Approve`/`Reject`——`allowed_actions`
/// 的 `Process` 分支要求 `owner_role` 能过责任范围校验，而审批任务的 `owner_role`
/// 是语义标签（`sales_order_approver`）不是角色 ID，永远过不了。只认 `Process` 会让
/// 待办列表有条目、「待我处理」却是 0。
fn counts_as_processable_stat(scope: WorkItemScope, access: &ViewAccess) -> bool {
    if access.processing_state != ProcessingState::Ready {
        return false;
    }
    match scope {
        WorkItemScope::Mine => access.allowed_actions.iter().any(|action| {
            matches!(
                action,
                WorkItemAllowedAction::Process | WorkItemAllowedAction::Approve
            )
        }),
        WorkItemScope::Managed | WorkItemScope::History => false,
    }
}

/// 计算当前账号在指定队列范围内可执行的工作项动作。
///
/// # 参数
/// * `item` - 已完成对象授权的工作项投影
/// * `scope` - 当前队列范围
/// * `actor_id` - 当前账号 ID
/// * `access` - 当前账号访问事实
///
/// # 返回
/// 返回查看、处理、审批或管理动作集合。
///
/// # 错误
/// 无；未满足责任或管理条件的动作不会出现在结果中。
fn allowed_actions(
    item: &dto::WorkItemFields,
    scope: WorkItemScope,
    actor_id: &str,
    access: &ActorAccess,
) -> Vec<WorkItemAllowedAction> {
    let mut actions = vec![WorkItemAllowedAction::View];
    let is_explicit_owner = item.work_item_type.uses_explicit_owner_authorization()
        && item.owner_user_id.as_deref() == Some(actor_id);
    if item.owner_user_id.as_deref() == Some(actor_id)
        && has_execution_permissions(item.work_item_type, &item.business_object_type, access)
        && (is_explicit_owner
            || covers_responsibility(access, &item.owner_role, &item.owner_organization_id)
            || item.status != WorkItemStatus::Open)
    {
        actions.push(WorkItemAllowedAction::Process);
    }
    // 开放的单据审批任务由审批运行时直接指派给责任人（owner_user_id = 指派
    // 人，owner_role 为语义标签而非角色 ID，无法用责任范围校验）；最终授权由
    // /admin/approval-decisions 写时重验（账号启用 + approval_instance:decide +
    // 单据读权）。
    if item.work_item_type.is_document_approval()
        && item.status == WorkItemStatus::Open
        && item.approval_node_execution_id.is_some()
        && item.owner_user_id.as_deref() == Some(actor_id)
    {
        actions.push(WorkItemAllowedAction::Approve);
        actions.push(WorkItemAllowedAction::Reject);
    }
    let is_approval_responsibility =
        item.work_item_type.is_document_approval() || item.approval_node_execution_id.is_some();
    if access.can_manage && scope == WorkItemScope::Managed && !is_approval_responsibility {
        if has_permission(access, REASSIGN_PERMISSION) {
            actions.push(WorkItemAllowedAction::Reassign);
        }
        if is_w29_fields_closable(item) && has_permission(access, CLOSE_PERMISSION) {
            actions.push(WorkItemAllowedAction::Close);
        }
    }
    actions
}

/// 将实体对审批任务的通用责任变更禁令映射为稳定审批错误码。
fn ensure_generic_work_item_mutation(item: &WorkItem) -> Result<()> {
    item.ensure_generic_responsibility_mutation()
        .map_err(|_| Error::from_approval_code(ErrorCode::ApprovalGenericWorkItemMutationForbidden))
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

fn responsibility_scope_for_role(
    role_id: &str,
    role_scopes: &[DataScope],
    user_scopes: &[DataScope],
) -> Vec<(String, Option<String>)> {
    let Some(role_coverage) = OrganizationCoverage::from_scopes(role_scopes) else {
        return Vec::new();
    };
    // 默认政策仍由 Service 拥有：用户未配置显式范围时解释为 All；角色未配置
    // 时上方已失败关闭为 None。
    let user_coverage = OrganizationCoverage::from_scopes(user_scopes).unwrap_or(OrganizationCoverage::All);
    role_coverage
        .intersect(&user_coverage)
        .map(|coverage| {
            ResponsibilityScopeSet::for_role(role_id, &coverage)
                .as_slice()
                .to_vec()
        })
        .unwrap_or_default()
}

fn covers_responsibility(access: &ActorAccess, role: &str, organization_id: &str) -> bool {
    ResponsibilityScopeSet::new(access.responsibility_scopes.clone()).covers(role, organization_id)
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
    OrganizationCoverage::from_targets(access.organization_ids.clone())
        .is_some_and(|coverage| coverage.covers(organization_id))
}

fn apply_due_filter(filter: &mut WorkItemFilter, due: Option<WorkItemDueFilter>) -> Result<()> {
    let Some(due) = due else {
        return Ok(());
    };
    let window = due
        .window_at(Instant::now())
        .map_err(|error| Error::Internal(error.to_string()))?;
    filter.due_from = window.from;
    filter.due_before = Some(window.before);
    Ok(())
}

fn business_day_bounds() -> Result<(Instant, Instant)> {
    business_day_bounds_at(Instant::now().unix_secs())
}

fn business_day_bounds_at(now_unix_secs: i64) -> Result<(Instant, Instant)> {
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
fn family_counts_for_types(
    work_item_types: impl IntoIterator<Item = WorkItemType>,
) -> WorkItemFamilyCountsView {
    let mut counts = WorkItemFamilyCountsView::default();
    for work_item_type in work_item_types {
        match dto::family_of(work_item_type) {
            WorkItemFamily::Approval => counts.approval = counts.approval.saturating_add(1),
            WorkItemFamily::Procurement => {
                counts.procurement = counts.procurement.saturating_add(1);
            }
            WorkItemFamily::Fulfillment => {
                counts.fulfillment = counts.fulfillment.saturating_add(1);
            }
            WorkItemFamily::Finance => counts.finance = counts.finance.saturating_add(1),
            WorkItemFamily::Exception => counts.exception = counts.exception.saturating_add(1),
        }
    }
    counts
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

fn single_item_context_id(actor_id: &str, work_item_id: &str) -> String {
    QueueContextIdentity::new(
        "work-item-single",
        [
            QueueContextField::scalar("actor", actor_id),
            QueueContextField::scalar("work_item", work_item_id),
        ],
    )
    .into_string()
}

fn ensure_queue_context(provided: &Option<String>, expected: &str) -> Result<()> {
    if provided.as_deref().is_none_or(|provided| provided == expected) {
        return Ok(());
    }
    Err(Error::ConflictError("队列上下文已变化，请刷新队列".to_string()))
}

fn required_text(value: &str, message: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::ValidationError(message.to_string()));
    }
    Ok(value.to_string())
}

/// 将 HTTP 任务版本解析为正整数乐观锁版本。
pub(crate) fn expected_task_version(value: &str) -> Result<u64> {
    let value = value.trim();
    let version = value
        .parse::<u64>()
        .map_err(|_| Error::ValidationError("任务版本必须为正整数字符串".to_string()))?;
    if version == 0 {
        return Err(Error::ValidationError("任务版本必须为正整数字符串".to_string()));
    }
    Ok(version)
}

/// 判断工作项投影是否属于 W29 可受控关闭关系。
///
/// # 参数
/// * `item` - 已授权的工作项投影字段
///
/// # 返回
/// 非审批的集成异常或对账差异任务返回 `true`。
///
/// # 错误
/// 无。
fn is_w29_fields_closable(item: &dto::WorkItemFields) -> bool {
    item.work_item_type.is_w29_closable(
        &item.business_object_type,
        item.approval_node_execution_id.is_some(),
    )
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
        allowed_actions, approval_assignment_separated, audited_fact_operator_actors, authorized_fields,
        authorized_item_fields, business_day_bounds_at, counts_as_processable_stat, detail_scope,
        ensure_generic_work_item_mutation, expected_task_version, family_counts_for_types,
        has_assignment_candidate_access, non_empty_assignment_actors, object_access_shapes, object_policy,
        purchase_order_fulfillment_responsibility_id, remove_approval_decision_actions, ActorAccess,
        AssignmentSeparationPolicy, AuthorizedPage, AuthorizedPageCollector, Error, ObjectFact,
        ObjectFactMap, ObjectKind, ViewAccess, AUTHORIZED_SCAN_BATCH_SIZE,
    };
    use super::{ProcessingBlockerView, WorkItemAllowedAction, WorkItemScope};
    use crate::errors::ErrorCode;
    use entities::{
        common::time::Instant,
        ids::WorkItemId,
        work_item::{
            AssignmentSource, DocumentApprovalWorkItemData, WorkItem, WorkItemData, WorkItemPriority,
            WorkItemStatus, WorkItemType,
        },
        AccountKind, AuditLog, AuditLogData, Permission,
    };
    use std::collections::{HashMap, HashSet};

    /// 验证工作项管理员转交的授权提交栅栏。
    ///
    /// 转交必须以分派授权快照版本执行 policy CAS，不能退回仅在事务内读取比较。
    #[test]
    fn reassign_binds_assignment_authorization_to_commit() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码必须存在");

        assert!(production.contains("run_authorized_policy_transaction(policy_revision"));
        assert!(production.contains("item.work_item_type.requires_full_execution_permissions()"));
        assert!(!production.contains("ensure_policy_revision(&db, authorization.policy_revision"));
    }

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
            ObjectFact::new("sales-order-1", "应收子账 2", "sales-user"),
        )])
    }

    fn w13_delta_row() -> database::WorkItemRow {
        database::WorkItemRow {
            id: "wi-w13-delta".to_string(),
            work_item_type: WorkItemType::CardFundsDeltaReview,
            approval_node_execution_id: None,
            business_object_type: "receivable_account".to_string(),
            business_object_id: "account-1".to_string(),
            subject_version: "revision-2".to_string(),
            status: WorkItemStatus::Open,

            owner_role: "role-finance".to_string(),
            owner_organization_id: "company".to_string(),
            owner_user_id: Some("alice".to_string()),
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
                business_object_type: "receivable_account".to_string(),
                business_object_id: "account-1".to_string(),
                subject_version: "revision-2".to_string(),

                owner_role: "role-finance".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: "alice".to_string(),
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

    fn procurement_access(actor_id: &str) -> ActorAccess {
        ActorAccess {
            actor_id: actor_id.to_string(),
            permissions: vec![Permission::parse("purchase_order:create").unwrap()],
            participant_document_ids: HashSet::new(),
            organization_ids: Vec::new(),
            responsibility_scopes: Vec::new(),
            can_manage: false,
        }
    }

    fn procurement_facts() -> ObjectFactMap {
        HashMap::from([(
            (ObjectKind::SalesOrder, "sales-order-1".to_string()),
            ObjectFact::new("sales-order-1", "销售单 SO-1", "sales-user"),
        )])
    }

    fn procurement_item(owner_user_id: &str) -> WorkItem {
        WorkItem::new_with_responsibility_scope(
            WorkItemId::new("wi-procurement"),
            WorkItemData {
                work_item_type: WorkItemType::ProcurementOrderCreation,
                business_object_type: "sales_order".to_string(),
                business_object_id: "sales-order-1".to_string(),
                subject_version: "submission-1".to_string(),
                owner_role: "role-procurement".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: owner_user_id.to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: Some("SALES_ORDER_EFFECTIVE".to_string()),
                impact_summary: Some("1 行待分配供给".to_string()),
            },
            "sales-lines:digest".to_string(),
            vec!["sales-line-1".to_string()],
        )
        .unwrap()
    }

    fn fulfillment_item(
        business_object_type: &str,
        owner_role: &str,
        responsibility_key: &str,
        reason_code: &str,
    ) -> WorkItem {
        WorkItem::new_with_responsibility_key(
            WorkItemId::new(format!("wi-{business_object_type}")),
            WorkItemData {
                work_item_type: WorkItemType::FulfillmentOperation,
                business_object_type: business_object_type.to_string(),
                business_object_id: format!("{business_object_type}-1"),
                subject_version: "1".to_string(),
                owner_role: owner_role.to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: "owner-1".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: Some(reason_code.to_string()),
                impact_summary: None,
            },
            responsibility_key,
        )
        .unwrap()
    }

    fn invoice_execution_item(owner_user_id: &str) -> WorkItem {
        WorkItem::new_with_responsibility_key(
            WorkItemId::new("wi-invoice-execution"),
            WorkItemData {
                work_item_type: WorkItemType::SalesInvoiceExecution,
                business_object_type: "receivable_account".to_string(),
                business_object_id: "account-invoice-1".to_string(),
                subject_version: "1".to_string(),
                owner_role: "role-finance".to_string(),
                owner_organization_id: "party-1".to_string(),
                owner_user_id: owner_user_id.to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: Some("RECEIVABLE_INVOICE_REQUIRED".to_string()),
                impact_summary: Some("待开票金额 ¥100.00".to_string()),
            },
            "finance:SALES_INVOICE:rule-1",
        )
        .unwrap()
    }

    fn invoice_execution_facts() -> ObjectFactMap {
        HashMap::from([(
            (ObjectKind::ReceivableAccount, "account-invoice-1".to_string()),
            ObjectFact::new("sales-order-1", "应收子账 1", "sales-user"),
        )])
    }

    fn invoice_execution_access(actor_id: &str, full: bool) -> ActorAccess {
        let codes: &[&str] = if full {
            &[
                "receivable_account:list",
                "receivable_account:detail",
                "invoice:list",
                "invoice:detail",
                "invoice:create",
                "invoice:post",
            ]
        } else {
            &["receivable_account:detail"]
        };
        ActorAccess {
            actor_id: actor_id.to_string(),
            permissions: codes
                .iter()
                .map(|code| Permission::parse(code).unwrap())
                .collect(),
            participant_document_ids: HashSet::new(),
            organization_ids: Vec::new(),
            responsibility_scopes: Vec::new(),
            can_manage: false,
        }
    }

    fn managed_action_access() -> ActorAccess {
        ActorAccess {
            actor_id: "manager-1".to_string(),
            permissions: ["work_item:reassign", "work_item:close"]
                .into_iter()
                .map(|code| Permission::parse(code).unwrap())
                .collect(),
            participant_document_ids: HashSet::new(),
            organization_ids: vec!["company".to_string()],
            responsibility_scopes: Vec::new(),
            can_manage: true,
        }
    }

    fn managed_w29_item() -> WorkItem {
        WorkItem::new_at(
            WorkItemId::new("wi-managed-w29"),
            WorkItemData {
                work_item_type: WorkItemType::BusinessException,
                business_object_type: "integration_error_task".to_string(),
                business_object_id: "error-task-1".to_string(),
                subject_version: "1".to_string(),
                owner_role: "integration_error_handler".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: "worker-1".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: Some("INTEGRATION_RESULT_UNKNOWN".to_string()),
                impact_summary: None,
            },
            Instant::from_unix_secs(100),
        )
        .unwrap()
    }

    fn managed_w29_fields(has_approval_step: bool) -> super::dto::WorkItemFields {
        let item = managed_w29_item();
        let mut fields = super::dto::WorkItemFields::from(item);
        fields.approval_node_execution_id = has_approval_step.then(|| "approval-execution-1".to_string());
        fields
    }

    fn document_approval_fields_without_execution() -> super::dto::WorkItemFields {
        let item = WorkItem::new_document_approval(
            WorkItemId::new("wi-document-approval"),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: bpm::ApprovalNodeExecutionId::new("approval-execution-1"),
                business_object_type: "stock_adjustment".to_string(),
                business_object_id: "adjustment-1".to_string(),
                subject_version: "1".to_string(),
                owner_role: "stock_adjustment_approver".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: "worker-1".to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(100),
        )
        .unwrap();
        let mut fields = super::dto::WorkItemFields::from(item);
        fields.approval_node_execution_id = None;
        fields
    }

    #[test]
    fn w29_close_registry_excludes_other_business_exception_workspaces() {
        assert!(WorkItemType::IntegrationResultUnknown.is_w29_closable("integration_error_task", false,));
        assert!(WorkItemType::BusinessException.is_w29_closable("reconciliation_difference", false,));
        assert!(!WorkItemType::BusinessException.is_w29_closable("MASTER_MAPPING_TASK", false));
        assert!(!WorkItemType::BusinessException.is_w29_closable("SUPPLIER_OFFERING", false));
        assert!(!WorkItemType::BusinessException.is_w29_closable("SUPPLIER_FULFILLMENT_ORDER", false,));
        assert!(!WorkItemType::BusinessException.is_w29_closable("integration_error_task", true,));
    }

    #[test]
    fn w29_business_exception_integration_error_object_policy_is_registered() {
        let policy = object_policy(WorkItemType::BusinessException, "integration_error_task").unwrap();

        assert_eq!(policy.object_kind, ObjectKind::IntegrationErrorTask);
        assert_eq!(policy.read_permission, "integration_error_task:detail");
    }

    #[test]
    fn w13_delta_policy_authorizes_list_and_stats_projection() {
        let access = w13_access();
        let facts = w13_facts();
        let policy = object_policy(WorkItemType::CardFundsDeltaReview, "receivable_account").unwrap();

        assert_eq!(policy.object_kind, ObjectKind::ReceivableAccount);
        assert_eq!(policy.read_permission, "receivable_account:detail");
        assert!(object_access_shapes(&access).contains(&(
            WorkItemType::CardFundsDeltaReview,
            "receivable_account".to_string(),
        )));
        let fields = authorized_fields(vec![w13_delta_row()], &access, &facts);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].business_object_label, "应收子账 2");
    }

    #[test]
    fn w13_delta_policy_authorizes_detail_projection() {
        let fields = authorized_item_fields(w13_delta_item(), &w13_access(), &w13_facts()).unwrap();

        assert_eq!(fields.work_item_type, WorkItemType::CardFundsDeltaReview);
        assert_eq!(fields.business_object_label, "应收子账 2");
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
    fn procurement_owner_and_reassign_candidate_use_concrete_permission() {
        let owner_access = procurement_access("buyer-1");
        let facts = procurement_facts();
        let item = procurement_item("buyer-1");
        let policy = object_policy(WorkItemType::ProcurementOrderCreation, "sales_order").unwrap();

        assert_eq!(policy.object_kind, ObjectKind::SalesOrder);
        assert_eq!(policy.read_permission, "purchase_order:create");
        assert_eq!(
            WorkItemType::ProcurementOrderCreation.assignment_separation_policy(),
            AssignmentSeparationPolicy::RoleAndParticipation
        );
        let fields = authorized_item_fields(item.clone(), &owner_access, &facts).unwrap();
        assert!(
            allowed_actions(&fields, WorkItemScope::Mine, "buyer-1", &owner_access)
                .contains(&WorkItemAllowedAction::Process)
        );

        let candidate_access = procurement_access("buyer-2");
        assert!(has_assignment_candidate_access(&item, &candidate_access, &facts));
    }

    #[test]
    fn procurement_direct_assignment_still_requires_create_permission() {
        let item = procurement_item("buyer-1");
        let access = ActorAccess {
            actor_id: "buyer-1".to_string(),
            permissions: Vec::new(),
            participant_document_ids: HashSet::new(),
            organization_ids: Vec::new(),
            responsibility_scopes: Vec::new(),
            can_manage: false,
        };

        assert!(authorized_item_fields(item.clone(), &access, &procurement_facts()).is_none());
        assert!(!has_assignment_candidate_access(
            &item,
            &access,
            &procurement_facts()
        ));
    }

    #[test]
    fn fulfillment_candidate_requires_complete_execution_permissions() {
        let item = fulfillment_item(
            "purchase_receipt",
            "warehouse_inbound_handler",
            "warehouse:wh-1:receipt",
            "PURCHASE_RECEIPT_READY",
        );
        let facts = HashMap::from([(
            (ObjectKind::PurchaseReceipt, "purchase_receipt-1".to_string()),
            ObjectFact::new("po-1", "采购入库单 GRN-1", "__system__"),
        )]);
        let access = |codes: &[&str]| ActorAccess {
            actor_id: "candidate-1".to_string(),
            permissions: codes
                .iter()
                .map(|code| Permission::parse(code).unwrap())
                .collect(),
            participant_document_ids: HashSet::new(),
            organization_ids: Vec::new(),
            responsibility_scopes: Vec::new(),
            can_manage: false,
        };
        assert!(!has_assignment_candidate_access(
            &item,
            &access(&["purchase_receipt:post"]),
            &facts,
        ));
        assert!(has_assignment_candidate_access(
            &item,
            &access(&[
                "purchase_receipt:list",
                "purchase_receipt:detail",
                "purchase_receipt:update",
                "purchase_receipt:post",
            ]),
            &facts,
        ));
    }

    #[test]
    fn customer_acceptance_candidate_requires_complete_execution_permissions() {
        let item = WorkItem::new_with_responsibility_key(
            WorkItemId::new("wi-customer-acceptance"),
            WorkItemData {
                work_item_type: WorkItemType::CustomerAcceptanceRegistration,
                business_object_type: "sales_order".to_string(),
                business_object_id: "sales-order-1".to_string(),
                subject_version: "1".to_string(),
                owner_role: "sales_order_owner".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: "sales-1".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: Some("CUSTOMER_ACCEPTANCE_REQUIRED".to_string()),
                impact_summary: None,
            },
            "sales_order:sales-order-1:customer_acceptance",
        )
        .unwrap();
        let facts = HashMap::from([(
            (ObjectKind::SalesOrder, "sales-order-1".to_string()),
            ObjectFact::new("sales-order-1", "销售单 SO-1", "sales-1"),
        )]);
        let access = |codes: &[&str]| ActorAccess {
            actor_id: "candidate-1".to_string(),
            permissions: codes
                .iter()
                .map(|code| Permission::parse(code).unwrap())
                .collect(),
            participant_document_ids: HashSet::new(),
            organization_ids: Vec::new(),
            responsibility_scopes: Vec::new(),
            can_manage: false,
        };

        assert!(!has_assignment_candidate_access(
            &item,
            &access(&["sales_order:detail"]),
            &facts,
        ));
        assert!(has_assignment_candidate_access(
            &item,
            &access(&[
                "customer_acceptance:list",
                "customer_acceptance:detail",
                "customer_acceptance:create",
                "customer_acceptance:post",
                "sales_order:detail",
            ]),
            &facts,
        ));
    }

    #[test]
    fn invoice_owner_can_view_but_cannot_process_after_execution_permission_is_revoked() {
        let item = invoice_execution_item("finance-1");
        let facts = invoice_execution_facts();
        let revoked = invoice_execution_access("finance-1", false);
        let fields = authorized_item_fields(item.clone(), &revoked, &facts).unwrap();

        assert!(
            !allowed_actions(&fields, WorkItemScope::Mine, "finance-1", &revoked)
                .contains(&WorkItemAllowedAction::Process)
        );
        assert!(!has_assignment_candidate_access(&item, &revoked, &facts));

        let full = invoice_execution_access("finance-1", true);
        let fields = authorized_item_fields(item.clone(), &full, &facts).unwrap();
        assert!(allowed_actions(&fields, WorkItemScope::Mine, "finance-1", &full)
            .contains(&WorkItemAllowedAction::Process));
        assert!(has_assignment_candidate_access(&item, &full, &facts));
    }

    #[test]
    fn fulfillment_reassign_parses_only_registered_responsibility_keys() {
        let procurement = fulfillment_item(
            "delivery",
            "purchase_order_owner",
            "purchase_order:po-1",
            "SUPPLIER_DIRECT_DELIVERY_READY",
        );
        assert_eq!(
            purchase_order_fulfillment_responsibility_id(&procurement).unwrap(),
            Some("po-1".to_string())
        );

        let warehouse = fulfillment_item(
            "delivery",
            "warehouse_outbound_handler",
            "warehouse:wh-1:warehouse_ship",
            "WAREHOUSE_DELIVERY_READY",
        );
        assert_eq!(
            purchase_order_fulfillment_responsibility_id(&warehouse).unwrap(),
            None
        );

        let malformed = fulfillment_item(
            "delivery",
            "purchase_order_owner",
            "warehouse:wh-1:warehouse_ship",
            "SUPPLIER_DIRECT_DELIVERY_READY",
        );
        assert!(purchase_order_fulfillment_responsibility_id(&malformed).is_err());

        let mismatched_reason = fulfillment_item(
            "delivery",
            "purchase_order_owner",
            "purchase_order:po-1",
            "WAREHOUSE_DELIVERY_READY",
        );
        assert!(purchase_order_fulfillment_responsibility_id(&mismatched_reason).is_err());

        let mismatched_object = fulfillment_item(
            "purchase_receipt",
            "purchase_order_owner",
            "purchase_order:po-1",
            "SUPPLIER_DIRECT_DELIVERY_READY",
        );
        assert!(purchase_order_fulfillment_responsibility_id(&mismatched_object).is_err());
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

        let mine = ViewAccess::ready(vec![WorkItemAllowedAction::Process]);
        assert!(counts_as_processable_stat(WorkItemScope::Mine, &mine));
        assert!(!counts_as_processable_stat(WorkItemScope::Managed, &mine));
        assert!(!counts_as_processable_stat(WorkItemScope::History, &mine));
    }

    #[test]
    fn approval_tasks_count_even_though_they_never_get_process() {
        // 单据审批任务的动作集只有 View/Approve/Reject，仍必须计入「待我处理」，
        // 否则列表有条目而指标是 0。
        let approval = ViewAccess::ready(vec![
            WorkItemAllowedAction::View,
            WorkItemAllowedAction::Approve,
            WorkItemAllowedAction::Reject,
        ]);
        assert!(counts_as_processable_stat(WorkItemScope::Mine, &approval));

        let view_only = ViewAccess::ready(vec![WorkItemAllowedAction::View]);
        assert!(!counts_as_processable_stat(WorkItemScope::Mine, &view_only));
    }

    #[test]
    fn missing_approval_context_removes_decisions_but_keeps_view() {
        let mut actions = vec![
            WorkItemAllowedAction::View,
            WorkItemAllowedAction::Approve,
            WorkItemAllowedAction::Reject,
        ];

        assert!(remove_approval_decision_actions(&mut actions));
        assert_eq!(actions, vec![WorkItemAllowedAction::View]);
        assert!(!remove_approval_decision_actions(&mut actions));
    }

    #[test]
    fn managed_approval_items_never_project_generic_responsibility_actions() {
        let access = managed_action_access();

        let type_bound = document_approval_fields_without_execution();
        assert_eq!(
            allowed_actions(&type_bound, WorkItemScope::Managed, "manager-1", &access),
            vec![WorkItemAllowedAction::View]
        );

        let execution_bound = managed_w29_fields(true);
        assert_eq!(
            allowed_actions(&execution_bound, WorkItemScope::Managed, "manager-1", &access,),
            vec![WorkItemAllowedAction::View]
        );
    }

    #[test]
    fn managed_non_approval_item_still_projects_reassign_and_close() {
        let access = managed_action_access();
        let item = managed_w29_fields(false);

        assert_eq!(
            allowed_actions(&item, WorkItemScope::Managed, "manager-1", &access),
            vec![
                WorkItemAllowedAction::View,
                WorkItemAllowedAction::Reassign,
                WorkItemAllowedAction::Close,
            ]
        );
    }

    /// 类型标记或 execution 关联任一成立时，新鲜与回放分支都映射同一稳定错误。
    #[test]
    fn approval_generic_mutation_guard_is_stable_for_fresh_and_replay() {
        let mut type_bound = WorkItem::new_document_approval(
            WorkItemId::new("wi-approval-type-only"),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: bpm::ApprovalNodeExecutionId::new("approval-execution-1"),
                business_object_type: "stock_adjustment".to_string(),
                business_object_id: "adjustment-1".to_string(),
                subject_version: "1".to_string(),
                owner_role: "stock_adjustment_approver".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: "worker-1".to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(100),
        )
        .unwrap();
        type_bound.approval_node_execution_id = None;

        let mut execution_bound = managed_w29_item();
        execution_bound.approval_node_execution_id =
            Some(bpm::ApprovalNodeExecutionId::new("approval-execution-2"));

        for item in [&type_bound, &execution_bound] {
            for branch in ["fresh", "replay"] {
                let error =
                    ensure_generic_work_item_mutation(item).expect_err("审批任务的通用责任变更必须失败关闭");
                assert_eq!(
                    error.code(),
                    Some(ErrorCode::ApprovalGenericWorkItemMutationForbidden),
                    "{branch} 分支必须返回相同稳定错误",
                );
            }
        }
        assert!(ensure_generic_work_item_mutation(&managed_w29_item()).is_ok());
    }

    /// Service 必须在查询正式命令回执前识别并拒绝审批任务。
    #[test]
    fn generic_mutation_guard_precedes_idempotent_replay() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码必须存在");
        for method in ["pub async fn reassign(", "pub async fn close("] {
            let body = production
                .split_once(method)
                .map(|(_, tail)| tail)
                .expect("通用责任命令必须存在")
                .split_once("\n    }")
                .map(|(body, _)| body)
                .expect("通用责任命令必须闭合");
            let guard = body
                .find("ensure_generic_work_item_mutation(&item)")
                .expect("命令必须先识别审批任务");
            let replay = body
                .find("idempotent_replay(&receipt, id)")
                .expect("命令必须保留幂等回放");
            assert!(guard < replay, "审批任务守卫必须先于命令回放");
        }
    }

    #[test]
    fn family_counts_use_the_registered_server_mapping() {
        let counts = family_counts_for_types([
            WorkItemType::DocumentApproval,
            WorkItemType::ProcurementOrderCreation,
            WorkItemType::SalesInvoiceExecution,
            WorkItemType::InventoryAdjustmentReview,
            WorkItemType::BusinessException,
            WorkItemType::SupplierPaymentExecution,
        ]);

        assert_eq!(counts.approval, 1);
        assert_eq!(counts.procurement, 1);
        assert_eq!(counts.fulfillment, 1);
        assert_eq!(counts.finance, 2);
        assert_eq!(counts.exception, 1);
    }

    #[test]
    fn former_responsibility_actor_can_open_terminal_history_detail() {
        let mut item = WorkItem::new_at(
            WorkItemId::new("wi-history"),
            WorkItemData {
                work_item_type: WorkItemType::ImportBusinessConfirmation,
                business_object_type: "LEGACY_IMPORT_BATCH".to_string(),
                business_object_id: "batch-1".to_string(),
                subject_version: "v1".to_string(),

                owner_role: "role-sales".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: "alice".to_string(),
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
    fn approval_assignment_excludes_submitter_starter_history_and_decider() {
        let history = vec!["former-owner".to_string()];
        let decided = ["previous-decider"];

        assert!(!approval_assignment_separated(
            "starter",
            "starter",
            "submitter",
            &history,
            None,
            false,
            &decided,
        ));
        assert!(!approval_assignment_separated(
            "submitter",
            "starter",
            "submitter",
            &history,
            None,
            false,
            &decided,
        ));
        assert!(!approval_assignment_separated(
            "former-owner",
            "starter",
            "submitter",
            &history,
            None,
            false,
            &decided,
        ));
        assert!(!approval_assignment_separated(
            "previous-decider",
            "starter",
            "submitter",
            &history,
            None,
            false,
            &decided,
        ));
        assert!(approval_assignment_separated(
            "next-owner",
            "starter",
            "submitter",
            &history,
            None,
            false,
            &decided,
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
            WorkItemType::ImportBusinessConfirmation,
            WorkItemType::ImportBusinessConfirmation,
            WorkItemType::PurchaseOrderReview,
            WorkItemType::SalesChangeImpactReview,
            WorkItemType::SalesChangeFinanceReview,
            WorkItemType::CardFundsReview,
            WorkItemType::CardFundsDeltaReview,
            WorkItemType::InventoryAdjustmentReview,
            WorkItemType::SupplierSettlementReview,
        ] {
            assert_eq!(
                work_item_type.assignment_separation_policy(),
                AssignmentSeparationPolicy::DomainActors
            );
        }
        assert_eq!(
            WorkItemType::DocumentApproval.assignment_separation_policy(),
            AssignmentSeparationPolicy::ApprovalHistory
        );
        assert_eq!(
            WorkItemType::FinanceCorrectionReview.assignment_separation_policy(),
            AssignmentSeparationPolicy::FailClosed
        );
        assert_eq!(
            WorkItemType::DocumentApproval.assignment_separation_policy(),
            AssignmentSeparationPolicy::ApprovalHistory
        );
        assert!(object_policy(WorkItemType::DocumentApproval, "stock_adjustment").is_some());
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
