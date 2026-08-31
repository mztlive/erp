//! 域 D17 `inventory` 服务编排（页面：W10 库存台账）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 单集合无跨步骤原子性要求的 CRUD 传入 `&mut NoTransaction`；
//! - 表头+明细创建、状态迁移+审计、过账（调整单+流水+余额+预占）使用
//!   `database::Transactional::with_transaction`（§8.2 第 3 条跨集合原子性）。
//!
//! 跨域协作（P3-service-api §2：只调对方 Repository，不依赖对方 Service）：
//! - D11 `warehouse`：仓库代码 + 当前修订名称；
//! - D10 `catalog`：SKU 编号 + 当前修订名称/规格。
//!
//! 过账去重（§8.2）：状态守卫（仅 `IN_APPROVAL` 可由最终通过动作过账）+
//! `stock_movement` 的 `(source_document_id, source_line_id, movement_type)`
//! 唯一索引双重防护，重复过账返回 409，不产生第二条正式流水。

use std::collections::{HashMap, HashSet};

use database::{
    AccessControlExt, ApprovalIntegrationExt, BpmExt, Executor, InventoryExt, NoTransaction, Transactional,
    WorkItemExt,
};
use entities::common::source::SourceType;
use entities::common::time::Instant;
use entities::ids::{StockAdjustmentId, StockAdjustmentLineId, StockMovementId, StockReservationEntryId};
use entities::inventory::{
    AdjustmentReasonType, MovementDirection, MovementType, ReservationEntryType, StockAdjustment,
    StockAdjustmentData, StockAdjustmentLine, StockAdjustmentLineData, StockAdjustmentLineUpdate,
    StockAdjustmentUpdate, StockBalance, StockMovement, StockMovementData, StockReservation,
    StockReservationEntry, StockReservationEntryData,
};
use entities::money::Quantity;
use entities::work_item::{AssignmentSource, WorkItemStatus, WorkItemType};
use id_generator::next_id;
use mongodb::Database;
use std::str::FromStr;
use validator::Validate;

use crate::approval::binding::{
    attach_published_binding, bind_published_definition_on_document_create, BindPublishedDefinitionCommand,
};
use crate::approval::business_adapter::BindingRevalidationContext;
use crate::approval::execution::{prepare_start, PreparedExecution};
use crate::approval::ApprovalActionContext;
use crate::audit::AuditActor;
use crate::document_registry::{new_registered_document, persist_registered_document};
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::{BusinessDocument, DocumentType};

use self::adapter::{
    build_stock_adjustment_snapshot, execute_stock_adjustment_domain_action, require_frozen_binding,
    start_approval_command_kind, stock_adjustment_adapter, stock_adjustment_start_command,
    stock_adjustment_subject_ref, RECENT_HISTORY_LIMIT,
};
use self::dto::SortDir;
pub use self::dto::{
    CancelStockAdjustmentApprovalRequest, CancelStockAdjustmentApprovalTokenView,
    CreateStockAdjustmentRequest, DocumentApprovalView, ExpectedStockBalanceVersion, PageView,
    StockAdjustmentDetailView, StockAdjustmentLineInput, StockAdjustmentLineUpdateInput,
    StockAdjustmentLineView, StockAdjustmentListParams, StockAdjustmentSubmitResultQuery,
    StockAdjustmentView, StockBalanceDetailView, StockBalanceListParams, StockBalanceView,
    StockMovementListParams, StockMovementView, StockReservationListParams, StockReservationView,
    SubmitStockAdjustmentApprovalTokenView, SubmitStockAdjustmentRequest, UpdateStockAdjustmentRequest,
};

mod adapter;
mod approval_query;
mod cancel_approval;
mod dto;
mod start_approval;

/// 库存余额列表筛选条件类型（经 `InventoryExt` 关联类型跨 crate 可达）。
type StockBalanceFilter = <mongodb::Database as InventoryExt>::StockBalanceFilter;
/// 库存流水列表筛选条件类型。
type StockMovementFilter = <mongodb::Database as InventoryExt>::StockMovementFilter;
/// 库存预占列表筛选条件类型。
type StockReservationFilter = <mongodb::Database as InventoryExt>::StockReservationFilter;
/// 库存调整单列表筛选条件类型。
type StockAdjustmentFilter = <mongodb::Database as InventoryExt>::StockAdjustmentFilter;

/// 库存服务。
///
/// 提供余额/流水/预占/调整单查询、创建绑定、提交启动与普通受控撤回。
/// 最终通过过账和管理员受阻取消由审批动作注册表复用运行时事务执行。
pub struct InventoryService {
    db: Database,
    rbac: SharedRbacService,
}

