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

use database::{AccessControlExt, CatalogExt, InventoryExt, NoTransaction, Transactional, WarehouseExt};
use entities::common::source::SourceType;
use entities::common::time::Instant;
use entities::ids::{StockAdjustmentId, StockAdjustmentLineId, StockMovementId, StockReservationEntryId};
use entities::inventory::{
    AdjustmentReasonType, MovementDirection, MovementType, ReservationEntryType, ReservationStatus,
    StockAdjustment, StockAdjustmentData, StockAdjustmentLine, StockAdjustmentLineData, StockAdjustmentState,
    StockAdjustmentUpdate, StockBalance, StockMovement, StockMovementData, StockReservation,
    StockReservationEntry, StockReservationEntryData,
};
use entities::money::Quantity;
use id_generator::next_id;
use mongodb::bson::doc;
use mongodb::Database;
use std::str::FromStr;
use validator::Validate;

use crate::approval::binding::{
    attach_published_binding, bind_published_definition_on_document_create, BindPublishedDefinitionCommand,
};
use crate::approval::business_adapter::BindingRevalidationContext;
use crate::audit::AuditActor;
use crate::document_registry::{find_approval_binding, new_registered_document, persist_registered_document};
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;
use entities::document_registry::{BusinessDocument, DocumentType};

use self::adapter::{
    build_stock_adjustment_snapshot, document_approval_view, ensure_final_approve_posting,
    execute_stock_adjustment_domain_action, require_frozen_binding, start_approval_command_kind,
    stock_adjustment_adapter, stock_adjustment_start_command, stock_adjustment_subject_ref,
    RECENT_HISTORY_LIMIT,
};
use self::dto::SortDir;
pub use self::dto::{
    CancelStockAdjustmentApprovalRequest, CreateStockAdjustmentRequest, DocumentApprovalView, PageView,
    StockAdjustmentDetailView, StockAdjustmentLineInput, StockAdjustmentLineView, StockAdjustmentListParams,
    StockAdjustmentView, StockBalanceDetailView, StockBalanceListParams, StockBalanceView,
    StockMovementListParams, StockMovementView, StockReservationListParams, StockReservationView,
    SubmitStockAdjustmentRequest, UpdateStockAdjustmentRequest,
};

mod adapter;
mod dto;