impl InventoryService {
    /// 创建库存服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `rbac` - 共享 RBAC，用于创建时绑定发布定义
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 分页查询库存余额列表（W10 余额视图）。
    ///
    /// 列表行按页批量投影仓库与 SKU 基础信息（D11/D10 跨域只读），
    /// 不做逐行查询（禁止 N+1）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`warehouse_id`/`sku_id` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "inventory.stock_balance_list",
        skip_all,
        fields(layer = "service", domain = "inventory", operation = "stock_balance_list")
    )]
    pub async fn stock_balance_list(
        &self,
        params: &StockBalanceListParams,
    ) -> Result<PageView<StockBalanceView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = StockBalanceFilter {
            warehouse_id: query.warehouse_id,
            sku_id: query.sku_id,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .stock_balances()
            .search_stock_balances(&filter, &mut NoTransaction)
            .await?;
        let warehouse_ids: Vec<String> = page
            .items
            .iter()
            .map(|row| row.warehouse_id.to_string())
            .collect();
        let sku_ids: Vec<String> = page.items.iter().map(|row| row.sku_id.to_string()).collect();
        let movement_ids: Vec<String> = page
            .items
            .iter()
            .filter_map(|row| row.last_movement_id.as_ref().map(ToString::to_string))
            .collect();
        let enrichments = self
            .load_enrichments(&warehouse_ids, &sku_ids, &movement_ids)
            .await?;
        let active_reservation_dims = active_reservation_dims(&self.db, &warehouse_ids, &sku_ids).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| {
                let warehouse = enrichments.warehouses.get(&row.warehouse_id.to_string());
                let warehouse_name = warehouse
                    .and_then(|wh| wh.stable.current_revision_id.as_deref())
                    .and_then(|revision_id| enrichments.warehouse_revisions.get(revision_id))
                    .map(|revision| revision.name.clone());
                let sku = enrichments.skus.get(&row.sku_id.to_string());
                let sku_revision = sku
                    .and_then(|sku| sku.stable.current_revision_id.as_deref())
                    .and_then(|revision_id| enrichments.sku_revisions.get(revision_id));
                StockBalanceView {
                    id: row.id,
                    warehouse_id: row.warehouse_id.to_string(),
                    warehouse_code: warehouse.map(|wh| wh.warehouse_code.clone()).unwrap_or_default(),
                    warehouse_name: warehouse_name.unwrap_or_default(),
                    sku_id: row.sku_id.to_string(),
                    sku_code: sku.map(|sku| sku.sku_no.clone()).unwrap_or_default(),
                    sku_name: sku_revision
                        .map(|revision| revision.name.clone())
                        .unwrap_or_default(),
                    spec_summary: sku_revision.and_then(|revision| revision.specification.clone()),
                    on_hand_quantity: row.on_hand_quantity,
                    reserved_quantity: row.reserved_quantity,
                    available_quantity: row.available_quantity,
                    version: row.version.to_string(),
                    last_movement_id: row.last_movement_id.as_ref().map(ToString::to_string),
                    last_movement_at: row
                        .last_movement_id
                        .as_ref()
                        .and_then(|id| enrichments.movements.get(&id.to_string()))
                        .map(|movement| movement.fact.occurred_at.unix_secs()),
                    last_movement_type: row
                        .last_movement_id
                        .as_ref()
                        .and_then(|id| enrichments.movements.get(&id.to_string()))
                        .map(|movement| movement.movement_type),
                    has_active_reservation: active_reservation_dims
                        .contains(&(row.warehouse_id.to_string(), row.sku_id.to_string())),
                }
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询库存余额详情（W10 详情：余额 + 最近流水 + 有效预占 + 未过账调整）。
    ///
    /// # 参数
    /// * `id` - 余额主键
    ///
    /// # 返回
    /// 返回余额详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 余额不存在
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "inventory.stock_balance_detail",
        skip_all,
        fields(layer = "service", domain = "inventory", operation = "stock_balance_detail")
    )]
    pub async fn stock_balance_detail(&self, id: &str) -> Result<StockBalanceDetailView> {
        let balance = self
            .db
            .inventory()
            .stock_balance(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("库存余额不存在".to_string()))?;
        let filter = StockMovementFilter {
            warehouse_id: Some(balance.warehouse_id.clone()),
            sku_id: Some(balance.sku_id.clone()),
            movement_type: None,
            direction: None,
            occurred_from: None,
            occurred_to: None,
            page: 1,
            page_size: 8,
            sort_by: Some("occurred_at".to_string()),
            sort_ascending: false,
        };
        let movements = self
            .db
            .stock_movements()
            .search_stock_movements(&filter, &mut NoTransaction)
            .await?;
        let reservations = self
            .db
            .inventory()
            .operable_reservations_for_balance(&balance.warehouse_id, &balance.sku_id, &mut NoTransaction)
            .await?;
        let pending = self
            .db
            .inventory()
            .pending_adjustments_for_warehouse(&balance.warehouse_id, &mut NoTransaction)
            .await?;
        let enrichments = self
            .load_enrichments(
                &[balance.warehouse_id.to_string()],
                &[balance.sku_id.to_string()],
                &[],
            )
            .await?;
        let balance_view = build_balance_view(&balance, &enrichments, !reservations.is_empty());
        let mut recent_movements: Vec<StockMovementView> = movements
            .items
            .into_iter()
            .map(|row| StockMovementView {
                id: row.id,
                warehouse_id: row.warehouse_id.to_string(),
                sku_id: row.sku_id.to_string(),
                movement_type: row.movement_type,
                direction: row.direction,
                quantity: row.quantity,
                source_document_id: row.source_document_id,
                source_document_no: None,
                source_line_id: row.source_line_id,
                occurred_at: row.occurred_at.unix_secs(),
                recorded_at: row.recorded_at.unix_secs(),
                recorded_by: row.recorded_by.clone(),
            })
            .collect();
        let source_document_nos = load_movement_source_document_nos(&self.db, &recent_movements).await?;
        for movement in &mut recent_movements {
            movement.source_document_no = source_document_nos.get(&movement.source_document_id).cloned();
        }
        Ok(StockBalanceDetailView {
            balance: balance_view,
            recent_movements,
            active_reservations: reservations.into_iter().map(Into::into).collect(),
            pending_adjustments: pending.into_iter().map(Into::into).collect(),
        })
    }

    /// 分页查询库存流水台账（W10 流水视图，正式事实）。
    ///
    /// # 参数
    /// * `params` - 查询参数（仓库/SKU/类型/方向/发生时间区间筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页/时间区间/排序参数非法
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "inventory.stock_movement_list",
        skip_all,
        fields(layer = "service", domain = "inventory", operation = "stock_movement_list")
    )]
    pub async fn stock_movement_list(
        &self,
        params: &StockMovementListParams,
    ) -> Result<PageView<StockMovementView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = StockMovementFilter {
            warehouse_id: query.warehouse_id,
            sku_id: query.sku_id,
            movement_type: query.movement_type,
            direction: query.direction,
            occurred_from: query.occurred_from.map(Instant::from_unix_secs),
            occurred_to: query.occurred_to.map(Instant::from_unix_secs),
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .stock_movements()
            .search_stock_movements(&filter, &mut NoTransaction)
            .await?;
        let mut items: Vec<StockMovementView> = page
            .items
            .into_iter()
            .map(|row| StockMovementView {
                id: row.id,
                warehouse_id: row.warehouse_id.to_string(),
                sku_id: row.sku_id.to_string(),
                movement_type: row.movement_type,
                direction: row.direction,
                quantity: row.quantity,
                source_document_id: row.source_document_id,
                source_document_no: None,
                source_line_id: row.source_line_id,
                occurred_at: row.occurred_at.unix_secs(),
                recorded_at: row.recorded_at.unix_secs(),
                recorded_by: row.recorded_by.clone(),
            })
            .collect();
        let source_document_nos = load_movement_source_document_nos(&self.db, &items).await?;
        for item in &mut items {
            item.source_document_no = source_document_nos.get(&item.source_document_id).cloned();
        }
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 分页查询库存预占列表（W10 销售预占视图）。
    ///
    /// # 参数
    /// * `params` - 查询参数（仓库/SKU/状态/销售明细筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "inventory.stock_reservation_list",
        skip_all,
        fields(layer = "service", domain = "inventory", operation = "stock_reservation_list")
    )]
    pub async fn stock_reservation_list(
        &self,
        params: &StockReservationListParams,
    ) -> Result<PageView<StockReservationView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = StockReservationFilter {
            warehouse_id: query.warehouse_id,
            sku_id: query.sku_id,
            status: query.status,
            sales_order_line_id: query.sales_order_line_id,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .stock_reservations()
            .search_stock_reservations(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| StockReservationView {
                id: row.id,
                warehouse_id: row.warehouse_id.to_string(),
                sku_id: row.sku_id.to_string(),
                sales_order_line_id: row.sales_order_line_id.to_string(),
                reserved_quantity: row.reserved_quantity,
                consumed_quantity: row.consumed_quantity,
                released_quantity: row.released_quantity,
                status: row.status,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 分页查询库存调整单列表（W10 调整记录视图）。
    ///
    /// # 参数
    /// * `params` - 查询参数（仓库/状态筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "inventory.stock_adjustment_list",
        skip_all,
        fields(layer = "service", domain = "inventory", operation = "stock_adjustment_list")
    )]
    pub async fn stock_adjustment_list(
        &self,
        params: &StockAdjustmentListParams,
    ) -> Result<PageView<StockAdjustmentView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = StockAdjustmentFilter {
            warehouse_id: query.warehouse_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .stock_adjustments()
            .search_stock_adjustments(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| StockAdjustmentView {
                id: row.id,
                adjustment_no: row.adjustment_no,
                warehouse_id: row.warehouse_id.to_string(),
                reason_type: row.reason_type,
                status: row.status,
                prepared_by: row.prepared_by,
                reviewed_by: row.reviewed_by,
                finance_reviewed_by: row.finance_reviewed_by,
                note: row.note,
                occurred_at: row.occurred_at.map(|instant| instant.unix_secs()),
                version: row.version.to_string(),
                created_at: row.created_at,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询库存调整单详情（表头 + 明细 + 过账流水）。
    ///
    /// # 参数
    /// * `id` - 调整单主键
    /// * `actor` - 当前认证操作人，用于投影 actor-aware 审批动作
    ///
    /// # 返回
    /// 返回调整单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 调整单不存在
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "inventory.stock_adjustment_detail",
        skip_all,
        fields(
            layer = "service",
            domain = "inventory",
            operation = "stock_adjustment_detail"
        )
    )]
    pub async fn stock_adjustment_detail(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<StockAdjustmentDetailView> {
        self.stock_adjustment_detail_with_instance(id, actor, None).await
    }

    /// 构造库存调整详情；签署命令结果必须按收据引用的实例精确投影。
    async fn stock_adjustment_detail_with_instance(
        &self,
        id: &str,
        actor: &AuditActor,
        approval_instance: Option<(&str, u32)>,
    ) -> Result<StockAdjustmentDetailView> {
        let adjustment = self
            .db
            .inventory()
            .stock_adjustment(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))?;
        let lines = self
            .db
            .inventory()
            .adjustment_lines_by_adjustment_ids(&[adjustment.base.id.clone().into()], &mut NoTransaction)
            .await?;
        let movements = self
            .db
            .inventory()
            .movements_for_source_document(id, &mut NoTransaction)
            .await?;
        let binding = load_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_frozen_binding(binding.as_ref())?;
        let approval = match approval_instance {
            Some((instance_id, expected_subject_version)) => {
                approval_query::load_document_approval_for_instance(
                    self,
                    &adjustment,
                    Some(binding),
                    instance_id,
                    expected_subject_version,
                    actor,
                )
                .await?
            }
            None => approval_query::load_document_approval(self, &adjustment, Some(binding), actor).await?,
        };
        Ok(StockAdjustmentDetailView {
            approval,
            adjustment: adjustment.into(),
            lines: lines.into_iter().map(Into::into).collect(),
            posted_movements: movements.into_iter().map(Into::into).collect(),
        })
    }

    /// 创建库存调整单（草稿，跨集合：表头 + 明细 + 绑定 + 审计）。
    ///
    /// 同一事务内注册 `BusinessDocument` 并调用统一绑定端口。无已发布定义
    /// 时整体失败关闭，不得留下以后补流程的单据。
    ///
    /// # 参数
    /// * `req` - 创建请求（表头 + 明细）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建调整单的完整详情视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - 调整单号重复或流程未配置
    /// * `RepositoryError` - 数据库写入失败
    #[tracing::instrument(
        name = "inventory.stock_adjustment_create",
        skip_all,
        fields(
            layer = "service",
            domain = "inventory",
            operation = "stock_adjustment_create"
        )
    )]
    pub async fn create_stock_adjustment(
        &self,
        req: CreateStockAdjustmentRequest,
        actor: &AuditActor,
    ) -> Result<StockAdjustmentDetailView> {
        req.validate()?;
        let balance_id = req.balance_id.clone();
        let expected_balance_version = req.expected_balance_version;
        let id = StockAdjustmentId::new(next_id());
        let adjustment = StockAdjustment::new(
            id.clone(),
            StockAdjustmentData {
                adjustment_no: req.adjustment_no,
                warehouse_id: req.warehouse_id,
                reason_type: req.reason_type,
                prepared_by: actor.id().to_string(),
                note: req.note,
                occurred_at: req.occurred_at.map(Instant::from_unix_secs),
            },
        )?;
        let lines = build_adjustment_lines(&id, adjustment.reason_type, &req.lines)?;
        let audit =
            actor
                .clone()
                .resource_log("stock_adjustment.create", "stock_adjustment", id.to_string())?;
        let document = new_registered_document(
            &id,
            DocumentType::StockAdjustment,
            adjustment.adjustment_no.clone(),
        )?;
        let bind_command = BindPublishedDefinitionCommand {
            document_type: DocumentType::StockAdjustment,
            business_object_id: id.to_string(),
            business_object_version: adjustment.base.version,
            context: BindingRevalidationContext {
                organization_id: adjustment.warehouse_id.to_string(),
                creator_id: actor.id().to_string(),
            },
        };
        persist_created_adjustment(
            &self.db,
            &self.rbac,
            CreatedAdjustmentPersist {
                adjustment: adjustment.clone(),
                lines,
                document,
                bind_command,
                audit,
                actor: actor.clone(),
                balance_id,
                expected_balance_version,
            },
        )
        .await?;
        self.stock_adjustment_detail(id.as_ref(), actor).await
    }

    /// 更新库存调整单（仅草稿/驳回；乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 调整单主键
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后调整单的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 调整单不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    /// * `ValidationError` - 请求体校验失败
    #[tracing::instrument(
        name = "inventory.stock_adjustment_update",
        skip_all,
        fields(
            layer = "service",
            domain = "inventory",
            operation = "stock_adjustment_update"
        )
    )]
    pub async fn update_stock_adjustment(
        &self,
        id: &str,
        req: UpdateStockAdjustmentRequest,
        actor: &AuditActor,
    ) -> Result<StockAdjustmentView> {
        req.validate()?;
        let mut adjustment = self
            .db
            .inventory()
            .stock_adjustment(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))?;
        if !adjustment.matches_version(req.version) {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        let line_updates = build_adjustment_line_updates(req.lines.as_deref().unwrap_or_default())?;
        let requires_line_validation = req.reason_type.is_some() || !line_updates.is_empty();
        adjustment.update(StockAdjustmentUpdate {
            reason_type: req.reason_type,
            reviewed_by: None,
            finance_reviewed_by: None,
            note: req.note,
            occurred_at: req.occurred_at.map(Instant::from_unix_secs),
        })?;
        let audit =
            actor
                .clone()
                .resource_log("stock_adjustment.update", "stock_adjustment", id.to_string())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let adjustment_id = StockAdjustmentId::new(id.to_string());
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    if requires_line_validation {
                        let mut existing = db
                            .inventory()
                            .adjustment_lines_by_adjustment_ids(std::slice::from_ref(&adjustment_id), session)
                            .await?;
                        let changed = adjustment.apply_line_updates(&mut existing, &line_updates, false)?;
                        for line in &changed {
                            if !db.inventory().persist_adjustment_line(line, session).await? {
                                return Err(Error::NotFound("调整明细不存在".to_string()));
                            }
                        }
                    }
                    db.stock_adjustments().update(&mut adjustment, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<StockAdjustment, crate::errors::Error>(adjustment)
                })
            })
            .await?;
        Ok(updated.into())
    }

    /// 提交库存调整并调用统一 `start_approval`。
    ///
    /// 按合同 §4.4.1 冻结 `approval_subject_version` 与 `subject_snapshot`，
    /// 单据进入 `IN_APPROVAL`。定义与审批人取自已绑定事实，不接受客户端选择。
    ///
    /// # 参数
    /// * `id` - 调整单主键
    /// * `req` - 最终草稿、余额版本与幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回提交后的完整详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 调整单不存在
    /// * `ConflictError` - 非草稿、无绑定或并发冲突
    #[tracing::instrument(
        name = "inventory.stock_adjustment_submit",
        skip_all,
        fields(
            layer = "service",
            domain = "inventory",
            operation = "stock_adjustment_submit"
        )
    )]
    pub async fn submit_stock_adjustment(
        &self,
        id: &str,
        req: SubmitStockAdjustmentRequest,
        actor: &AuditActor,
    ) -> Result<StockAdjustmentDetailView> {
        req.validate()?;
        if let Some(view) = committed_stock_adjustment_start_replay(self, id, &req, actor).await? {
            return Ok(view);
        }
        let start_digest = start_approval::stock_adjustment_start_digest(&req, actor.id())?;
        let adapter = stock_adjustment_adapter()?;
        let subject = stock_adjustment_subject_ref(id)?;
        let mut adjustment = self.load_stock_adjustment(id).await?;
        start_approval::ensure_stock_adjustment_submit_authorized_with_executor(
            &self.db,
            &self.rbac,
            &adjustment,
            actor,
            &mut NoTransaction,
        )
        .await?;
        if !adjustment.matches_version(req.expected_version) {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        let target_subject_version = adjustment
            .approval_subject_version
            .checked_add(1)
            .ok_or_else(|| Error::ConflictError("库存调整审批主题版本已达上限".to_string()))?;
        if req.expected_subject_version != target_subject_version {
            return Err(Error::ConflictError(
                "库存调整审批主题版本已变化，请刷新后重试".to_string(),
            ));
        }
        adjustment.update(StockAdjustmentUpdate {
            reason_type: Some(req.reason_type),
            reviewed_by: None,
            finance_reviewed_by: None,
            note: Some(req.note.clone()),
            occurred_at: Some(Instant::from_unix_secs(req.occurred_at)),
        })?;
        let binding = load_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let mut lines = self
            .db
            .inventory()
            .adjustment_lines_by_adjustment_ids(&[StockAdjustmentId::new(id.to_string())], &mut NoTransaction)
            .await?;
        let line_updates = build_adjustment_line_updates(&req.lines)?;
        adjustment.apply_line_updates(&mut lines, &line_updates, true)?;
        execute_stock_adjustment_domain_action(&mut adjustment, adapter.on_approval_start)?;
        let now = Instant::now();
        let snapshot = build_stock_adjustment_snapshot(&adjustment, &lines, actor.id(), now)?;
        let start = stock_adjustment_start_command(
            id,
            adjustment.approval_subject_version,
            actor.id(),
            &req.idempotency_key,
        );
        let _ = (start_approval_command_kind(&start), RECENT_HISTORY_LIMIT);
        let organization_id = adjustment.warehouse_id.to_string();
        let graph = start_approval::load_bound_definition_graph(&self.db, &binding).await?;
        let start_input = start_approval::build_stock_adjustment_start_input(
            &self.db,
            &self.rbac,
            start_approval::StockAdjustmentStartInput {
                graph,
                binding: &binding,
                subject,
                subject_version: adjustment.approval_subject_version,
                actor_id: actor.id(),
                organization_id: &organization_id,
                idempotency_key: &req.idempotency_key,
                receipt: None,
                now,
            },
            &mut NoTransaction,
        )
        .await?;
        let PreparedExecution::Apply(mut writes) = prepare_start(start_input)? else {
            return Err(Error::Internal("新库存调整提交不得进入回放分支".to_string()));
        };
        writes.receipt.payload_digest = start_digest;
        let result_instance_id = writes.instance.base.id.clone();
        let persist_result = start_approval::persist_stock_adjustment_start(
            &self.db,
            start_approval::StockAdjustmentStartPersistInput {
                rbac: self.rbac.clone(),
                adjustment,
                actor: actor.clone(),
                id: id.to_string(),
                snapshot_payload: snapshot,
                writes: *writes,
                binding,
                owner_role: adapter.owner_role,
                organization_id,
                now,
                lines,
                balances: req.balances.clone(),
                expected_document_version: req.expected_version,
                expected_subject_version: req.expected_subject_version,
            },
        )
        .await;
        if let Err(error) = persist_result {
            if let Some(view) = recover_stock_adjustment_start_replay(self, id, &req, actor).await? {
                return Ok(view);
            }
            return Err(error);
        }
        self.stock_adjustment_detail_with_instance(
            id,
            actor,
            Some((&result_instance_id, req.expected_subject_version)),
        )
        .await
    }

    /// 按原 StartApproval 作用域和幂等键查询已提交的库存调整结果。
    ///
    /// 本端口只承认精确存在且能通过 receipt → instance → snapshot → frozen
    /// binding → actor/current scope 验证的收据；不存在或错 key 返回 `NotFound`，
    /// 不得依据当前单据状态推断原命令成功。
    pub async fn stock_adjustment_submit_result(
        &self,
        id: &str,
        query: StockAdjustmentSubmitResultQuery,
        actor: &AuditActor,
    ) -> Result<StockAdjustmentDetailView> {
        query.validate()?;
        let detail_id = id.to_string();
        let detail_actor = actor.clone();
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let id = id.to_string();
        let actor = actor.clone();
        let query_for_read = query.clone();
        let client = db.client().clone();
        let result_ref = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    start_approval::find_stock_adjustment_start_result(
                        &db,
                        &rbac,
                        &id,
                        query_for_read.expected_subject_version,
                        &query_for_read.idempotency_key,
                        &actor,
                        session,
                    )
                    .await
                })
            })
            .await?;
        let result_ref = result_ref.ok_or_else(|| Error::NotFound("库存调整提交结果不存在".to_string()))?;
        self.stock_adjustment_detail_with_instance(
            &detail_id,
            &detail_actor,
            Some((&result_ref, query.expected_subject_version)),
        )
        .await
    }

    /// 在审批运行时持有的唯一事务内过账库存调整单。
    ///
    /// 本方法是合同 §4.4.4 签署的最终通过端口。调用方必须传入审批决定
    /// 事务的执行器；`NoTransaction` 会失败关闭，防止脱离 BPM 终态写入
    /// 单据、库存流水、余额和审计。
    ///
    /// # 参数
    /// * `context` - 审批运行时冻结的实例、执行、任务、业务对象与主题版本
    /// * `actor` - 已认证的审批决定操作人
    /// * `executor` - 审批运行时持有的事务执行器
    ///
    /// # 返回
    /// 返回事务内已过账的库存调整单视图。
    ///
    /// # 错误
    /// 缺少事务会话、非审批中、明细/余额不完整、库存不足、重复过账或任一
    /// 写入失败时返回错误；事务由审批运行时整体提交或回滚。
    pub async fn post_stock_adjustment(
        &self,
        context: &ApprovalActionContext,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<StockAdjustmentView> {
        let session = executor
            .session()
            .ok_or_else(|| Error::Internal("库存调整审批过账缺少运行时事务会话".to_string()))?;
        post_stock_adjustment_write(&self.db, context, actor, session)
            .await
            .map(Into::into)
    }

    /// 按主键读取库存调整单。
    ///
    /// # 错误
    /// 不存在时返回 `NotFound`。
    async fn load_stock_adjustment(&self, id: &str) -> Result<StockAdjustment> {
        self.db
            .inventory()
            .stock_adjustment(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))
    }

    /// 批量加载余额列表/详情的基础信息投影（仓库/仓库修订/SKU/SKU修订/最后流水）。
    ///
    /// 全部按 `$in` 批量取回（禁止 N+1）。
    ///
    /// # 参数
    /// * `warehouse_ids` - 仓库主键集合
    /// * `sku_ids` - SKU 主键集合
    /// * `movement_ids` - 流水主键集合（可为空）
    ///
    /// # 返回
    /// 返回按主键索引的投影映射。
    ///
    /// # 错误
    /// 任一批量查询失败时返回 `RepositoryError`。
    async fn load_enrichments(
        &self,
        warehouse_ids: &[String],
        sku_ids: &[String],
        movement_ids: &[String],
    ) -> Result<BalanceEnrichments> {
        let warehouses = load_warehouses_by_ids(&self.db, warehouse_ids).await?;
        let skus = load_skus_by_ids(&self.db, sku_ids).await?;
        let warehouse_revisions = load_warehouse_revisions(&self.db, &warehouses).await?;
        let sku_revisions = load_sku_revisions(&self.db, &skus).await?;
        let movements = load_movements_by_ids(&self.db, movement_ids).await?;
        Ok(BalanceEnrichments {
            warehouses,
            warehouse_revisions,
            skus,
            sku_revisions,
            movements,
        })
    }
}