pub use adapter::stock_adjustment_object_readable;

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
/// 提供余额/流水/预占/调整单的查询，以及调整单创建绑定、提交启动审批、
/// 最终通过过账与撤回编排。
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
                    version: row.version,
                    last_movement_id: row.last_movement_id.as_ref().map(ToString::to_string),
                    last_movement_at: row
                        .last_movement_id
                        .as_ref()
                        .and_then(|id| enrichments.movements.get(&id.to_string()))
                        .map(|movement| movement.fact.occurred_at.unix_secs()),
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
    pub async fn stock_balance_detail(&self, id: &str) -> Result<StockBalanceDetailView> {
        let balance = self
            .db
            .stock_balances()
            .find_by_id(id, &mut NoTransaction)
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
        let active = [
            ReservationStatus::Active.as_str(),
            ReservationStatus::PartiallyConsumed.as_str(),
        ];
        let reservations = self
            .db
            .stock_reservations()
            .find_many(
                doc! {
                    "warehouse_id": balance.warehouse_id.to_string(),
                    "sku_id": balance.sku_id.to_string(),
                    "status": { "$in": active.as_slice() },
                },
                &mut NoTransaction,
            )
            .await?;
        let pending = self
            .db
            .stock_adjustments()
            .find_many(
                doc! {
                    "warehouse_id": balance.warehouse_id.to_string(),
                    "status": {
                        "$in": vec![
                            StockAdjustmentState::Draft.as_str(),
                            StockAdjustmentState::InApproval.as_str(),
                        ],
                    },
                },
                &mut NoTransaction,
            )
            .await?;
        let enrichments = self
            .load_enrichments(
                &[balance.warehouse_id.to_string()],
                &[balance.sku_id.to_string()],
                &[],
            )
            .await?;
        let balance_view = build_balance_view(&balance, &enrichments, !reservations.is_empty());
        Ok(StockBalanceDetailView {
            balance: balance_view,
            recent_movements: movements
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
                    source_line_id: row.source_line_id,
                    occurred_at: row.occurred_at.unix_secs(),
                    recorded_at: row.recorded_at.unix_secs(),
                })
                .collect(),
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
        let items = page
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
                source_line_id: row.source_line_id,
                occurred_at: row.occurred_at.unix_secs(),
                recorded_at: row.recorded_at.unix_secs(),
            })
            .collect();
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
                version: row.version,
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
    ///
    /// # 返回
    /// 返回调整单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 调整单不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn stock_adjustment_detail(&self, id: &str) -> Result<StockAdjustmentDetailView> {
        let adjustment = self
            .db
            .stock_adjustments()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))?;
        let lines = self
            .db
            .inventory()
            .adjustment_lines_by_adjustment_ids(&[adjustment.base.id.clone().into()], &mut NoTransaction)
            .await?;
        let movements = self
            .db
            .stock_movements()
            .find_by_source_document(id, &mut NoTransaction)
            .await?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction)
            .await
            .ok()
            .flatten();
        Ok(StockAdjustmentDetailView {
            approval: document_approval_view(binding.as_ref(), None, adjustment.status),
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
    /// 返回新建调整单的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - 调整单号重复或流程未配置
    /// * `RepositoryError` - 数据库写入失败
    pub async fn create_stock_adjustment(
        &self,
        req: CreateStockAdjustmentRequest,
        actor: &AuditActor,
    ) -> Result<StockAdjustmentView> {
        req.validate()?;
        let id = StockAdjustmentId::new(next_id());
        let adjustment = StockAdjustment::new(
            id.clone(),
            StockAdjustmentData {
                adjustment_no: req.adjustment_no,
                warehouse_id: req.warehouse_id,
                reason_type: req.reason_type,
                prepared_by: actor.id().to_string(),
            },
        )?;
        let lines = build_adjustment_lines(&id, &req.lines)?;
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
            adjustment.clone(),
            lines,
            document,
            bind_command,
            audit,
            actor.clone(),
        )
        .await?;
        Ok(adjustment.into())
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
    pub async fn update_stock_adjustment(
        &self,
        id: &str,
        req: UpdateStockAdjustmentRequest,
        actor: &AuditActor,
    ) -> Result<StockAdjustmentView> {
        req.validate()?;
        let mut adjustment = self
            .db
            .stock_adjustments()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))?;
        if adjustment.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        adjustment.update(StockAdjustmentUpdate {
            reason_type: req.reason_type,
            reviewed_by: None,
            finance_reviewed_by: None,
        })?;
        let audit =
            actor
                .clone()
                .resource_log("stock_adjustment.update", "stock_adjustment", id.to_string())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
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
    /// * `req` - 提交请求（版本与幂等键）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回提交后的调整单视图。
    ///
    /// # 错误
    /// * `NotFound` - 调整单不存在
    /// * `ConflictError` - 非草稿、无绑定或并发冲突
    pub async fn submit_stock_adjustment(
        &self,
        id: &str,
        req: SubmitStockAdjustmentRequest,
        actor: &AuditActor,
    ) -> Result<StockAdjustmentView> {
        req.validate()?;
        let adapter = stock_adjustment_adapter()?;
        let _ = stock_adjustment_subject_ref(id)?;
        let mut adjustment = self.load_stock_adjustment(id).await?;
        ensure_expected_version(adjustment.base.version, req.expected_version)?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
        require_frozen_binding(binding.as_ref())?;
        let lines = self
            .db
            .inventory()
            .adjustment_lines_by_adjustment_ids(&[StockAdjustmentId::new(id.to_string())], &mut NoTransaction)
            .await?;
        execute_stock_adjustment_domain_action(&mut adjustment, adapter.on_approval_start)?;
        let snapshot = build_stock_adjustment_snapshot(&adjustment, &lines, actor.id(), Instant::now())?;
        let start = stock_adjustment_start_command(
            id,
            adjustment.approval_subject_version,
            actor.id(),
            &req.idempotency_key,
        );
        let _ = (
            start_approval_command_kind(&start),
            snapshot,
            binding,
            RECENT_HISTORY_LIMIT,
        );
        persist_adjustment_transition(&self.db, adjustment, actor, "stock_adjustment.submit", id).await
    }

    /// 撤回库存调整审批，成功后回到草稿且 `subject_version` 不回退。
    ///
    /// 作为合同 `cancel_action`，供业务撤回与管理员受阻取消共用。
    ///
    /// # 参数
    /// * `id` - 调整单主键
    /// * `req` - 撤回请求（原因必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回撤回后的调整单视图。
    ///
    /// # 错误
    /// * `NotFound` - 调整单不存在
    /// * `ConflictError` - 非审批中或并发冲突
    pub async fn cancel_stock_adjustment_approval(
        &self,
        id: &str,
        req: CancelStockAdjustmentApprovalRequest,
        actor: &AuditActor,
    ) -> Result<StockAdjustmentView> {
        req.validate()?;
        let mut adjustment = self.load_stock_adjustment(id).await?;
        ensure_expected_version(adjustment.base.version, req.expected_version)?;
        let adapter = stock_adjustment_adapter()?;
        execute_stock_adjustment_domain_action(&mut adjustment, adapter.cancel_action)?;
        persist_adjustment_transition(
            &self.db,
            adjustment,
            actor,
            "stock_adjustment.cancel_approval",
            id,
        )
        .await
    }

    /// 最终通过过账（`IN_APPROVAL` → `POSTED`；§8.2 第 3 条跨集合事务）。
    ///
    /// 仅由合同 §4.4.4 `on_final_approve` 调用，不得再作为人工中间旁路。
    ///
    /// # 参数
    /// * `id` - 调整单主键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后的调整单视图。
    ///
    /// # 错误
    /// * `NotFound` - 调整单不存在
    /// * `ConflictError` - 状态不允许过账、余额不足或重复过账
    /// * `OutcomeUnknown` - 提交结果无法确认
    pub async fn post_stock_adjustment(&self, id: &str, actor: &AuditActor) -> Result<StockAdjustmentView> {
        let adjustment_id = StockAdjustmentId::new(id.to_string());
        let actor = actor.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let posted = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut adjustment = db
                        .stock_adjustments()
                        .find_by_id(adjustment_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))?;
                    ensure_final_approve_posting(&adjustment)?;
                    let lines = db
                        .inventory()
                        .adjustment_lines_by_adjustment_ids(std::slice::from_ref(&adjustment_id), session)
                        .await?;
                    if lines.is_empty() {
                        return Err(Error::ValidationError("库存调整单没有明细，无法过账".to_string()));
                    }
                    let occurred_at = Instant::now();
                    for line in &lines {
                        ensure_line_reason_coherent(&adjustment.reason_type, &line.direction)?;
                        post_adjustment_line(&db, session, &adjustment, line, &occurred_at, &actor).await?;
                    }
                    adjustment.mark_posted()?;
                    db.stock_adjustments().update(&mut adjustment, session).await?;
                    let audit = actor.resource_log(
                        "stock_adjustment.post",
                        "stock_adjustment",
                        adjustment_id.to_string(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<StockAdjustment, crate::errors::Error>(adjustment)
                })
            })
            .await?;
        Ok(posted.into())
    }

    /// 按主键读取库存调整单。
    ///
    /// # 错误
    /// 不存在时返回 `NotFound`。
    async fn load_stock_adjustment(&self, id: &str) -> Result<StockAdjustment> {
        self.db
            .stock_adjustments()
            .find_by_id(id, &mut NoTransaction)
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
        let warehouses = find_warehouses_by_ids(&self.db, warehouse_ids).await?;
        let skus = find_skus_by_ids(&self.db, sku_ids).await?;
        let warehouse_revisions = load_warehouse_revisions(&self.db, &warehouses).await?;
        let sku_revisions = load_sku_revisions(&self.db, &skus).await?;
        let movements = find_movements_by_ids(&self.db, movement_ids).await?;
        Ok(BalanceEnrichments {
            warehouses,
            warehouse_revisions,
            skus,
            sku_revisions,
            movements,
        })
    }
}

/// 校验乐观锁版本。
///
/// # 错误
/// 不一致时返回冲突。
fn ensure_expected_version(actual: u64, expected: u64) -> Result<()> {
    if actual == expected {
        return Ok(());
    }
    Err(Error::ConflictError(
        "数据已被其他请求修改，请刷新后重试".to_string(),
    ))
}

/// 在创建事务内写入调整单、绑定发布定义并登记单据。
///
/// 绑定失败必须回滚业务实体，不得留下以后补流程的单据。
///
/// # 错误
/// 无发布定义、人员重验失败或写入失败时返回错误。
#[allow(clippy::too_many_arguments)]
async fn persist_created_adjustment(
    db: &Database,
    rbac: &SharedRbacService,
    adjustment: StockAdjustment,
    lines: Vec<StockAdjustmentLine>,
    mut document: BusinessDocument,
    bind_command: BindPublishedDefinitionCommand,
    audit: entities::AuditLog,
    actor: AuditActor,
) -> Result<()> {
    let db = db.clone();
    let rbac = rbac.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
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
    let _ = stock_adjustment_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, session).await?;
    let binding = binding.ok_or_else(|| Error::Internal("库存调整单必须绑定已发布定义".to_string()))?;
    attach_published_binding(document, binding)?;
    persist_registered_document(db, document, session).await
}