const START_REPLAY_RECOVERY_ATTEMPTS: usize = 32;

/// 使用独立只读事务先解析启动收据，再返回当前可读详情。
async fn committed_stock_adjustment_start_replay(
    service: &InventoryService,
    id: &str,
    req: &SubmitStockAdjustmentRequest,
    actor: &AuditActor,
) -> Result<Option<StockAdjustmentDetailView>> {
    let db = service.db.clone();
    let rbac = service.rbac.clone();
    let id_owned = id.to_string();
    let req_owned = req.clone();
    let actor_owned = actor.clone();
    let client = db.client().clone();
    let result_ref = client
        .with_transaction(move |session| {
            Box::pin(async move {
                start_approval::reconcile_stock_adjustment_start_receipt(
                    &db,
                    &rbac,
                    &id_owned,
                    &req_owned,
                    &actor_owned,
                    session,
                )
                .await
            })
        })
        .await?;
    let Some(result_ref) = result_ref else {
        return Ok(None);
    };
    Ok(Some(
        service
            .stock_adjustment_detail_with_instance(
                id,
                actor,
                Some((&result_ref, req.expected_subject_version)),
            )
            .await?,
    ))
}

/// 事务失败或结果未知后，以有限次新会话等待并发 winner 的收据可见。
async fn recover_stock_adjustment_start_replay(
    service: &InventoryService,
    id: &str,
    req: &SubmitStockAdjustmentRequest,
    actor: &AuditActor,
) -> Result<Option<StockAdjustmentDetailView>> {
    for _ in 0..START_REPLAY_RECOVERY_ATTEMPTS {
        if let Some(view) = committed_stock_adjustment_start_replay(service, id, req, actor).await? {
            return Ok(Some(view));
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    Ok(None)
}

/// 在审批运行时持有的事务内过账库存调整单。
///
/// # 参数
/// * `db` - 数据库实例
/// * `adjustment_id` - 库存调整单 ID
/// * `actor` - 已认证操作人
/// * `session` - 审批运行时持有的唯一事务会话
///
/// # 返回
/// 返回事务内已推进到过账状态的调整单。
///
/// # 错误
/// 调整单/明细不存在、方向或库存不变量失败、任一写入失败时返回错误。
async fn post_stock_adjustment_write(
    db: &Database,
    context: &ApprovalActionContext,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<StockAdjustment> {
    let adjustment_id = StockAdjustmentId::new(context.business_object_id.clone());
    let mut adjustment = db
        .inventory()
        .stock_adjustment(adjustment_id.as_ref(), session)
        .await?
        .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))?;
    validate_post_runtime_context(db, context, actor, &adjustment, session).await?;
    adjustment
        .ensure_approval_postable()
        .map_err(|error| Error::ConflictError(error.to_string()))?;
    let lines = db
        .inventory()
        .adjustment_lines_by_adjustment_ids(std::slice::from_ref(&adjustment_id), session)
        .await?;
    if lines.is_empty() {
        return Err(Error::ValidationError("库存调整单没有明细，无法过账".to_string()));
    }
    let occurred_at = Instant::now();
    for line in &lines {
        adjustment.reason_type.ensure_direction(line.direction)?;
        post_adjustment_line(db, session, &adjustment, line, &occurred_at, actor).await?;
    }
    adjustment.mark_posted()?;
    db.stock_adjustments().update(&mut adjustment, session).await?;
    let audit = actor.clone().resource_log(
        "stock_adjustment.post",
        "stock_adjustment",
        adjustment_id.to_string(),
    )?;
    db.audit_logs().create(&audit, session).await?;
    Ok(adjustment)
}

/// 逐项验证最终通过动作仍对应当前运行实例、执行、任务和冻结主题版本。
///
/// # 错误
/// 任一运行身份缺失、漂移，或当前处理人与开放任务责任不一致时返回冲突，
/// 禁止写入任何库存事实。
async fn validate_post_runtime_context(
    db: &Database,
    context: &ApprovalActionContext,
    actor: &AuditActor,
    adjustment: &StockAdjustment,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    if context.business_object_type != DocumentType::StockAdjustment.as_str()
        || context.actor_id != actor.id()
    {
        return Err(Error::ConflictError("库存调整最终动作上下文不匹配".to_string()));
    }
    let subject_version = context
        .subject_version
        .parse::<u32>()
        .map_err(|_| Error::ConflictError("库存调整审批主题版本无效".to_string()))?;
    if subject_version != adjustment.approval_subject_version {
        return Err(Error::ConflictError("库存调整审批主题版本已变化".to_string()));
    }
    let execution_id = context
        .approval_node_execution_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::ConflictError("库存调整最终动作缺少审批执行".to_string()))?;
    let work_item_id = context
        .work_item_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::ConflictError("库存调整最终动作缺少审批任务".to_string()))?;
    let instance_id = bpm::ids::ApprovalProcessInstanceId::new(&context.approval_process_instance_id);
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&instance_id, session)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整审批实例不存在".to_string()))?;
    let binding = load_approval_binding(db, &adjustment.base.id, session).await?;
    let binding = require_frozen_binding(binding.as_ref())?;
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(&instance.base.id, session)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整审批实例缺少冻结快照".to_string()))?;
    snapshot
        .ensure_matches_runtime_subject(
            DocumentType::StockAdjustment,
            adjustment.base.id.as_str(),
            subject_version,
        )
        .map_err(|_| Error::ConflictError("库存调整审批冻结快照已变化".to_string()))?;
    let current_execution = instance
        .current_node_execution_id
        .as_ref()
        .map(ToString::to_string);
    if instance.status != bpm::model::types::ApprovalProcessInstanceStatus::Running
        || instance.process_kind
            != crate::approval::process_kind::process_kind_of(DocumentType::StockAdjustment)
        || instance.process_definition_id != binding.approval_process_definition_id
        || instance.definition_version != binding.approval_definition_version
        || instance.subject.subject_kind() != DocumentType::StockAdjustment.as_str()
        || instance.subject.subject_id() != adjustment.base.id
        || instance.subject_version != subject_version
        || current_execution.as_deref() != Some(execution_id)
    {
        return Err(Error::ConflictError("库存调整审批实例上下文已变化".to_string()));
    }
    let execution = db
        .bpm_workflow()
        .find_execution_by_id(&bpm::ids::ApprovalNodeExecutionId::new(execution_id), session)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整审批执行不存在".to_string()))?;
    if execution.process_instance_id != instance_id
        || execution.status != bpm::model::types::ApprovalNodeExecutionStatus::Active
        || execution.round_no != instance.current_round_no
        || execution.blocker_code.is_some()
        || execution.assignee_participant_id.as_str() != actor.id()
    {
        return Err(Error::ConflictError("库存调整审批执行上下文已变化".to_string()));
    }
    let task = db
        .work_items()
        .find_document_approval_by_id(work_item_id, session)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整审批任务不存在".to_string()))?;
    let adapter = stock_adjustment_adapter()?;
    if task.work_item_type != WorkItemType::DocumentApproval
        || task.status != WorkItemStatus::Open
        || task.assignment_source != AssignmentSource::ApprovalRuntime
        || task
            .approval_node_execution_id
            .as_ref()
            .map(ToString::to_string)
            .as_deref()
            != Some(execution_id)
        || task.business_object_type != DocumentType::StockAdjustment.as_str()
        || task.business_object_id != adjustment.base.id
        || task.subject_version != context.subject_version
        || task.owner_user_id.as_deref() != Some(actor.id())
        || task.owner_role != adapter.owner_role
        || task.owner_organization_id != snapshot.payload.responsible_org_id
    {
        return Err(Error::ConflictError("库存调整审批任务上下文已变化".to_string()));
    }
    Ok(())
}

/// 在审批运行时持有的事务内撤回库存调整审批。
///
/// # 错误
/// 调整单不存在、动作不匹配、状态迁移或 CAS 写入失败时返回错误。
pub(crate) async fn cancel_stock_adjustment_approval_in_transaction(
    db: &Database,
    context: &ApprovalActionContext,
    action: crate::approval::policy::ApprovalDomainAction,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = executor
        .session()
        .ok_or_else(|| Error::Internal("库存调整受阻取消缺少运行时事务会话".to_string()))?;
    if context.business_object_type != DocumentType::StockAdjustment.as_str()
        || context.actor_id != actor.id()
        || context.work_item_id.is_some()
    {
        return Err(Error::ConflictError("库存调整受阻取消上下文不匹配".to_string()));
    }
    let subject_version = context
        .subject_version
        .parse::<u32>()
        .map_err(|_| Error::ConflictError("库存调整审批主题版本无效".to_string()))?;
    let adjustment_id = StockAdjustmentId::new(&context.business_object_id);
    let mut adjustment = db
        .inventory()
        .stock_adjustment(adjustment_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))?;
    if adjustment.approval_subject_version != subject_version {
        return Err(Error::ConflictError("库存调整审批主题版本已变化".to_string()));
    }
    validate_blocked_cancel_runtime_context(db, context, actor, subject_version, executor).await?;
    execute_stock_adjustment_domain_action(&mut adjustment, action)?;
    db.stock_adjustments().update(&mut adjustment, executor).await?;
    let audit = actor.clone().resource_log(
        "stock_adjustment.cancel_approval",
        "stock_adjustment",
        adjustment_id.to_string(),
    )?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(())
}