/// 在事务内持久化调整单状态迁移与审计。
///
/// # 错误
/// 写入失败时返回错误。
async fn persist_adjustment_transition(
    db: &Database,
    mut adjustment: StockAdjustment,
    actor: &AuditActor,
    action: &'static str,
    id: &str,
) -> Result<StockAdjustmentView> {
    let audit = actor
        .clone()
        .resource_log(action, "stock_adjustment", id.to_string())?;
    let db = db.clone();
    let client = db.client().clone();
    let updated = client
        .with_transaction(move |session| {
            Box::pin(async move {
                db.stock_adjustments().update(&mut adjustment, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<StockAdjustment, crate::errors::Error>(adjustment)
            })
        })
        .await?;
    Ok(updated.into())
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
    let active = [
        ReservationStatus::Active.as_str(),
        ReservationStatus::PartiallyConsumed.as_str(),
    ];
    let reservations = db
        .stock_reservations()
        .find_many(
            doc! {
                "warehouse_id": { "$in": warehouse_ids },
                "sku_id": { "$in": sku_ids },
                "status": { "$in": active.as_slice() },
            },
            &mut NoTransaction,
        )
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

/// 校验调整明细方向与原因类型一致（§6.7：盘盈必增、盘亏/损坏必减）。
///
/// # 参数
/// * `reason_type` - 调整原因类型
/// * `direction` - 明细方向
///
/// # 返回
/// 一致返回 `Ok(())`。
///
/// # 错误
/// 方向与原因类型矛盾时返回 `ValidationError`。
fn ensure_line_reason_coherent(
    reason_type: &AdjustmentReasonType,
    direction: &MovementDirection,
) -> Result<()> {
    let expected = match reason_type {
        AdjustmentReasonType::StockGain => MovementDirection::Increase,
        AdjustmentReasonType::StockLoss | AdjustmentReasonType::Damage => MovementDirection::Decrease,
    };
    if direction != &expected {
        return Err(Error::ValidationError(format!(
            "调整原因 {} 的明细方向必须为 {}",
            reason_type.label(),
            expected.as_str()
        )));
    }
    Ok(())
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
    let movement_type = match adjustment.reason_type {
        AdjustmentReasonType::StockGain => MovementType::StockGain,
        AdjustmentReasonType::StockLoss => MovementType::StockLoss,
        AdjustmentReasonType::Damage => MovementType::Damage,
    };
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
            occurred_at: *occurred_at,
            recorded_at: *occurred_at,
            recorded_by: actor.id().to_string(),
            source_type: SourceType::Erp,
            source_reference: None,
            reason_code: Some(adjustment.reason_type.as_str().to_string()),
            reason_text: None,
        },
    )?;
    db.stock_movements().create(&movement, session).await?;

    let balance = db
        .stock_balances()
        .find_by_dimensions(&adjustment.warehouse_id, &line.sku_id, session)
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
            release_applicable_reservations(db, session, &adjustment.warehouse_id, &line.sku_id, line)
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
    line: &StockAdjustmentLine,
) -> Result<()> {
    let active = [
        ReservationStatus::Active.as_str(),
        ReservationStatus::PartiallyConsumed.as_str(),
    ];
    let reservations = db
        .stock_reservations()
        .find_many_sorted(
            doc! {
                "warehouse_id": warehouse_id.to_string(),
                "sku_id": sku_id.to_string(),
                "status": { "$in": active.as_slice() },
            },
            doc! { "created_at": 1 },
            session,
        )
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
async fn find_warehouses_by_ids(
    db: &Database,
    ids: &[String],
) -> Result<HashMap<String, entities::warehouse::Warehouse>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let warehouses = db
        .warehouses()
        .find_many(doc! { "id": { "$in": ids } }, &mut NoTransaction)
        .await?;
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
async fn find_skus_by_ids(db: &Database, ids: &[String]) -> Result<HashMap<String, entities::catalog::Sku>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let skus = db
        .skus()
        .find_many(doc! { "id": { "$in": ids } }, &mut NoTransaction)
        .await?;
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
        .warehouse_revisions()
        .find_many(doc! { "id": { "$in": revision_ids } }, &mut NoTransaction)
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
        .sku_revisions()
        .find_many(doc! { "id": { "$in": revision_ids } }, &mut NoTransaction)
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
async fn find_movements_by_ids(db: &Database, ids: &[String]) -> Result<HashMap<String, StockMovement>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let movements = db
        .stock_movements()
        .find_many(doc! { "id": { "$in": ids } }, &mut NoTransaction)
        .await?;
    Ok(movements
        .into_iter()
        .map(|movement| (movement.base.id.clone(), movement))
        .collect())
}