/// 校验受阻取消动作仍精确指向同一 BLOCKED 实例、执行和无任务事实。
async fn validate_blocked_cancel_runtime_context(
    db: &Database,
    context: &ApprovalActionContext,
    actor: &AuditActor,
    subject_version: u32,
    executor: &mut dyn Executor,
) -> Result<()> {
    let execution_id = context
        .approval_node_execution_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::ConflictError("库存调整受阻取消缺少审批执行".to_string()))?;
    let instance_id = bpm::ids::ApprovalProcessInstanceId::new(&context.approval_process_instance_id);
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&instance_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整审批实例不存在".to_string()))?;
    let binding = load_approval_binding(db, &context.business_object_id, executor).await?;
    let binding = require_frozen_binding(binding.as_ref())?;
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(&instance.base.id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整受阻实例缺少冻结快照".to_string()))?;
    snapshot
        .ensure_matches_runtime_subject(
            DocumentType::StockAdjustment,
            &context.business_object_id,
            subject_version,
        )
        .map_err(|_| Error::ConflictError("库存调整受阻实例冻结快照已变化".to_string()))?;
    if instance.status != bpm::model::types::ApprovalProcessInstanceStatus::Blocked
        || instance.process_kind
            != crate::approval::process_kind::process_kind_of(DocumentType::StockAdjustment)
        || instance.process_definition_id != binding.approval_process_definition_id
        || instance.definition_version != binding.approval_definition_version
        || instance.subject.subject_kind() != DocumentType::StockAdjustment.as_str()
        || instance.subject.subject_id() != context.business_object_id
        || instance.subject_version != subject_version
        || instance.current_node_execution_id.as_ref().map(AsRef::as_ref) != Some(execution_id)
    {
        return Err(Error::ConflictError("库存调整受阻实例上下文已变化".to_string()));
    }
    let execution_id = bpm::ids::ApprovalNodeExecutionId::new(execution_id);
    let execution = db
        .bpm_workflow()
        .find_execution_by_id(&execution_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整受阻执行不存在".to_string()))?;
    if execution.process_instance_id != instance_id
        || execution.status != bpm::model::types::ApprovalNodeExecutionStatus::Blocked
        || execution.round_no != instance.current_round_no
        || execution.blocker_code.is_none()
        || execution.blocker_code != instance.blocker_code
        || context.actor_id != actor.id()
    {
        return Err(Error::ConflictError("库存调整受阻执行上下文已变化".to_string()));
    }
    if !db
        .work_items()
        .open_approval_tasks_for_execution(&execution_id, executor)
        .await?
        .is_empty()
    {
        return Err(Error::ConflictError(
            "受阻库存调整执行不得存在开放审批任务".to_string(),
        ));
    }
    Ok(())
}

/// 库存调整单创建事务的单据、绑定与审计载荷。
///
/// # 用途
/// 将调整单、明细、单据注册、绑定命令与审计打包后一次写入。
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
/// 绑定失败必须回滚业务实体，不得留下以后补流程的单据。
struct CreatedAdjustmentPersist {
    /// 已构造的调整单。
    adjustment: StockAdjustment,
    /// 调整明细。
    lines: Vec<StockAdjustmentLine>,
    /// 待登记单据。
    document: BusinessDocument,
    /// 发布定义绑定命令。
    bind_command: BindPublishedDefinitionCommand,
    /// 已构造审计。
    audit: entities::AuditLog,
    /// 审计操作人。
    actor: AuditActor,
    /// 用户发起时所依据的库存余额行。
    balance_id: String,
    /// 用户发起时看到的库存余额版本。
    expected_balance_version: u64,
}

/// 在创建事务内写入调整单、绑定发布定义并登记单据。
///
/// 绑定失败必须回滚业务实体，不得留下以后补流程的单据。
///
/// # 用途
/// 在同一事务内写入调整单、绑定与审计。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `persist` - 调整单、单据绑定与审计
///
/// # 返回
/// 写入成功时返回 `Ok(())`。
///
/// # 错误
/// 无发布定义、人员重验失败或写入失败时返回错误。
///
/// # 关键业务约束
/// 绑定失败必须整体回滚。
async fn persist_created_adjustment(
    db: &Database,
    rbac: &SharedRbacService,
    persist: CreatedAdjustmentPersist,
) -> Result<()> {
    let CreatedAdjustmentPersist {
        adjustment,
        lines,
        mut document,
        bind_command,
        audit,
        actor,
        balance_id,
        expected_balance_version,
    } = persist;
    let db = db.clone();
    let rbac = rbac.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                let balance = db
                    .inventory()
                    .stock_balance(&balance_id, session)
                    .await?
                    .ok_or_else(|| Error::NotFound("库存余额不存在".to_string()))?;
                if !balance.matches_version(expected_balance_version) {
                    return Err(Error::ConflictError("库存余额已变化，请刷新后重试".to_string()));
                }
                if !balance.matches_adjustment_dimensions(&adjustment, &lines) {
                    return Err(Error::ValidationError("库存余额与调整单维度不一致".to_string()));
                }
                db.inventory()
                    .create_stock_adjustment_with_lines(&adjustment, &lines, session)
                    .await?;
                persist_bound_document(&db, &rbac, &mut document, &bind_command, &actor, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}

/// 把服务输入转换为已解析的调整明细更新值对象。
///
/// # 参数
/// * `updates` - 客户端提交的明细更新
///
/// # 返回
/// 返回完成主键规范化与数量解析的值对象集合。
///
/// # 错误
/// 行主键或数量非法时返回 `ValidationError`。
fn build_adjustment_line_updates(
    updates: &[self::dto::StockAdjustmentLineUpdateInput],
) -> Result<Vec<StockAdjustmentLineUpdate>> {
    updates
        .iter()
        .map(|update| {
            StockAdjustmentLineUpdate::new(update.line_id.clone(), &update.quantity, update.direction)
                .map_err(|error| Error::ValidationError(error.to_string()))
        })
        .collect()
}

/// 按执行器加载库存调整单的审批绑定。
///
/// # 参数
/// * `db` - 数据库实例
/// * `document_id` - 库存调整单与注册单据共用的主键
/// * `executor` - 数据访问执行器，由调用方决定是否位于事务中
///
/// # 返回
/// 返回已冻结审批绑定；单据尚未绑定时返回 `None`。
///
/// # 错误
/// 单据未注册或仓储查询失败时返回错误。
async fn load_approval_binding(
    db: &Database,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalDefinitionBinding>> {
    let document = db
        .inventory()
        .business_document(document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("业务单据未注册".to_string()))?;
    Ok(document.approval_binding)
}

/// 查询发布定义、写入绑定并持久化注册行。
///
/// # 错误
/// 无发布定义或绑定失败时返回错误，调用方必须回滚。
async fn persist_bound_document(
    db: &Database,
    rbac: &SharedRbacService,
    document: &mut BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, session).await?;
    let binding = binding.ok_or_else(|| Error::Internal("库存调整单必须绑定已发布定义".to_string()))?;
    attach_published_binding(document, binding)?;
    persist_registered_document(db, document, session).await
}

/// 余额列表/详情的基础信息投影映射。
struct BalanceEnrichments {
    /// 仓库按主键索引。
    warehouses: HashMap<String, entities::warehouse::Warehouse>,
    /// 仓库修订按主键索引。
    warehouse_revisions: HashMap<String, entities::warehouse::WarehouseRevision>,
    /// SKU 按主键索引。
    skus: HashMap<String, entities::catalog::Sku>,
    /// SKU 修订按主键索引。
    sku_revisions: HashMap<String, entities::catalog::SkuRevision>,
    /// 库存流水按主键索引。
    movements: HashMap<String, StockMovement>,
}

/// 批量取页内余额维度上是否存在有效预占（禁止 N+1）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `warehouse_ids` - 页内仓库主键集合
/// * `sku_ids` - 页内 SKU 主键集合
///
/// # 返回
/// 返回「(仓库, SKU)」有效预占维度集合。
///
/// # 错误
/// 批量查询失败时返回 `RepositoryError`。
async fn active_reservation_dims(
    db: &Database,
    warehouse_ids: &[String],
    sku_ids: &[String],
) -> Result<HashSet<(String, String)>> {
    if warehouse_ids.is_empty() || sku_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let reservations = db
        .inventory()
        .operable_reservations_for_dimensions(warehouse_ids, sku_ids, &mut NoTransaction)
        .await?;
    Ok(reservations
        .into_iter()
        .map(|reservation| {
            (
                reservation.warehouse_id.to_string(),
                reservation.sku_id.to_string(),
            )
        })
        .collect())
}

/// 过账单条调整明细（流水 + 余额 + 适用预占释放，位于调用方事务内）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `adjustment` - 调整单表头
/// * `line` - 调整明细
/// * `occurred_at` - 过账业务时间
/// * `actor` - 审计操作人（记录人身份）
///
/// # 返回
/// 无返回值；流水/余额/预占写入失败时返回错误。
///
/// # 错误
/// 余额缺失、可用量不足或写入失败时返回错误。
async fn post_adjustment_line(
    db: &Database,
    session: &mut mongodb::ClientSession,
    adjustment: &StockAdjustment,
    line: &StockAdjustmentLine,
    occurred_at: &Instant,
    actor: &AuditActor,
) -> Result<()> {
    let movement_type = adjustment.reason_type.movement_type();
    let movement = StockMovement::new(
        StockMovementId::new(next_id()),
        StockMovementData {
            warehouse_id: adjustment.warehouse_id.clone(),
            sku_id: line.sku_id.clone(),
            movement_type,
            direction: line.direction,
            quantity: line.quantity,
            source_document_id: adjustment.base.id.clone(),
            source_line_id: Some(line.base.id.clone()),
            reversal_of_movement_id: None,
            fact_no: next_id(),
            occurred_at: adjustment.occurred_at.unwrap_or(*occurred_at),
            recorded_at: *occurred_at,
            recorded_by: actor.id().to_string(),
            source_type: SourceType::Erp,
            source_reference: None,
            reason_code: Some(adjustment.reason_type.as_str().to_string()),
            reason_text: adjustment.note.clone(),
        },
    )?;
    db.stock_movements().create(&movement, session).await?;

    let balance = db
        .inventory()
        .balance_for_dimensions(&adjustment.warehouse_id, &line.sku_id, session)
        .await?
        .ok_or_else(|| {
            Error::BusinessLogicError(format!(
                "库存余额不存在（仓库 {}，SKU {}），请先建立期初或入库",
                adjustment.warehouse_id.as_ref(),
                line.sku_id.as_ref()
            ))
        })?;
    match line.direction {
        MovementDirection::Increase => {
            if !db
                .stock_balances()
                .increase_on_hand(&balance.base.id, line.quantity, session)
                .await?
            {
                return Err(Error::BusinessLogicError("库存余额行不存在".to_string()));
            }
        }
        MovementDirection::Decrease => {
            release_applicable_reservations(
                db,
                session,
                &adjustment.warehouse_id,
                &line.sku_id,
                &balance.base.id,
                line,
            )
            .await?;
            if !db
                .stock_balances()
                .deduct_available(&balance.base.id, line.quantity, session)
                .await?
            {
                return Err(Error::BusinessLogicError(
                    "可用库存不足，无法过账库存调整".to_string(),
                ));
            }
        }
    }
    // 余额记录最后流水（台账「最后变动」列），与数量增减同事务
    if !db
        .stock_balances()
        .apply_last_movement(&balance.base.id, &movement.base.id, session)
        .await?
    {
        return Err(Error::BusinessLogicError("库存余额行不存在".to_string()));
    }
    Ok(())
}

/// 释放调整仓库/SKU 上的适用预占（盘亏/损坏扣减前）。
///
/// 按预占建立时间顺序整体释放（预占释放只支持全额释放，§6.7），释放总量
/// 不超过本明细扣减数量；释放的同时写预占释放流水并同步余额预占。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `warehouse_id` - 调整仓库
/// * `sku_id` - 调整 SKU
/// * `balance_id` - 库存余额主键（同步释放预占）
/// * `line` - 调整明细（来源单据引用）
///
/// # 返回
/// 无返回值。
///
/// # 错误
/// 释放写入失败时返回错误。
async fn release_applicable_reservations(
    db: &Database,
    session: &mut mongodb::ClientSession,
    warehouse_id: &entities::ids::WarehouseId,
    sku_id: &entities::ids::SkuId,
    balance_id: &str,
    line: &StockAdjustmentLine,
) -> Result<()> {
    let reservations = db
        .inventory()
        .oldest_operable_reservations(warehouse_id, sku_id, session)
        .await?;
    let mut released_total = Quantity::from_str("0").unwrap();
    let target = line.quantity.to_decimal();
    for reservation in reservations {
        if released_total.to_decimal() >= target {
            break;
        }
        let remaining = reservation.reserved_quantity.to_decimal();
        if remaining <= Quantity::from_str("0").unwrap().to_decimal() {
            continue;
        }
        if !db
            .stock_reservations()
            .release_quantity(&reservation.base.id, reservation.reserved_quantity, session)
            .await?
        {
            continue;
        }
        // 同步余额预占（释放量从 reserved 转入 available），否则后续
        // deduct_available 会因可用量不足而误拒盘亏/损坏过账。
        if !db
            .stock_balances()
            .release_reserved(balance_id, reservation.reserved_quantity, session)
            .await?
        {
            return Err(Error::BusinessLogicError(
                "库存余额预占与预占记录不一致，无法过账库存调整".to_string(),
            ));
        }
        db.stock_reservation_entries()
            .create(
                &StockReservationEntry::new(
                    StockReservationEntryId::new(next_id()),
                    StockReservationEntryData {
                        reservation_id: reservation.base.id.clone().into(),
                        entry_type: ReservationEntryType::Release,
                        quantity: reservation.reserved_quantity,
                        source_document_id: line.stock_adjustment_id.to_string(),
                    },
                )?,
                session,
            )
            .await?;
        released_total = Quantity::try_from(released_total.to_decimal() + remaining)
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
    }
    Ok(())
}

/// 批量解析流水来源单据号（按流水类型查对应单据集合，`$in` 批量避免 N+1）。
///
/// 当前支持库存调整单（盘盈/盘亏/损坏）与采购入库单；其余来源类型暂不解析，
/// 由前端回退显示来源主键。
///
/// # 参数
/// * `db` - 数据库实例
/// * `movements` - 流水视图列表
///
/// # 返回
/// 返回「来源单据主键 → 单据号」映射。
///
/// # 错误
/// 查询失败时返回 `RepositoryError`。
async fn load_movement_source_document_nos(
    db: &Database,
    movements: &[StockMovementView],
) -> Result<HashMap<String, String>> {
    let mut document_nos = HashMap::new();
    let adjustment_ids: Vec<String> = movements
        .iter()
        .filter(|movement| {
            matches!(
                movement.movement_type,
                MovementType::StockGain | MovementType::StockLoss | MovementType::Damage
            )
        })
        .map(|movement| movement.source_document_id.clone())
        .collect();
    if !adjustment_ids.is_empty() {
        let adjustments = db
            .inventory()
            .stock_adjustments_by_ids(&adjustment_ids, &mut NoTransaction)
            .await?;
        for adjustment in adjustments {
            document_nos.insert(adjustment.base.id.clone(), adjustment.adjustment_no.clone());
        }
    }
    let receipt_ids: Vec<String> = movements
        .iter()
        .filter(|movement| matches!(movement.movement_type, MovementType::PurchaseReceiptIn))
        .map(|movement| movement.source_document_id.clone())
        .collect();
    if !receipt_ids.is_empty() {
        let receipts = db
            .inventory()
            .purchase_receipts_by_ids(&receipt_ids, &mut NoTransaction)
            .await?;
        for receipt in receipts {
            document_nos.insert(receipt.base.id.clone(), receipt.receipt_no.clone());
        }
    }
    Ok(document_nos)
}

/// 按主键集合批量取回仓库（`$in`，禁止 N+1）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `ids` - 仓库主键集合
///
/// # 返回
/// 返回按主键索引的仓库映射。
///
/// # 错误
/// 查询失败时返回 `RepositoryError`。
async fn load_warehouses_by_ids(
    db: &Database,
    ids: &[String],
) -> Result<HashMap<String, entities::warehouse::Warehouse>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let warehouses = db.inventory().warehouses_by_ids(ids, &mut NoTransaction).await?;
    Ok(warehouses
        .into_iter()
        .map(|warehouse| (warehouse.base.id.clone(), warehouse))
        .collect())
}

/// 按主键集合批量取回 SKU（`$in`，禁止 N+1）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `ids` - SKU 主键集合
///
/// # 返回
/// 返回按主键索引的 SKU 映射。
///
/// # 错误
/// 查询失败时返回 `RepositoryError`。
async fn load_skus_by_ids(db: &Database, ids: &[String]) -> Result<HashMap<String, entities::catalog::Sku>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let skus = db.inventory().skus_by_ids(ids, &mut NoTransaction).await?;
    Ok(skus.into_iter().map(|sku| (sku.base.id.clone(), sku)).collect())
}

/// 按当前修订主键集合批量取回仓库修订（`$in`，禁止 N+1）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `warehouses` - 仓库映射
///
/// # 返回
/// 返回按主键索引的仓库修订映射。
///
/// # 错误
/// 查询失败时返回 `RepositoryError`。
async fn load_warehouse_revisions(
    db: &Database,
    warehouses: &HashMap<String, entities::warehouse::Warehouse>,
) -> Result<HashMap<String, entities::warehouse::WarehouseRevision>> {
    let revision_ids: Vec<String> = warehouses
        .values()
        .filter_map(|warehouse| warehouse.stable.current_revision_id.clone())
        .collect();
    if revision_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let revisions = db
        .inventory()
        .warehouse_revisions_by_ids(&revision_ids, &mut NoTransaction)
        .await?;
    Ok(revisions
        .into_iter()
        .map(|revision| (revision.base.id.clone(), revision))
        .collect())
}

/// 按当前修订主键集合批量取回 SKU 修订（`$in`，禁止 N+1）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `skus` - SKU 映射
///
/// # 返回
/// 返回按主键索引的 SKU 修订映射。
///
/// # 错误
/// 查询失败时返回 `RepositoryError`。
async fn load_sku_revisions(
    db: &Database,
    skus: &HashMap<String, entities::catalog::Sku>,
) -> Result<HashMap<String, entities::catalog::SkuRevision>> {
    let revision_ids: Vec<String> = skus
        .values()
        .filter_map(|sku| sku.stable.current_revision_id.clone())
        .collect();
    if revision_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let revisions = db
        .inventory()
        .sku_revisions_by_ids(&revision_ids, &mut NoTransaction)
        .await?;
    Ok(revisions
        .into_iter()
        .map(|revision| (revision.base.id.clone(), revision))
        .collect())
}