/// 构建调整明细实体集合（明细主键逐条生成）。
///
/// # 参数
/// * `adjustment_id` - 调整单主键
/// * `inputs` - 明细输入
///
/// # 返回
/// 返回明细实体集合。
///
/// # 错误
/// 明细数量非正时返回错误（实体构造）。
fn build_adjustment_lines(
    adjustment_id: &StockAdjustmentId,
    inputs: &[StockAdjustmentLineInput],
) -> Result<Vec<StockAdjustmentLine>> {
    let mut lines = Vec::with_capacity(inputs.len());
    for input in inputs {
        lines.push(
            StockAdjustmentLine::new(
                StockAdjustmentLineId::new(next_id()),
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
        version: balance.base.version,
        last_movement_id: balance.last_movement_id.as_ref().map(ToString::to_string),
        last_movement_at: balance
            .last_movement_id
            .as_ref()
            .and_then(|id| enrichments.movements.get(&id.to_string()))
            .map(|movement| movement.fact.occurred_at.unix_secs()),
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
            source_line_id: movement.source_line_id,
            occurred_at: movement.fact.occurred_at.unix_secs(),
            recorded_at: movement.fact.recorded_at.unix_secs(),
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
            version: adjustment.base.version,
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
    use super::{build_adjustment_lines, ensure_line_reason_coherent, StockAdjustmentLineInput};
    use entities::ids::{SkuId, StockAdjustmentId};
    use entities::inventory::{AdjustmentReasonType, MovementDirection};
    use entities::money::Quantity;
    use std::str::FromStr;

    #[test]
    fn reason_coherence_accepts_matching_direction() {
        assert!(
            ensure_line_reason_coherent(&AdjustmentReasonType::StockGain, &MovementDirection::Increase)
                .is_ok()
        );
        assert!(
            ensure_line_reason_coherent(&AdjustmentReasonType::StockLoss, &MovementDirection::Decrease)
                .is_ok()
        );
        assert!(
            ensure_line_reason_coherent(&AdjustmentReasonType::Damage, &MovementDirection::Decrease).is_ok()
        );
    }

    #[test]
    fn reason_coherence_rejects_contradictory_direction() {
        assert!(
            ensure_line_reason_coherent(&AdjustmentReasonType::StockGain, &MovementDirection::Decrease)
                .is_err()
        );
        assert!(
            ensure_line_reason_coherent(&AdjustmentReasonType::Damage, &MovementDirection::Increase).is_err()
        );
    }

    #[test]
    fn adjustment_lines_are_built_with_entity_validation() {
        let lines = build_adjustment_lines(
            &StockAdjustmentId::new("adj-1"),
            &[StockAdjustmentLineInput {
                sku_id: SkuId::new("sku-1"),
                quantity: Quantity::from_str("2").unwrap(),
                direction: MovementDirection::Increase,
            }],
        )
        .unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].quantity, Quantity::from_str("2").unwrap());

        let negative = build_adjustment_lines(
            &StockAdjustmentId::new("adj-2"),
            &[StockAdjustmentLineInput {
                sku_id: SkuId::new("sku-1"),
                quantity: Quantity::from_str("0").unwrap(),
                direction: MovementDirection::Increase,
            }],
        );
        assert!(negative.is_err(), "调整数量必须为正数");
    }
}