/// 按主键集合批量取回库存流水（`$in`，禁止 N+1）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `ids` - 流水主键集合
///
/// # 返回
/// 返回按主键索引的流水映射。
///
/// # 错误
/// 查询失败时返回 `RepositoryError`。
async fn load_movements_by_ids(db: &Database, ids: &[String]) -> Result<HashMap<String, StockMovement>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let movements = db.inventory().movements_by_ids(ids, &mut NoTransaction).await?;
    Ok(movements
        .into_iter()
        .map(|movement| (movement.base.id.clone(), movement))
        .collect())
}

/// 构建调整明细实体集合（明细主键逐条生成）。
///
/// # 参数
/// * `adjustment_id` - 调整单主键
/// * `reason_type` - 调整原因，用于校验明细方向
/// * `inputs` - 明细输入
///
/// # 返回
/// 返回明细实体集合。
///
/// # 错误
/// 明细数量非正时返回错误（实体构造）。
fn build_adjustment_lines(
    adjustment_id: &StockAdjustmentId,
    reason_type: AdjustmentReasonType,
    inputs: &[StockAdjustmentLineInput],
) -> Result<Vec<StockAdjustmentLine>> {
    let mut lines = Vec::with_capacity(inputs.len());
    for input in inputs {
        lines.push(
            StockAdjustmentLine::new_for_reason(
                StockAdjustmentLineId::new(next_id()),
                reason_type,
                StockAdjustmentLineData {
                    stock_adjustment_id: adjustment_id.clone(),
                    sku_id: input.sku_id.clone(),
                    quantity: input.quantity,
                    direction: input.direction,
                },
            )
            .map_err(Error::Logic)?,
        );
    }
    Ok(lines)
}

fn build_balance_view(
    balance: &StockBalance,
    enrichments: &BalanceEnrichments,
    has_active_reservation: bool,
) -> StockBalanceView {
    let warehouse = enrichments.warehouses.get(&balance.warehouse_id.to_string());
    let warehouse_name = warehouse
        .and_then(|wh| wh.stable.current_revision_id.as_deref())
        .and_then(|revision_id| enrichments.warehouse_revisions.get(revision_id))
        .map(|revision| revision.name.clone());
    let sku = enrichments.skus.get(&balance.sku_id.to_string());
    let sku_revision = sku
        .and_then(|sku| sku.stable.current_revision_id.as_deref())
        .and_then(|revision_id| enrichments.sku_revisions.get(revision_id));
    StockBalanceView {
        id: balance.base.id.clone(),
        warehouse_id: balance.warehouse_id.to_string(),
        warehouse_code: warehouse.map(|wh| wh.warehouse_code.clone()).unwrap_or_default(),
        warehouse_name: warehouse_name.unwrap_or_default(),
        sku_id: balance.sku_id.to_string(),
        sku_code: sku.map(|sku| sku.sku_no.clone()).unwrap_or_default(),
        sku_name: sku_revision
            .map(|revision| revision.name.clone())
            .unwrap_or_default(),
        spec_summary: sku_revision.and_then(|revision| revision.specification.clone()),
        on_hand_quantity: balance.on_hand_quantity,
        reserved_quantity: balance.reserved_quantity,
        available_quantity: balance.available_quantity,
        version: balance.base.version.to_string(),
        last_movement_id: balance.last_movement_id.as_ref().map(ToString::to_string),
        last_movement_at: balance
            .last_movement_id
            .as_ref()
            .and_then(|id| enrichments.movements.get(&id.to_string()))
            .map(|movement| movement.fact.occurred_at.unix_secs()),
        last_movement_type: balance
            .last_movement_id
            .as_ref()
            .and_then(|id| enrichments.movements.get(&id.to_string()))
            .map(|movement| movement.movement_type),
        has_active_reservation,
    }
}

impl From<StockMovement> for StockMovementView {
    /// 从流水实体构造视图。
    fn from(movement: StockMovement) -> Self {
        Self {
            id: movement.base.id,
            warehouse_id: movement.warehouse_id.to_string(),
            sku_id: movement.sku_id.to_string(),
            movement_type: movement.movement_type,
            direction: movement.direction,
            quantity: movement.quantity,
            source_document_id: movement.source_document_id,
            source_document_no: None,
            source_line_id: movement.source_line_id,
            occurred_at: movement.fact.occurred_at.unix_secs(),
            recorded_at: movement.fact.recorded_at.unix_secs(),
            recorded_by: movement.fact.recorded_by.clone(),
        }
    }
}

impl From<StockReservation> for StockReservationView {
    /// 从预占实体构造视图。
    fn from(reservation: StockReservation) -> Self {
        Self {
            id: reservation.base.id,
            warehouse_id: reservation.warehouse_id.to_string(),
            sku_id: reservation.sku_id.to_string(),
            sales_order_line_id: reservation.sales_order_line_id.to_string(),
            reserved_quantity: reservation.reserved_quantity,
            consumed_quantity: reservation.consumed_quantity,
            released_quantity: reservation.released_quantity,
            status: reservation.status,
            version: reservation.base.version,
        }
    }
}

impl From<StockAdjustment> for StockAdjustmentView {
    /// 从调整单实体构造视图。
    fn from(adjustment: StockAdjustment) -> Self {
        Self {
            id: adjustment.base.id,
            adjustment_no: adjustment.adjustment_no,
            warehouse_id: adjustment.warehouse_id.to_string(),
            reason_type: adjustment.reason_type,
            status: adjustment.status,
            prepared_by: adjustment.prepared_by,
            reviewed_by: adjustment.reviewed_by,
            finance_reviewed_by: adjustment.finance_reviewed_by,
            note: adjustment.note,
            occurred_at: adjustment.occurred_at.map(|instant| instant.unix_secs()),
            version: adjustment.base.version.to_string(),
            created_at: adjustment.base.created_at,
        }
    }
}

impl From<StockAdjustmentLine> for StockAdjustmentLineView {
    /// 从调整明细实体构造视图。
    fn from(line: StockAdjustmentLine) -> Self {
        Self {
            id: line.base.id,
            sku_id: line.sku_id.to_string(),
            quantity: line.quantity,
            direction: line.direction,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{build_adjustment_lines, StockAdjustmentLineInput};
    use entities::ids::{SkuId, StockAdjustmentId};
    use entities::inventory::{AdjustmentReasonType, MovementDirection};
    use entities::money::Quantity;
    use std::str::FromStr;

    #[test]
    fn adjustment_lines_are_built_with_entity_validation() {
        let lines = build_adjustment_lines(
            &StockAdjustmentId::new("adj-1"),
            AdjustmentReasonType::StockGain,
            &[StockAdjustmentLineInput {
                sku_id: SkuId::new("sku-1"),
                quantity: Quantity::from_str("2").unwrap(),
                direction: MovementDirection::Increase,
            }],
        )
        .unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].quantity, Quantity::from_str("2").unwrap());

        let invalid_quantity = build_adjustment_lines(
            &StockAdjustmentId::new("adj-2"),
            AdjustmentReasonType::StockGain,
            &[StockAdjustmentLineInput {
                sku_id: SkuId::new("sku-1"),
                quantity: Quantity::from_str("0").unwrap(),
                direction: MovementDirection::Increase,
            }],
        );
        assert!(invalid_quantity.is_err(), "调整数量必须为正数");

        let invalid_direction = build_adjustment_lines(
            &StockAdjustmentId::new("adj-3"),
            AdjustmentReasonType::StockLoss,
            &[StockAdjustmentLineInput {
                sku_id: SkuId::new("sku-1"),
                quantity: Quantity::from_str("1").unwrap(),
                direction: MovementDirection::Increase,
            }],
        );
        assert!(invalid_direction.is_err(), "盘亏明细必须为减少方向");
    }
}
