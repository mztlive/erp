//! 域 D16 `fulfillment` 服务编排（页面：W06 客户验收、W09 收货与发货/交付与代发）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 单集合无跨步骤原子性要求的 CRUD 传入 `&mut NoTransaction`；
//! - 表头+行创建、状态迁移+审计、过账（§8.2 第 1/2/5 条跨集合原子性）使用
//!   `database::Transactional::with_transaction`。
//!
//! 跨域协作（P3-service-api §2：只调对方 Repository，不依赖对方 Service）：
//! - D15 `purchase_order`：采购单/生效版本/版本行/采购销售分配；§8.1.5
//!   `PREPAY` 门槛按 B-G7（P2 依赖）经 D19 `payable` Repository 重算
//!   有效已过账付款净核销金额；
//! - D13 `sales_order`：销售版本行（预占归属与验收工作台）；
//! - D17 `inventory`：余额/流水/预占（入库过账与仓发过账）。
//!
//! 过账去重：状态守卫（仅草稿可过账/确认）+ `stock_movement` 的
//! `(source_document_id, source_line_id, movement_type)` 唯一索引 + 验收
//! `Draft → Posted` 状态机三重防护，重复过账返回 409，不产生第二条正式事实。

use std::collections::HashMap;

use database::{
    AccessControlExt, FulfillmentExt, InventoryExt, NoTransaction, PayableExt, PurchaseOrderExt,
    SalesOrderExt, Transactional,
};
use entities::common::source::SourceType;
use entities::common::time::Instant;
use entities::fulfillment::{
    AcceptanceFulfillmentAllocation, AcceptanceFulfillmentAllocationData, AcceptanceResult, AllocationAction,
    CustomerAcceptance, CustomerAcceptanceData, CustomerAcceptanceLine, CustomerAcceptanceLineData,
    CustomerAcceptanceState, Delivery, DeliveryData, DeliveryLine, DeliveryLineData, DeliveryState,
    DeliveryType, ElectronicDelivery, ElectronicDeliveryData, ElectronicDeliveryState, FulfillmentFactType,
    PurchaseReceipt, PurchaseReceiptData, PurchaseReceiptLine, PurchaseReceiptLineData, PurchaseReceiptState,
    QualityResult, ServiceFulfillment, ServiceFulfillmentData, ServiceFulfillmentState,
};
use entities::ids::{
    AcceptanceFulfillmentAllocationId, CustomerAcceptanceId, CustomerAcceptanceLineId, DeliveryId,
    DeliveryLineId, ElectronicDeliveryId, PayableAccountId, PayableEntryId, PurchaseLineSalesAllocationId,
    PurchaseOrderId, PurchaseReceiptId, PurchaseReceiptLineId, SalesOrderId, SalesOrderRevisionLineId,
    ServiceFulfillmentId, StockBalanceId, StockMovementId, StockReservationEntryId, StockReservationId,
};
use entities::inventory::{
    MovementDirection, MovementType, ReservationEntryType, ReservationStatus, StockBalance, StockBalanceData,
    StockMovement, StockMovementData, StockReservation, StockReservationData, StockReservationEntry,
    StockReservationEntryData,
};
use entities::money::{round_to_cent, Amount, Quantity};
use entities::payable::AllocationAction as PayableAllocationAction;
use entities::purchase_order::{ProgressStatus, PurchaseOrder, PurchaseOrderRevision, PurchaseOrderStatus};
use id_generator::next_id;
use mongodb::bson::doc;
use mongodb::Database;
use std::str::FromStr;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use self::dto::SortDir;
pub use self::dto::{
    AcceptanceAllocationInput, AcceptanceAllocationView, AcceptanceEligibilityView, AcceptanceLineInput,
    AcceptanceSalesLineGroupView, CreateCustomerAcceptanceRequest, CreateDeliveryRequest,
    CreateElectronicDeliveryRequest, CreatePurchaseReceiptRequest, CreateServiceFulfillmentRequest,
    CustomerAcceptanceDetailView, CustomerAcceptanceLineView, CustomerAcceptanceListParams,
    CustomerAcceptanceView, DeliveryDetailView, DeliveryLineInput, DeliveryLineView, DeliveryListParams,
    DeliveryView, ElectronicDeliveryListParams, ElectronicDeliveryView, EligibleFulfillmentFactView,
    PageView, PostAcceptanceLineInput, PostCustomerAcceptanceRequest, PurchaseReceiptDetailView,
    PurchaseReceiptLineInput, PurchaseReceiptLineView, PurchaseReceiptListParams, PurchaseReceiptView,
    ReverseCustomerAcceptanceRequest, ServiceFulfillmentListParams, ServiceFulfillmentView,
    UpdateDeliveryRequest, UpdatePurchaseReceiptRequest,
};

mod dto;

/// 采购入库单列表筛选条件类型（经 `FulfillmentExt` 关联类型跨 crate 可达）。
type PurchaseReceiptFilter = <mongodb::Database as FulfillmentExt>::PurchaseReceiptFilter;
/// 发货单列表筛选条件类型。
type DeliveryFilter = <mongodb::Database as FulfillmentExt>::DeliveryFilter;
/// 电子交付记录列表筛选条件类型。
type ElectronicDeliveryFilter = <mongodb::Database as FulfillmentExt>::ElectronicDeliveryFilter;
/// 线下服务履约记录列表筛选条件类型。
type ServiceFulfillmentFilter = <mongodb::Database as FulfillmentExt>::ServiceFulfillmentFilter;
/// 客户验收单列表筛选条件类型。
type CustomerAcceptanceFilter = <mongodb::Database as FulfillmentExt>::CustomerAcceptanceFilter;

/// 履约服务。
///
/// 提供采购入库、发货（仓发/直发）、电子交付、服务履约、客户验收的
/// 查询与过账编排；`fingerprint_key` 用于对履约对象快照计算查询指纹
/// （§4.5.5，带密钥 HMAC，密钥不持久化）。
pub struct FulfillmentService {
    db: Database,
    fingerprint_key: Vec<u8>,
}

impl FulfillmentService {
    /// 创建履约服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `fingerprint_key` - 履约对象快照查询指纹密钥（取 `app.secret` 字节）
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database, fingerprint_key: Vec<u8>) -> Self {
        Self { db, fingerprint_key }
    }

    // ------------------------------------------------------------ purchase_receipt

    /// 分页查询采购入库单列表（W09 入库视图）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`purchase_order_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn purchase_receipt_list(
        &self,
        params: &PurchaseReceiptListParams,
    ) -> Result<PageView<PurchaseReceiptView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = PurchaseReceiptFilter {
            purchase_order_id: query.purchase_order_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .purchase_receipts()
            .search_purchase_receipts(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| PurchaseReceiptView {
                id: row.id,
                receipt_no: row.receipt_no,
                purchase_order_id: row.purchase_order_id.to_string(),
                warehouse_id: row.warehouse_id.to_string(),
                status: row.status,
                posted_at: row.posted_at.map(|instant| instant.unix_secs()),
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

    /// 查询采购入库单详情（表头 + 行）。
    ///
    /// # 参数
    /// * `id` - 入库单主键
    ///
    /// # 返回
    /// 返回入库单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 入库单不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn purchase_receipt_detail(&self, id: &str) -> Result<PurchaseReceiptDetailView> {
        let receipt = self
            .db
            .purchase_receipts()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购入库单不存在".to_string()))?;
        let lines = self
            .db
            .fulfillment()
            .receipt_lines_by_receipt_ids(&[receipt.base.id.clone().into()], &mut NoTransaction)
            .await?;
        Ok(PurchaseReceiptDetailView {
            receipt: receipt.into(),
            lines: lines.into_iter().map(Into::into).collect(),
        })
    }

    /// 创建采购入库单（草稿，跨集合：表头 + 行 + 审计）。
    ///
    /// 行的质量结果由服务端按合格/到货关系派生（全部合格/全部不合格/部分合格）。
    ///
    /// # 参数
    /// * `req` - 创建请求（表头 + 行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建入库单的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - 单号重复（唯一索引透出）
    /// * `RepositoryError` - 数据库写入失败
    pub async fn create_purchase_receipt(
        &self,
        req: CreatePurchaseReceiptRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseReceiptView> {
        req.validate()?;
        let id = PurchaseReceiptId::new(next_id());
        let receipt = PurchaseReceipt::new(
            id.clone(),
            PurchaseReceiptData {
                receipt_no: req.receipt_no,
                purchase_order_id: req.purchase_order_id,
                warehouse_id: req.warehouse_id,
            },
        )?;
        let lines = build_receipt_lines(&id, &req.lines)?;
        let audit =
            actor
                .clone()
                .resource_log("purchase_receipt.create", "purchase_receipt", id.to_string())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let receipt_for_tx = receipt.clone();
        let lines_for_tx = lines.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.fulfillment()
                        .create_purchase_receipt_with_lines(&receipt_for_tx, &lines_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(receipt.into())
    }

    /// 更新采购入库单（仅草稿；乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 入库单主键
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后入库单的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 入库单不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    /// * `ValidationError` - 请求体校验失败
    pub async fn update_purchase_receipt(
        &self,
        id: &str,
        req: UpdatePurchaseReceiptRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseReceiptView> {
        req.validate()?;
        let mut receipt = self
            .db
            .purchase_receipts()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购入库单不存在".to_string()))?;
        if receipt.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        receipt.update(entities::fulfillment::PurchaseReceiptUpdate {
            warehouse_id: req.warehouse_id.or(Some(receipt.warehouse_id.clone())),
        })?;
        let audit =
            actor
                .clone()
                .resource_log("purchase_receipt.update", "purchase_receipt", id.to_string())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.purchase_receipts().update(&mut receipt, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<PurchaseReceipt, crate::errors::Error>(receipt)
                })
            })
            .await?;
        Ok(updated.into())
    }

    /// 过账采购入库（草稿 → 已过账；§8.2 第 1 条跨集合事务）。
    ///
    /// 在同一事务内：校验采购单可履约与 `PREPAY` 门槛（§8.1.5）、校验累计
    /// 有效收货不超当前有效采购数量、写入库行对应的库存增加流水、更新/创建
    /// 库存余额、沿采购销售分配自动建立销售预占（含预占流水）、推进采购履约
    /// 进度、迁移入库单状态、写审计。重复过账由状态守卫（仅草稿）与流水唯一
    /// 索引双重防护。
    ///
    /// # 参数
    /// * `id` - 入库单主键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后的入库单视图。
    ///
    /// # 错误
    /// * `NotFound` - 入库单/采购单/生效版本不存在
    /// * `ConflictError` - 状态不允许过账或重复过账
    /// * `BusinessLogicError` - 门槛未满足、超收或采购单不可履约
    /// * `OutcomeUnknown` - 提交结果无法确认
    pub async fn post_purchase_receipt(&self, id: &str, actor: &AuditActor) -> Result<PurchaseReceiptView> {
        let receipt_id = PurchaseReceiptId::new(id.to_string());
        let actor = actor.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let posted = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut receipt = db
                        .purchase_receipts()
                        .find_by_id(receipt_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("采购入库单不存在".to_string()))?;
                    if receipt.status != PurchaseReceiptState::Draft {
                        return Err(Error::ConflictError(
                            "只有草稿状态的采购入库单可以过账".to_string(),
                        ));
                    }
                    let lines = db
                        .fulfillment()
                        .receipt_lines_by_receipt_ids(std::slice::from_ref(&receipt_id), session)
                        .await?;
                    if lines.is_empty() {
                        return Err(Error::ValidationError("采购入库单没有行，无法过账".to_string()));
                    }
                    let mut po = db
                        .purchase_orders()
                        .find_by_id(receipt.purchase_order_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("来源采购单不存在".to_string()))?;
                    ensure_po_fulfillable(&po)?;
                    ensure_prepay_gate(&db, session, &po).await?;
                    let revision = load_po_current_revision(&db, session, &po).await?;
                    let revision_lines = db
                        .purchase_order_revision_lines()
                        .find_lines_by_revision_ids(&[revision.base.id.clone().into()], session)
                        .await?;
                    let mut received =
                        cumulative_received_quantities(&db, session, &receipt.purchase_order_id).await?;
                    let occurred_at = Instant::now();
                    for line in &lines {
                        ensure_receipt_within_revision(line, &revision_lines, &received)?;
                        post_receipt_line(
                            &db,
                            session,
                            &receipt,
                            line,
                            &revision_lines,
                            &occurred_at,
                            &actor,
                        )
                        .await?;
                        received
                            .entry(line.purchase_order_revision_line_id.to_string())
                            .and_modify(|total| {
                                *total = Quantity::try_from(
                                    total.to_decimal() + line.qualified_quantity.to_decimal(),
                                )
                                .expect("Quantity 加法和不超过精度上限")
                            })
                            .or_insert(line.qualified_quantity);
                    }
                    receipt.mark_posted(occurred_at, actor.id().to_string())?;
                    db.purchase_receipts().update(&mut receipt, session).await?;
                    let progress = compute_po_fulfillment_progress(&revision_lines, &received);
                    po.set_fulfillment_progress(progress, actor.id().to_string());
                    db.purchase_orders().update(&mut po, session).await?;
                    let audit = actor.resource_log(
                        "purchase_receipt.post",
                        "purchase_receipt",
                        receipt_id.to_string(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<PurchaseReceipt, crate::errors::Error>(receipt)
                })
            })
            .await?;
        Ok(posted.into())
    }

    // ------------------------------------------------------------------- delivery

    /// 分页查询发货单列表（W09 发货视图）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`sales_order_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn delivery_list(&self, params: &DeliveryListParams) -> Result<PageView<DeliveryView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = DeliveryFilter {
            sales_order_id: query.sales_order_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .deliveries()
            .search_deliveries(&filter, &mut NoTransaction)
            .await?;
        let direct_ids: Vec<String> = page
            .items
            .iter()
            .filter(|row| row.delivery_type == DeliveryType::SupplierDirect)
            .map(|row| row.id.clone())
            .collect();
        let direct_po_ids = load_direct_po_ids(&self.db, &direct_ids).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| DeliveryView {
                id: row.id.clone(),
                delivery_no: row.delivery_no,
                delivery_type: row.delivery_type,
                sales_order_id: row.sales_order_id.to_string(),
                purchase_order_id: direct_po_ids.get(&row.id).cloned().flatten(),
                warehouse_id: row.warehouse_id.map(|id| id.to_string()),
                status: row.status,
                carrier: row.carrier,
                tracking_no: row.tracking_no,
                shipped_at: row.shipped_at.map(|instant| instant.unix_secs()),
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

    /// 查询发货单详情（表头 + 行）。
    ///
    /// # 参数
    /// * `id` - 发货单主键
    ///
    /// # 返回
    /// 返回发货单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 发货单不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn delivery_detail(&self, id: &str) -> Result<DeliveryDetailView> {
        let delivery = self
            .db
            .deliveries()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("发货单不存在".to_string()))?;
        let lines = self
            .db
            .fulfillment()
            .delivery_lines_by_delivery_ids(&[delivery.base.id.clone().into()], &mut NoTransaction)
            .await?;
        Ok(DeliveryDetailView {
            delivery: delivery.clone().into(),
            lines: lines.into_iter().map(Into::into).collect(),
        })
    }

    /// 创建发货单（草稿，跨集合：表头 + 行 + 审计）。
    ///
    /// 仓发/直发的表头与行归属由实体按发货类型校验；预占与采购分配的存在性
    /// 在过账时校验。
    ///
    /// # 参数
    /// * `req` - 创建请求（表头 + 行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建发货单的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - 单号重复（唯一索引透出）
    /// * `RepositoryError` - 数据库写入失败
    pub async fn create_delivery(
        &self,
        req: CreateDeliveryRequest,
        actor: &AuditActor,
    ) -> Result<DeliveryView> {
        req.validate()?;
        let id = DeliveryId::new(next_id());
        let delivery = Delivery::new(
            id.clone(),
            DeliveryData {
                delivery_no: req.delivery_no,
                delivery_type: req.delivery_type,
                sales_order_id: req.sales_order_id,
                purchase_order_id: req.purchase_order_id,
                warehouse_id: req.warehouse_id,
                carrier: req.carrier,
                tracking_no: req.tracking_no,
                address_snapshot_encrypted: None,
                address_snapshot_fingerprint: None,
            },
        )?;
        let lines = build_delivery_lines(&id, delivery.delivery_type, &req.lines)?;
        let audit = actor
            .clone()
            .resource_log("delivery.create", "delivery", id.to_string())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let delivery_for_tx = delivery.clone();
        let lines_for_tx = lines.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.fulfillment()
                        .create_delivery_with_lines(&delivery_for_tx, &lines_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(delivery.into())
    }

    /// 更新发货单（仅草稿；乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 发货单主键
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后发货单的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 发货单不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn update_delivery(
        &self,
        id: &str,
        req: UpdateDeliveryRequest,
        actor: &AuditActor,
    ) -> Result<DeliveryView> {
        req.validate()?;
        let mut delivery = self
            .db
            .deliveries()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("发货单不存在".to_string()))?;
        if delivery.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        delivery.update(entities::fulfillment::DeliveryUpdate {
            carrier: req.carrier,
            tracking_no: req.tracking_no,
        })?;
        let audit = actor
            .clone()
            .resource_log("delivery.update", "delivery", id.to_string())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.deliveries().update(&mut delivery, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<Delivery, crate::errors::Error>(delivery)
                })
            })
            .await?;
        Ok(updated.into())
    }

    /// 过账发货（草稿 → 已发货；§8.2 第 2 条跨集合事务）。
    ///
    /// 仓发在同一事务内：校验预占归属（预占必须属于本销售明细且数量充足）、
    /// 消耗预占（含预占流水）、追加库存减少流水、更新库存余额、迁移发货单
    /// 状态、写审计。供应商直发不写自有库存流水，只做 `PREPAY` 门槛校验
    /// （§8.1.5）后迁移状态。重复过账由状态守卫（仅草稿）与流水唯一索引双重
    /// 防护。
    ///
    /// # 参数
    /// * `id` - 发货单主键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后的发货单视图。
    ///
    /// # 错误
    /// * `NotFound` - 发货单/预占/余额不存在
    /// * `ConflictError` - 状态不允许过账或重复过账
    /// * `BusinessLogicError` - 预占归属不符、数量不足或门槛未满足
    /// * `OutcomeUnknown` - 提交结果无法确认
    pub async fn post_delivery(&self, id: &str, actor: &AuditActor) -> Result<DeliveryView> {
        let delivery_id = DeliveryId::new(id.to_string());
        let actor = actor.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let posted = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut delivery = db
                        .deliveries()
                        .find_by_id(delivery_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("发货单不存在".to_string()))?;
                    if delivery.status != DeliveryState::Draft {
                        return Err(Error::ConflictError("只有草稿状态的发货单可以过账".to_string()));
                    }
                    let lines = db
                        .fulfillment()
                        .delivery_lines_by_delivery_ids(std::slice::from_ref(&delivery_id), session)
                        .await?;
                    if lines.is_empty() {
                        return Err(Error::ValidationError("发货单没有行，无法过账".to_string()));
                    }
                    let occurred_at = Instant::now();
                    match delivery.delivery_type {
                        DeliveryType::WarehouseShip => {
                            for line in &lines {
                                post_warehouse_ship_line(&db, session, &delivery, line, &occurred_at, &actor)
                                    .await?;
                            }
                        }
                        DeliveryType::SupplierDirect => {
                            let po = delivery.purchase_order_id.clone().ok_or_else(|| {
                                Error::BusinessLogicError("供应商直发缺少采购来源".to_string())
                            })?;
                            let po = db
                                .purchase_orders()
                                .find_by_id(po.as_ref(), session)
                                .await?
                                .ok_or_else(|| Error::NotFound("来源采购单不存在".to_string()))?;
                            ensure_po_fulfillable(&po)?;
                            ensure_prepay_gate(&db, session, &po).await?;
                        }
                    }
                    delivery.mark_shipped(occurred_at)?;
                    db.deliveries().update(&mut delivery, session).await?;
                    let audit = actor.resource_log("delivery.post", "delivery", delivery_id.to_string())?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<Delivery, crate::errors::Error>(delivery)
                })
            })
            .await?;
        Ok(posted.into())
    }

    // ----------------------------------------------------------- electronic_delivery

    /// 分页查询电子交付记录列表（W09 电子交付视图）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`sales_order_line_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn electronic_delivery_list(
        &self,
        params: &ElectronicDeliveryListParams,
    ) -> Result<PageView<ElectronicDeliveryView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ElectronicDeliveryFilter {
            sales_order_line_id: query.sales_order_line_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .electronic_deliveries()
            .search_electronic_deliveries(&filter, &mut NoTransaction)
            .await?;
        let page_ids: Vec<String> = page.items.iter().map(|row| row.id.clone()).collect();
        let allocation_ids = load_electronic_allocation_ids(&self.db, &page_ids).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| ElectronicDeliveryView {
                id: row.id.clone(),
                fulfillment_no: row.fulfillment_no,
                sales_order_line_id: row.sales_order_line_id.to_string(),
                purchase_order_id: row.purchase_order_id.to_string(),
                purchase_line_sales_allocation_id: allocation_ids.get(&row.id).cloned().unwrap_or_default(),
                quantity: row.quantity,
                result: row.result,
                status: row.status,
                occurred_at: row.occurred_at.unix_secs(),
                recorded_at: row.recorded_at.unix_secs(),
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

    /// 创建电子交付记录（草稿；单集合写入，无事务）。
    ///
    /// 交付对象快照以不透明值传入，服务端用指纹密钥计算查询指纹后落库；
    /// 快照的字段级加密由边界（前端/接入层）完成。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建记录的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - 履约记录号重复（唯一索引透出）
    /// * `RepositoryError` - 数据库写入失败
    pub async fn create_electronic_delivery(
        &self,
        req: CreateElectronicDeliveryRequest,
        actor: &AuditActor,
    ) -> Result<ElectronicDeliveryView> {
        req.validate()?;
        let occurred_at = Instant::from_unix_secs(req.occurred_at);
        let recorded_at = Instant::now();
        let record = ElectronicDelivery::new(
            ElectronicDeliveryId::new(next_id()),
            ElectronicDeliveryData {
                fulfillment_no: req.fulfillment_no,
                sales_order_line_id: req.sales_order_line_id,
                purchase_order_id: req.purchase_order_id,
                purchase_line_sales_allocation_id: req.purchase_line_sales_allocation_id,
                recipient_snapshot: req.recipient_snapshot.clone(),
                recipient_snapshot_fingerprint: ElectronicDelivery::recipient_snapshot_fingerprint(
                    &req.recipient_snapshot,
                    &self.fingerprint_key,
                ),
                quantity: req.quantity,
                result: req.result,
                evidence_attachment_id: req.evidence_attachment_id,
                fact_no: next_id(),
                occurred_at,
                recorded_at,
                recorded_by: actor.id().to_string(),
                source_type: SourceType::Erp,
                source_reference: None,
                reason_code: None,
                reason_text: None,
            },
        )?;
        let audit = actor.clone().resource_log(
            "electronic_delivery.create",
            "electronic_delivery",
            record.base.id.clone(),
        )?;
        self.db
            .electronic_deliveries()
            .create(&record, &mut NoTransaction)
            .await?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
        Ok(record.into())
    }

    /// 确认电子交付（草稿 → 已确认；§8.1.5 + §6.7 跨集合事务）。
    ///
    /// 在同一事务内：校验采购单可履约与 `PREPAY` 门槛、校验采购销售分配的
    /// 有效性（采购行归属当前生效版本、销售行归属本明细）、迁移记录状态、写
    /// 审计。重复确认由状态守卫（仅草稿）防护。
    ///
    /// # 参数
    /// * `id` - 记录主键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回确认后的记录视图。
    ///
    /// # 错误
    /// * `NotFound` - 记录/采购单/分配不存在
    /// * `ConflictError` - 状态不允许确认或重复确认
    /// * `BusinessLogicError` - 门槛未满足或分配无效
    /// * `OutcomeUnknown` - 提交结果无法确认
    pub async fn confirm_electronic_delivery(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<ElectronicDeliveryView> {
        let record_id = ElectronicDeliveryId::new(id.to_string());
        let actor = actor.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let confirmed = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut record = db
                        .electronic_deliveries()
                        .find_by_id(record_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("电子交付记录不存在".to_string()))?;
                    if record.status != ElectronicDeliveryState::Draft {
                        return Err(Error::ConflictError(
                            "只有草稿状态的电子交付记录可以确认".to_string(),
                        ));
                    }
                    let po = db
                        .purchase_orders()
                        .find_by_id(record.purchase_order_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("来源采购单不存在".to_string()))?;
                    ensure_po_fulfillable(&po)?;
                    ensure_prepay_gate(&db, session, &po).await?;
                    ensure_allocation_valid(
                        &db,
                        session,
                        &po,
                        &record.purchase_line_sales_allocation_id,
                        &record.sales_order_line_id,
                    )
                    .await?;
                    record.confirm()?;
                    db.electronic_deliveries().update(&mut record, session).await?;
                    let audit = actor.resource_log(
                        "electronic_delivery.confirm",
                        "electronic_delivery",
                        record_id.to_string(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<ElectronicDelivery, crate::errors::Error>(record)
                })
            })
            .await?;
        Ok(confirmed.into())
    }

    // ---------------------------------------------------------- service_fulfillment

    /// 分页查询线下服务履约记录列表（W09 服务视图）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`sales_order_line_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn service_fulfillment_list(
        &self,
        params: &ServiceFulfillmentListParams,
    ) -> Result<PageView<ServiceFulfillmentView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ServiceFulfillmentFilter {
            sales_order_line_id: query.sales_order_line_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .service_fulfillments()
            .search_service_fulfillments(&filter, &mut NoTransaction)
            .await?;
        let page_ids: Vec<String> = page.items.iter().map(|row| row.id.clone()).collect();
        let allocation_ids = load_service_allocation_ids(&self.db, &page_ids).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| ServiceFulfillmentView {
                id: row.id.clone(),
                fulfillment_no: row.fulfillment_no,
                sales_order_line_id: row.sales_order_line_id.to_string(),
                purchase_order_id: row.purchase_order_id.to_string(),
                purchase_line_sales_allocation_id: allocation_ids.get(&row.id).cloned().unwrap_or_default(),
                quantity: row.quantity,
                result: row.result,
                status: row.status,
                occurred_at: row.occurred_at.unix_secs(),
                recorded_at: row.recorded_at.unix_secs(),
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

    /// 创建线下服务履约记录（草稿；单集合写入，无事务）。
    ///
    /// 服务地点与交付对象快照以不透明值传入，服务端计算查询指纹后落库。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建记录的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - 履约记录号重复（唯一索引透出）
    /// * `RepositoryError` - 数据库写入失败
    pub async fn create_service_fulfillment(
        &self,
        req: CreateServiceFulfillmentRequest,
        actor: &AuditActor,
    ) -> Result<ServiceFulfillmentView> {
        req.validate()?;
        let occurred_at = Instant::from_unix_secs(req.occurred_at);
        let recorded_at = Instant::now();
        let record = ServiceFulfillment::new(
            ServiceFulfillmentId::new(next_id()),
            ServiceFulfillmentData {
                fulfillment_no: req.fulfillment_no,
                sales_order_line_id: req.sales_order_line_id,
                purchase_order_id: req.purchase_order_id,
                purchase_line_sales_allocation_id: req.purchase_line_sales_allocation_id,
                recipient_snapshot: req.recipient_snapshot.clone(),
                recipient_snapshot_fingerprint: ServiceFulfillment::recipient_snapshot_fingerprint(
                    &req.recipient_snapshot,
                    &self.fingerprint_key,
                ),
                quantity: req.quantity,
                result: req.result,
                evidence_attachment_id: req.evidence_attachment_id,
                service_location_encrypted: req.service_location.clone(),
                service_location_fingerprint: ServiceFulfillment::service_location_fingerprint(
                    &req.service_location,
                    &self.fingerprint_key,
                ),
                service_started_at: req.service_started_at.map(Instant::from_unix_secs),
                service_ended_at: req.service_ended_at.map(Instant::from_unix_secs),
                completion_note: req.completion_note,
                fact_no: next_id(),
                occurred_at,
                recorded_at,
                recorded_by: actor.id().to_string(),
                source_type: SourceType::Erp,
                source_reference: None,
                reason_code: None,
                reason_text: None,
            },
        )?;
        let audit = actor.clone().resource_log(
            "service_fulfillment.create",
            "service_fulfillment",
            record.base.id.clone(),
        )?;
        self.db
            .service_fulfillments()
            .create(&record, &mut NoTransaction)
            .await?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
        Ok(record.into())
    }

    /// 确认服务履约（草稿 → 已确认；§8.1.5 + §6.7 跨集合事务）。
    ///
    /// 与电子交付确认相同的门槛与分配有效性校验；重复确认由状态守卫防护。
    ///
    /// # 参数
    /// * `id` - 记录主键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回确认后的记录视图。
    ///
    /// # 错误
    /// * `NotFound` - 记录/采购单/分配不存在
    /// * `ConflictError` - 状态不允许确认或重复确认
    /// * `BusinessLogicError` - 门槛未满足或分配无效
    /// * `OutcomeUnknown` - 提交结果无法确认
    pub async fn confirm_service_fulfillment(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<ServiceFulfillmentView> {
        let record_id = ServiceFulfillmentId::new(id.to_string());
        let actor = actor.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let confirmed = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut record = db
                        .service_fulfillments()
                        .find_by_id(record_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("服务履约记录不存在".to_string()))?;
                    if record.status != ServiceFulfillmentState::Draft {
                        return Err(Error::ConflictError(
                            "只有草稿状态的服务履约记录可以确认".to_string(),
                        ));
                    }
                    let po = db
                        .purchase_orders()
                        .find_by_id(record.purchase_order_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("来源采购单不存在".to_string()))?;
                    ensure_po_fulfillable(&po)?;
                    ensure_prepay_gate(&db, session, &po).await?;
                    ensure_allocation_valid(
                        &db,
                        session,
                        &po,
                        &record.purchase_line_sales_allocation_id,
                        &record.sales_order_line_id,
                    )
                    .await?;
                    record.confirm()?;
                    db.service_fulfillments().update(&mut record, session).await?;
                    let audit = actor.resource_log(
                        "service_fulfillment.confirm",
                        "service_fulfillment",
                        record_id.to_string(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<ServiceFulfillment, crate::errors::Error>(record)
                })
            })
            .await?;
        Ok(confirmed.into())
    }

    // ---------------------------------------------------------- customer_acceptance

    /// 分页查询客户验收单列表（W06 验收历史视图）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`sales_order_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn customer_acceptance_list(
        &self,
        params: &CustomerAcceptanceListParams,
    ) -> Result<PageView<CustomerAcceptanceView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = CustomerAcceptanceFilter {
            sales_order_id: query.sales_order_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .customer_acceptances()
            .search_customer_acceptances(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| CustomerAcceptanceView {
                id: row.id,
                acceptance_no: row.acceptance_no,
                sales_order_id: row.sales_order_id.to_string(),
                accepted_at: row.accepted_at.unix_secs(),
                result: row.result,
                status: row.status,
                reversal_of_acceptance_id: row.reversal_of_acceptance_id.map(|id| id.to_string()),
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

    /// 查询客户验收单详情（表头 + 行 + 分配）。
    ///
    /// # 参数
    /// * `id` - 验收单主键
    ///
    /// # 返回
    /// 返回验收单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 验收单不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn customer_acceptance_detail(&self, id: &str) -> Result<CustomerAcceptanceDetailView> {
        let acceptance = self
            .db
            .customer_acceptances()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户验收单不存在".to_string()))?;
        let lines = self
            .db
            .fulfillment()
            .acceptance_lines_by_acceptance_ids(&[acceptance.base.id.clone().into()], &mut NoTransaction)
            .await?;
        let line_ids: Vec<CustomerAcceptanceLineId> =
            lines.iter().map(|line| line.base.id.clone().into()).collect();
        let allocations = self
            .db
            .fulfillment()
            .allocations_by_acceptance_lines(&line_ids, &mut NoTransaction)
            .await?;
        Ok(CustomerAcceptanceDetailView {
            acceptance: acceptance.into(),
            lines: lines.into_iter().map(Into::into).collect(),
            allocations: allocations.into_iter().map(Into::into).collect(),
        })
    }

    /// 创建客户验收单（草稿，跨集合：表头 + 行 + 审计）。
    ///
    /// 创建阶段不写验收分配；分配在过账时按行守恒与履约事实上限校验后写入。
    ///
    /// # 参数
    /// * `req` - 创建请求（表头 + 行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建验收单的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - 单号重复（唯一索引透出）
    /// * `RepositoryError` - 数据库写入失败
    pub async fn create_customer_acceptance(
        &self,
        req: CreateCustomerAcceptanceRequest,
        actor: &AuditActor,
    ) -> Result<CustomerAcceptanceView> {
        req.validate()?;
        let id = CustomerAcceptanceId::new(next_id());
        let acceptance = CustomerAcceptance::new(
            id.clone(),
            CustomerAcceptanceData {
                acceptance_no: req.acceptance_no,
                sales_order_id: req.sales_order_id,
                accepted_at: Instant::from_unix_secs(req.accepted_at),
                result: req.result,
            },
        )?;
        let lines = build_acceptance_lines(&id, &req.lines)?;
        let audit = actor.clone().resource_log(
            "customer_acceptance.create",
            "customer_acceptance",
            id.to_string(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let acceptance_for_tx = acceptance.clone();
        let lines_for_tx = lines.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.fulfillment()
                        .create_customer_acceptance_with_lines(&acceptance_for_tx, &lines_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(acceptance.into())
    }

    /// 过账客户验收（草稿 → 已过账；§8.2 第 5 条跨集合事务）。
    ///
    /// 在同一事务内：锁定验收行与履约事实、校验逐行分配守恒（分配合计等于
    /// 通过数量）、校验每个履约事实的净验收数量不超过净成功履约数量、写
    /// `APPLY` 分配、迁移验收单状态、写审计。重复过账由状态守卫（仅草稿）
    /// 与状态机（`Draft → Posted`）防护。
    ///
    /// # 参数
    /// * `id` - 验收单主键
    /// * `req` - 过账请求（逐行分配）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后的验收单视图。
    ///
    /// # 错误
    /// * `NotFound` - 验收单/履约事实不存在
    /// * `ConflictError` - 状态不允许过账或重复过账
    /// * `ValidationError` - 分配不守恒或超上限
    /// * `OutcomeUnknown` - 提交结果无法确认
    pub async fn post_customer_acceptance(
        &self,
        id: &str,
        req: PostCustomerAcceptanceRequest,
        actor: &AuditActor,
    ) -> Result<CustomerAcceptanceView> {
        req.validate()?;
        let acceptance_id = CustomerAcceptanceId::new(id.to_string());
        let actor = actor.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let posted = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut acceptance = db
                        .customer_acceptances()
                        .find_by_id(acceptance_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("客户验收单不存在".to_string()))?;
                    if acceptance.status != CustomerAcceptanceState::Draft {
                        return Err(Error::ConflictError(
                            "只有草稿状态的客户验收单可以过账".to_string(),
                        ));
                    }
                    let lines = db
                        .fulfillment()
                        .acceptance_lines_by_acceptance_ids(std::slice::from_ref(&acceptance_id), session)
                        .await?;
                    ensure_post_lines_match(&lines, &req.lines)?;
                    for line in &lines {
                        let allocations = req
                            .lines
                            .iter()
                            .find(|input| input.sales_order_line_id == line.sales_order_line_id)
                            .map(|input| input.allocations.clone())
                            .ok_or_else(|| Error::ValidationError("过账分配缺少验收行".to_string()))?;
                        ensure_line_allocations_conserved(line, &allocations)?;
                        for allocation in &allocations {
                            write_acceptance_allocation(
                                &db,
                                session,
                                &line.base.id,
                                allocation,
                                line,
                                &acceptance.sales_order_id,
                            )
                            .await?;
                        }
                    }
                    acceptance.mark_posted()?;
                    db.customer_acceptances().update(&mut acceptance, session).await?;
                    let audit = actor.resource_log(
                        "customer_acceptance.post",
                        "customer_acceptance",
                        acceptance_id.to_string(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<CustomerAcceptance, crate::errors::Error>(acceptance)
                })
            })
            .await?;
        Ok(posted.into())
    }

    /// 冲正客户验收（已过账 → 已冲正；§8.2 第 5 条反向分配事务）。
    ///
    /// 误录时新增反向验收单：原验收行的通过/短少/拒收数量镜像复制，原
    /// `APPLY` 分配逐条生成 `REVERSE` 分配（引用原分配），新验收单立即过账，
    /// 原验收单登记反向引用并迁移到 `REVERSED`。冲正不覆盖原验收事实。
    ///
    /// # 参数
    /// * `id` - 待冲正验收单主键
    /// * `req` - 冲正请求（期望版本 + 原因）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建反向验收单的视图。
    ///
    /// # 错误
    /// * `NotFound` - 验收单不存在
    /// * `ConflictError` - 版本不符或状态不允许冲正
    /// * `OutcomeUnknown` - 提交结果无法确认
    pub async fn reverse_customer_acceptance(
        &self,
        id: &str,
        req: ReverseCustomerAcceptanceRequest,
        actor: &AuditActor,
    ) -> Result<CustomerAcceptanceView> {
        req.validate()?;
        let original_id = CustomerAcceptanceId::new(id.to_string());
        let actor = actor.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let reversed = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut original = db
                        .customer_acceptances()
                        .find_by_id(original_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("客户验收单不存在".to_string()))?;
                    if original.base.version != req.expected_version {
                        return Err(Error::ConflictError(
                            "数据已被其他请求修改，请刷新后重试".to_string(),
                        ));
                    }
                    if original.status != CustomerAcceptanceState::Posted {
                        return Err(Error::ConflictError("只有已过账的客户验收单可以冲正".to_string()));
                    }
                    let original_lines = db
                        .fulfillment()
                        .acceptance_lines_by_acceptance_ids(std::slice::from_ref(&original_id), session)
                        .await?;
                    let original_line_ids: Vec<CustomerAcceptanceLineId> = original_lines
                        .iter()
                        .map(|line| line.base.id.clone().into())
                        .collect();
                    let original_allocations = db
                        .fulfillment()
                        .allocations_by_acceptance_lines(&original_line_ids, session)
                        .await?;
                    if original_allocations.is_empty() {
                        return Err(Error::ValidationError(
                            "原验收单没有可冲正的分配，无法冲正".to_string(),
                        ));
                    }
                    let reverse_acceptance = CustomerAcceptance::new(
                        CustomerAcceptanceId::new(next_id()),
                        CustomerAcceptanceData {
                            acceptance_no: format!("REV-{}", original.acceptance_no),
                            sales_order_id: original.sales_order_id.clone(),
                            accepted_at: Instant::now(),
                            result: AcceptanceResult::Rejected,
                        },
                    )?;
                    let mut reverse_lines = Vec::with_capacity(original_lines.len());
                    for line in &original_lines {
                        reverse_lines.push(
                            CustomerAcceptanceLine::new(
                                CustomerAcceptanceLineId::new(next_id()),
                                CustomerAcceptanceLineData {
                                    customer_acceptance_id: reverse_acceptance.base.id.clone().into(),
                                    line_no: line.line_no,
                                    sales_order_line_id: line.sales_order_line_id.clone(),
                                    accepted_quantity: line.accepted_quantity,
                                    short_quantity: line.short_quantity,
                                    rejected_quantity: line.rejected_quantity,
                                    reason: Some(req.reason_text.clone()),
                                    evidence_attachment_id: None,
                                },
                            )
                            .map_err(Error::Logic)?,
                        );
                    }
                    let reverse_line_ids: Vec<CustomerAcceptanceLineId> = reverse_lines
                        .iter()
                        .map(|line| line.base.id.clone().into())
                        .collect();
                    let mut reverse_allocations = Vec::with_capacity(original_allocations.len());
                    for (index, allocation) in original_allocations
                        .iter()
                        .filter(|allocation| allocation.allocation_action == AllocationAction::Apply)
                        .enumerate()
                    {
                        reverse_allocations.push(
                            AcceptanceFulfillmentAllocation::new(
                                AcceptanceFulfillmentAllocationId::new(next_id()),
                                AcceptanceFulfillmentAllocationData {
                                    customer_acceptance_line_id: reverse_line_ids[index].clone(),
                                    fulfillment_fact_type: allocation.fulfillment_fact_type,
                                    fulfillment_line_id: allocation.fulfillment_line_id.clone(),
                                    allocation_action: AllocationAction::Reverse,
                                    allocated_quantity: allocation.allocated_quantity,
                                    reverses_allocation_id: Some(allocation.base.id.clone().into()),
                                },
                            )
                            .map_err(Error::Logic)?,
                        );
                    }
                    db.fulfillment()
                        .create_customer_acceptance_with_lines(&reverse_acceptance, &reverse_lines, session)
                        .await?;
                    for allocation in &reverse_allocations {
                        db.acceptance_fulfillment_allocations()
                            .create(allocation, session)
                            .await?;
                    }
                    let mut reverse_acceptance = reverse_acceptance;
                    reverse_acceptance.mark_posted()?;
                    db.customer_acceptances()
                        .update(&mut reverse_acceptance, session)
                        .await?;
                    original.reverse(reverse_acceptance.base.id.clone().into())?;
                    db.customer_acceptances().update(&mut original, session).await?;
                    let audit = actor.resource_log(
                        "customer_acceptance.reverse",
                        "customer_acceptance",
                        original_id.to_string(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<CustomerAcceptance, crate::errors::Error>(reverse_acceptance)
                })
            })
            .await?;
        Ok(reversed.into())
    }

    /// 查询客户验收工作台（W06：销售行 + 可验收事实 + 验收历史）。
    ///
    /// 可验收数量守恒：`eligible = 净成功履约数量 − 净已验收分配（APPLY −
    /// REVERSE）`，全部由服务端计算（§8.2 第 5 条）。
    ///
    /// # 参数
    /// * `sales_order_id` - 销售单
    ///
    /// # 返回
    /// 返回验收工作台视图。
    ///
    /// # 错误
    /// * `NotFound` - 销售单或其生效版本不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn acceptance_eligibility(&self, sales_order_id: &str) -> Result<AcceptanceEligibilityView> {
        let so_id = SalesOrderId::new(sales_order_id.to_string());
        let so = self
            .db
            .sales_orders()
            .find_by_id(sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        let revision_id = so
            .stable
            .current_revision_id
            .clone()
            .ok_or_else(|| Error::NotFound("销售单没有生效版本".to_string()))?;
        let revision = self
            .db
            .sales_order_revisions()
            .find_by_id(&revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售生效版本不存在".to_string()))?;
        let revision_lines = self
            .db
            .sales_order_revision_lines()
            .list_lines_by_revision(&revision.base.id.clone().into(), &mut NoTransaction)
            .await?;
        let revision_line_ids: Vec<SalesOrderRevisionLineId> = revision_lines
            .iter()
            .map(|line| line.base.id.clone().into())
            .collect();
        let goods_service_lines = self
            .db
            .sales_order_goods_service_line_revisions()
            .list_by_revision_line_ids(&revision_line_ids, &mut NoTransaction)
            .await?;
        let deliveries = self
            .db
            .deliveries()
            .find_many(
                doc! {
                    "sales_order_id": so_id.to_string(),
                    "status": { "$in": vec![DeliveryState::Shipped.as_str(), DeliveryState::Signed.as_str()] },
                },
                &mut NoTransaction,
            )
            .await?;
        let delivery_ids: Vec<DeliveryId> = deliveries
            .iter()
            .map(|delivery| delivery.base.id.clone().into())
            .collect();
        let delivery_lines = self
            .db
            .fulfillment()
            .delivery_lines_by_delivery_ids(&delivery_ids, &mut NoTransaction)
            .await?;
        let electronic = self
            .db
            .electronic_deliveries()
            .find_many(
                doc! {
                    "sales_order_line_id": { "$in": so_line_ids(&revision_lines) },
                    "status": ElectronicDeliveryState::Confirmed.as_str(),
                },
                &mut NoTransaction,
            )
            .await?;
        let service = self
            .db
            .service_fulfillments()
            .find_many(
                doc! {
                    "sales_order_line_id": { "$in": so_line_ids(&revision_lines) },
                    "status": ServiceFulfillmentState::Confirmed.as_str(),
                },
                &mut NoTransaction,
            )
            .await?;
        let delivery_fact_ids: Vec<String> = delivery_lines.iter().map(|line| line.base.id.clone()).collect();
        let electronic_fact_ids: Vec<String> =
            electronic.iter().map(|record| record.base.id.clone()).collect();
        let service_fact_ids: Vec<String> = service.iter().map(|record| record.base.id.clone()).collect();
        let delivery_allocations = self
            .db
            .fulfillment()
            .allocations_by_fulfillment_fact(
                FulfillmentFactType::Delivery,
                &delivery_fact_ids,
                &mut NoTransaction,
            )
            .await?;
        let electronic_allocations = self
            .db
            .fulfillment()
            .allocations_by_fulfillment_fact(
                FulfillmentFactType::ElectronicDelivery,
                &electronic_fact_ids,
                &mut NoTransaction,
            )
            .await?;
        let service_allocations = self
            .db
            .fulfillment()
            .allocations_by_fulfillment_fact(
                FulfillmentFactType::ServiceFulfillment,
                &service_fact_ids,
                &mut NoTransaction,
            )
            .await?;
        let history = self
            .db
            .customer_acceptances()
            .find_many_sorted(
                doc! { "sales_order_id": so_id.to_string() },
                doc! { "accepted_at": -1 },
                &mut NoTransaction,
            )
            .await?;
        let groups = build_eligibility_groups(
            &revision_lines,
            &goods_service_lines,
            &deliveries,
            &delivery_lines,
            &electronic,
            &service,
            &delivery_allocations,
            &electronic_allocations,
            &service_allocations,
        );
        Ok(AcceptanceEligibilityView {
            sales_order_id: so_id.to_string(),
            sales_lines: groups,
            history: history.into_iter().map(Into::into).collect(),
        })
    }
}

// ------------------------------------------------------------------ private helpers

/// 批量取供应商直发货单的采购来源（P2 投影行未含 `purchase_order_id`）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `delivery_ids` - 供应商直发的主键集合（仓发无需查询）
///
/// # 返回
/// 返回「发货单 → 采购来源」映射。
///
/// # 错误
/// 批量查询失败时返回 `RepositoryError`。
async fn load_direct_po_ids(
    db: &Database,
    delivery_ids: &[String],
) -> Result<HashMap<String, Option<String>>> {
    if delivery_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut map = HashMap::new();
    for delivery in db
        .deliveries()
        .find_many(doc! { "id": { "$in": delivery_ids } }, &mut NoTransaction)
        .await?
    {
        map.insert(
            delivery.base.id.clone(),
            delivery.purchase_order_id.map(|id| id.to_string()),
        );
    }
    Ok(map)
}

/// 批量取电子交付记录的采购销售分配（P2 投影行未含该字段）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `record_ids` - 记录主键集合
///
/// # 返回
/// 返回「记录 → 采购销售分配」映射。
///
/// # 错误
/// 批量查询失败时返回 `RepositoryError`。
async fn load_electronic_allocation_ids(
    db: &Database,
    record_ids: &[String],
) -> Result<HashMap<String, String>> {
    if record_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut map = HashMap::new();
    for record in db
        .electronic_deliveries()
        .find_many(doc! { "id": { "$in": record_ids } }, &mut NoTransaction)
        .await?
    {
        map.insert(
            record.base.id.clone(),
            record.purchase_line_sales_allocation_id.to_string(),
        );
    }
    Ok(map)
}

/// 批量取服务履约记录的采购销售分配（P2 投影行未含该字段）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `record_ids` - 记录主键集合
///
/// # 返回
/// 返回「记录 → 采购销售分配」映射。
///
/// # 错误
/// 批量查询失败时返回 `RepositoryError`。
async fn load_service_allocation_ids(
    db: &Database,
    record_ids: &[String],
) -> Result<HashMap<String, String>> {
    if record_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut map = HashMap::new();
    for record in db
        .service_fulfillments()
        .find_many(doc! { "id": { "$in": record_ids } }, &mut NoTransaction)
        .await?
    {
        map.insert(
            record.base.id.clone(),
            record.purchase_line_sales_allocation_id.to_string(),
        );
    }
    Ok(map)
}

/// 校验采购单处于可履约状态（§6.6：生效或部分执行）。
///
/// # 参数
/// * `po` - 采购单实体
///
/// # 返回
/// 可履约返回 `Ok(())`。
///
/// # 错误
/// 采购单不在生效/部分执行状态时返回 `BusinessLogicError`。
fn ensure_po_fulfillable(po: &PurchaseOrder) -> Result<()> {
    match po.stable.status {
        PurchaseOrderStatus::Effective | PurchaseOrderStatus::PartiallyExecuted => Ok(()),
        _ => Err(Error::BusinessLogicError(
            "采购单不在可履约状态，无法过账".to_string(),
        )),
    }
}

/// 校验 `PREPAY` 采购履约门槛（§8.1.5）。
///
/// 按采购单当前生效版本的付款条件快照判定是否先款后货；门槛开启时重算
/// 有效已过账付款净核销金额（D19：应付子账 → 分录 → 付款核销分配，`APPLY −
/// REVERSE` 净额），达到冻结的金额或比例门槛才允许过账。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `po` - 采购单实体
///
/// # 返回
/// 门槛满足返回 `Ok(())`。
///
/// # 错误
/// 生效版本缺失、或有效付款未达门槛时返回 `BusinessLogicError`。
async fn ensure_prepay_gate(
    db: &Database,
    session: &mut mongodb::ClientSession,
    po: &PurchaseOrder,
) -> Result<()> {
    let revision = load_po_current_revision(db, session, po).await?;
    let snapshot = &revision.payment_term_snapshot;
    if !snapshot.prepay_gate {
        return Ok(());
    }
    let effective_paid = effective_paid_amount(db, session, &revision.purchase_order_id).await?;
    if let Some(min_amount) = snapshot.prepay_minimum_amount {
        if effective_paid.to_decimal() < min_amount.to_decimal() {
            return Err(Error::BusinessLogicError(
                "该采购单为先款后货，有效付款未达金额门槛，请先完成付款".to_string(),
            ));
        }
    }
    if let Some(min_ratio) = snapshot.prepay_minimum_ratio {
        let required = round_to_cent(revision.gross_amount.to_decimal() * min_ratio.to_decimal());
        if effective_paid.to_decimal() < required {
            return Err(Error::BusinessLogicError(
                "该采购单为先款后货，有效付款未达比例门槛，请先完成付款".to_string(),
            ));
        }
    }
    Ok(())
}

/// 取采购单当前生效版本。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `po` - 采购单实体
///
/// # 返回
/// 返回生效版本实体。
///
/// # 错误
/// 生效版本缺失时返回 `BusinessLogicError`。
async fn load_po_current_revision(
    db: &Database,
    session: &mut mongodb::ClientSession,
    po: &PurchaseOrder,
) -> Result<PurchaseOrderRevision> {
    let revision_id = po
        .stable
        .current_revision_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("采购单没有生效版本，无法履约".to_string()))?;
    db.purchase_order_revisions()
        .find_by_id(&revision_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("采购生效版本不存在".to_string()))
}

/// 重算采购单的有效已过账付款净核销金额（D19 跨域只读）。
///
/// 路径：应付往来子账（来源单据 = 采购单）→ 应付分录 → 付款核销分配，
/// `APPLY − REVERSE` 净额求和。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `po_id` - 采购单
///
/// # 返回
/// 返回净核销金额（未付款为 0）。
///
/// # 错误
/// 任一步查询失败时返回 `RepositoryError`。
async fn effective_paid_amount(
    db: &Database,
    session: &mut mongodb::ClientSession,
    po_id: &PurchaseOrderId,
) -> Result<Amount> {
    let accounts = db
        .payable_accounts()
        .find_many(
            doc! {
                "source_document_id": po_id.to_string(),
                "source_type": "purchase_order",
            },
            session,
        )
        .await?;
    let account_ids: Vec<PayableAccountId> = accounts
        .iter()
        .map(|account| account.base.id.clone().into())
        .collect();
    let entries = db
        .payable_entries()
        .find_entries_by_accounts(&account_ids, session)
        .await?;
    let entry_ids: Vec<PayableEntryId> = entries
        .iter()
        .filter(|entry| entry.source_document_id == po_id.to_string())
        .map(|entry| entry.base.id.clone().into())
        .collect();
    let allocations = db
        .payment_allocations()
        .find_allocations_by_entries(&entry_ids, session)
        .await?;
    let mut net = Amount::from_str("0").map_err(Error::Logic)?;
    for allocation in allocations {
        net = match allocation.allocation_action {
            PayableAllocationAction::Apply => net.checked_add(allocation.allocated_amount),
            PayableAllocationAction::Reverse => net.checked_sub(allocation.allocated_amount),
        };
    }
    Ok(net)
}

/// 校验采购销售分配有效（§6.7：采购行归属当前生效版本、销售行归属本明细）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `po` - 采购单实体
/// * `allocation_id` - 采购销售分配
/// * `sales_order_line_id` - 销售稳定明细
///
/// # 返回
/// 有效返回 `Ok(())`。
///
/// # 错误
/// 分配不存在、采购行不属于当前生效版本或销售行不属于本明细时返回
/// `BusinessLogicError`。
async fn ensure_allocation_valid(
    db: &Database,
    session: &mut mongodb::ClientSession,
    po: &PurchaseOrder,
    allocation_id: &PurchaseLineSalesAllocationId,
    sales_order_line_id: &entities::ids::SalesOrderLineId,
) -> Result<()> {
    let allocation = db
        .purchase_line_sales_allocations()
        .find_by_id(allocation_id.as_ref(), session)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("采购销售分配不存在".to_string()))?;
    let revision = load_po_current_revision(db, session, po).await?;
    let revision_lines = db
        .purchase_order_revision_lines()
        .find_lines_by_revision_ids(&[revision.base.id.clone().into()], session)
        .await?;
    if !revision_lines
        .iter()
        .any(|line| line.base.id == allocation.purchase_order_revision_line_id.to_string())
    {
        return Err(Error::BusinessLogicError(
            "采购销售分配不属于当前生效版本".to_string(),
        ));
    }
    let sales_revision_lines = db
        .sales_order_revision_lines()
        .find_many(
            doc! {
                "id": allocation.sales_order_revision_line_id.to_string(),
                "sales_order_line_id": sales_order_line_id.to_string(),
            },
            session,
        )
        .await?;
    if sales_revision_lines.is_empty() {
        return Err(Error::BusinessLogicError(
            "采购销售分配与销售明细不一致".to_string(),
        ));
    }
    Ok(())
}

/// 取当前生效版本行涉及的销售稳定明细 ID 集合。
///
/// # 参数
/// * `revision_lines` - 销售版本公共行
///
/// # 返回
/// 返回销售稳定明细 ID 字符串集合。
fn so_line_ids(revision_lines: &[entities::sales_order::SalesOrderRevisionLine]) -> Vec<String> {
    revision_lines
        .iter()
        .map(|line| line.sales_order_line_id.to_string())
        .collect()
}

/// 统计采购单已过账入库的累计有效收货（按采购版本行分组）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `po_id` - 采购单
///
/// # 返回
/// 返回「采购版本行 → 累计合格数量」映射。
///
/// # 错误
/// 任一步查询失败时返回 `RepositoryError`。
async fn cumulative_received_quantities(
    db: &Database,
    session: &mut mongodb::ClientSession,
    po_id: &PurchaseOrderId,
) -> Result<HashMap<String, Quantity>> {
    let receipts = db
        .purchase_receipts()
        .find_many(
            doc! {
                "purchase_order_id": po_id.to_string(),
                "status": PurchaseReceiptState::Posted.as_str(),
            },
            session,
        )
        .await?;
    let receipt_ids: Vec<PurchaseReceiptId> = receipts
        .iter()
        .map(|receipt| receipt.base.id.clone().into())
        .collect();
    let lines = db
        .fulfillment()
        .receipt_lines_by_receipt_ids(&receipt_ids, session)
        .await?;
    let mut totals: HashMap<String, Quantity> = HashMap::new();
    for line in lines {
        totals
            .entry(line.purchase_order_revision_line_id.to_string())
            .and_modify(|total| {
                *total = Quantity::try_from(total.to_decimal() + line.qualified_quantity.to_decimal())
                    .expect("Quantity 加法和不超过精度上限")
            })
            .or_insert(line.qualified_quantity);
    }
    Ok(totals)
}

/// 校验入库行累计有效收货不超过当前有效采购数量（§6.7）。
///
/// # 参数
/// * `line` - 入库行
/// * `revision_lines` - 采购生效版本行
/// * `received` - 已过账累计合格数量（不含本单）
///
/// # 返回
/// 不超收返回 `Ok(())`。
///
/// # 错误
/// 采购明细缺失、物流费用行不可入库或超收时返回 `ValidationError`/
/// `BusinessLogicError`。
fn ensure_receipt_within_revision(
    line: &PurchaseReceiptLine,
    revision_lines: &[entities::purchase_order::PurchaseOrderRevisionLine],
    received: &HashMap<String, Quantity>,
) -> Result<()> {
    let revision_line = revision_lines
        .iter()
        .find(|revision_line| revision_line.base.id == line.purchase_order_revision_line_id.to_string())
        .ok_or_else(|| Error::BusinessLogicError("采购明细不存在".to_string()))?;
    let available = revision_line
        .quantity
        .ok_or_else(|| Error::BusinessLogicError("物流费用行不能入库".to_string()))?;
    let already = received
        .get(line.purchase_order_revision_line_id.as_ref())
        .copied()
        .unwrap_or_else(|| Quantity::from_str("0").unwrap());
    let posting =
        Quantity::try_from(line.qualified_quantity.to_decimal() + line.rejected_quantity.to_decimal())
            .map_err(Error::Logic)?;
    if already.to_decimal() + posting.to_decimal() > available.to_decimal() {
        return Err(Error::BusinessLogicError(
            "累计有效收货超过当前有效采购数量，超收必须走明确审批和采购变更".to_string(),
        ));
    }
    Ok(())
}

/// 过账单条入库行（流水 + 余额 + 预占，位于调用方事务内）。
///
/// 仅合格数量形成库存入账和销售预占（§6.7）；预占沿采购销售分配按比例
/// 分摊回原销售明细，最后一个分配吸收舍入尾差。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `receipt` - 入库单表头
/// * `line` - 入库行
/// * `revision_lines` - 采购生效版本行
/// * `occurred_at` - 过账业务时间
/// * `actor` - 审计操作人（记录人身份）
///
/// # 返回
/// 无返回值；写入失败时返回错误。
///
/// # 错误
/// 采购明细缺失、余额写入失败或预占建立失败时返回错误。
async fn post_receipt_line(
    db: &Database,
    session: &mut mongodb::ClientSession,
    receipt: &PurchaseReceipt,
    line: &PurchaseReceiptLine,
    revision_lines: &[entities::purchase_order::PurchaseOrderRevisionLine],
    occurred_at: &Instant,
    actor: &AuditActor,
) -> Result<()> {
    let qualified = line.qualified_quantity.to_decimal();
    if qualified <= Quantity::from_str("0").unwrap().to_decimal() {
        return Ok(());
    }
    let revision_line = revision_lines
        .iter()
        .find(|revision_line| revision_line.base.id == line.purchase_order_revision_line_id.to_string())
        .ok_or_else(|| Error::BusinessLogicError("采购明细不存在".to_string()))?;
    let sku_id = revision_line
        .sku_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("物流费用行不能入库".to_string()))?;
    let balance_id = ensure_or_create_balance(
        db,
        session,
        &receipt.warehouse_id,
        &sku_id,
        line.qualified_quantity,
    )
    .await?;
    let movement = StockMovement::new(
        StockMovementId::new(next_id()),
        StockMovementData {
            warehouse_id: receipt.warehouse_id.clone(),
            sku_id: sku_id.clone(),
            movement_type: MovementType::PurchaseReceiptIn,
            direction: MovementDirection::Increase,
            quantity: line.qualified_quantity,
            source_document_id: receipt.base.id.clone(),
            source_line_id: Some(line.base.id.clone()),
            reversal_of_movement_id: None,
            fact_no: next_id(),
            occurred_at: *occurred_at,
            recorded_at: *occurred_at,
            recorded_by: actor.id().to_string(),
            source_type: SourceType::Erp,
            source_reference: None,
            reason_code: None,
            reason_text: None,
        },
    )?;
    db.stock_movements().create(&movement, session).await?;
    establish_reservations(db, session, receipt, line, revision_line, &sku_id, &balance_id).await
}

/// 建立/更新库存余额并返回余额主键（位于调用方事务内）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `warehouse_id` - 仓库
/// * `sku_id` - SKU
/// * `quantity` - 本次入库数量
///
/// # 返回
/// 返回余额主键。
///
/// # 错误
/// 余额写入失败时返回错误。
async fn ensure_or_create_balance(
    db: &Database,
    session: &mut mongodb::ClientSession,
    warehouse_id: &entities::ids::WarehouseId,
    sku_id: &entities::ids::SkuId,
    quantity: entities::money::Quantity,
) -> Result<String> {
    if let Some(balance) = db
        .stock_balances()
        .find_by_dimensions(warehouse_id, sku_id, session)
        .await?
    {
        if !db
            .stock_balances()
            .increase_on_hand(&balance.base.id, quantity, session)
            .await?
        {
            return Err(Error::BusinessLogicError("库存余额行不存在".to_string()));
        }
        return Ok(balance.base.id);
    }
    let balance = StockBalance::new(
        StockBalanceId::new(next_id()),
        StockBalanceData {
            warehouse_id: warehouse_id.clone(),
            sku_id: sku_id.clone(),
            on_hand_quantity: quantity,
            reserved_quantity: Quantity::from_str("0").map_err(Error::Logic)?,
            available_quantity: quantity,
            last_movement_id: None,
        },
    )?;
    db.stock_balances().create(&balance, session).await?;
    Ok(balance.base.id)
}

/// 沿采购销售分配自动建立销售预占（§8.2 第 1 条，位于调用方事务内）。
///
/// 预占数量按「分配数量 / 采购行数量」比例分摊本次合格入库，最后一个分配
/// 吸收舍入尾差；每个（入库行, 分配）的建立动作唯一由唯一索引保证。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `receipt` - 入库单表头
/// * `line` - 入库行
/// * `revision_line` - 采购生效版本行
/// * `sku_id` - SKU
/// * `balance_id` - 余额主键
/// * `occurred_at` - 过账业务时间
///
/// # 返回
/// 无返回值；写入失败时返回错误。
///
/// # 错误
/// 分配缺失/无销售归属、可用量不足或写入失败时返回错误。
async fn establish_reservations(
    db: &Database,
    session: &mut mongodb::ClientSession,
    receipt: &PurchaseReceipt,
    line: &PurchaseReceiptLine,
    revision_line: &entities::purchase_order::PurchaseOrderRevisionLine,
    sku_id: &entities::ids::SkuId,
    balance_id: &str,
) -> Result<()> {
    let allocations = db
        .purchase_line_sales_allocations()
        .find_by_purchase_revision_line_ids(
            std::slice::from_ref(&line.purchase_order_revision_line_id),
            session,
        )
        .await?;
    if allocations.is_empty() {
        return Ok(());
    }
    let total = revision_line
        .quantity
        .ok_or_else(|| Error::BusinessLogicError("采购明细缺少数量".to_string()))?;
    let allocation_quantities: Vec<Quantity> = allocations
        .iter()
        .map(|allocation| allocation.allocated_quantity)
        .collect();
    let shares = reservation_shares(line.qualified_quantity, &allocation_quantities, total)?;
    let sales_revision_line_ids: Vec<SalesOrderRevisionLineId> = allocations
        .iter()
        .map(|allocation| allocation.sales_order_revision_line_id.clone())
        .collect();
    let sales_revision_line_ids_str: Vec<String> =
        sales_revision_line_ids.iter().map(|id| id.to_string()).collect();
    let sales_revision_lines = db
        .sales_order_revision_lines()
        .find_many(doc! { "id": { "$in": sales_revision_line_ids_str } }, session)
        .await?;
    for (index, quantity) in shares.into_iter().enumerate() {
        let allocation = &allocations[index];
        let sales_line_id = sales_revision_lines
            .iter()
            .find(|sales_line| sales_line.base.id == allocation.sales_order_revision_line_id.to_string())
            .map(|sales_line| sales_line.sales_order_line_id.clone())
            .ok_or_else(|| Error::BusinessLogicError("采购销售分配缺少销售明细归属".to_string()))?;
        let reservation = StockReservation::new(
            StockReservationId::new(next_id()),
            StockReservationData {
                warehouse_id: receipt.warehouse_id.clone(),
                sku_id: sku_id.clone(),
                sales_order_line_id: sales_line_id,
                purchase_line_sales_allocation_id: allocation.base.id.clone().into(),
                source_receipt_line_id: line.base.id.clone().into(),
                reserved_quantity: quantity,
                consumed_quantity: Quantity::from_str("0").map_err(Error::Logic)?,
                released_quantity: Quantity::from_str("0").map_err(Error::Logic)?,
                status: ReservationStatus::Active,
            },
        )?;
        db.stock_reservations().create(&reservation, session).await?;
        if !db
            .stock_balances()
            .reserve_quantity(balance_id, quantity, session)
            .await?
        {
            return Err(Error::BusinessLogicError(
                "可用库存不足，无法建立销售预占".to_string(),
            ));
        }
        let entry = StockReservationEntry::new(
            StockReservationEntryId::new(next_id()),
            StockReservationEntryData {
                reservation_id: reservation.base.id.clone().into(),
                entry_type: ReservationEntryType::Establish,
                quantity,
                source_document_id: receipt.base.id.clone(),
            },
        )?;
        db.stock_reservation_entries().create(&entry, session).await?;
    }
    Ok(())
}

/// 按分配比例分摊本次合格入库数量（最后一个分配吸收舍入尾差）。
///
/// # 参数
/// * `qualified` - 本次合格入库数量
/// * `allocation_quantities` - 各采购销售分配的分配数量（按序分摊）
/// * `total` - 采购行总数
///
/// # 返回
/// 返回各分配的预占数量列表（与入参一一对应），合计恒等于 `qualified`。
///
/// # 错误
/// 采购行总数为 0 时返回 `BusinessLogicError`。
fn reservation_shares(
    qualified: Quantity,
    allocation_quantities: &[Quantity],
    total: Quantity,
) -> Result<Vec<Quantity>> {
    let zero = Quantity::from_str("0").unwrap();
    let total_dec = total.to_decimal();
    if total_dec <= zero.to_decimal() {
        return Err(Error::BusinessLogicError("采购明细数量必须为正数".to_string()));
    }
    let qualified_dec = qualified.to_decimal();
    let mut assigned = zero.to_decimal();
    let mut shares = Vec::with_capacity(allocation_quantities.len());
    for (index, allocation_quantity) in allocation_quantities.iter().enumerate() {
        let share = if index + 1 == allocation_quantities.len() {
            qualified_dec - assigned
        } else {
            (qualified_dec * allocation_quantity.to_decimal() / total_dec).round_dp(6)
        };
        assigned += share;
        shares.push(Quantity::try_from(share).map_err(|error| Error::BusinessLogicError(error.to_string()))?);
    }
    Ok(shares)
}

/// 过账单条仓发行（预占消耗 + 出库流水 + 余额，位于调用方事务内）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `delivery` - 发货单表头
/// * `line` - 发货行
/// * `occurred_at` - 过账业务时间
/// * `actor` - 审计操作人（记录人身份）
///
/// # 返回
/// 无返回值；写入失败时返回错误。
///
/// # 错误
/// 预占不存在/归属不符/数量不足、余额缺失或写入失败时返回错误。
async fn post_warehouse_ship_line(
    db: &Database,
    session: &mut mongodb::ClientSession,
    delivery: &Delivery,
    line: &DeliveryLine,
    occurred_at: &Instant,
    actor: &AuditActor,
) -> Result<()> {
    let reservation_id = line
        .stock_reservation_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("仓发必须消耗有效预占".to_string()))?;
    let reservation = db
        .stock_reservations()
        .find_by_id(reservation_id.as_ref(), session)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("库存预占不存在".to_string()))?;
    if reservation.sales_order_line_id != line.sales_order_line_id {
        return Err(Error::BusinessLogicError(
            "库存预占不属于本销售明细，不能消耗".to_string(),
        ));
    }
    if reservation.reserved_quantity.to_decimal() < line.quantity.to_decimal() {
        return Err(Error::BusinessLogicError(
            "为这单留的货不足，无法发货".to_string(),
        ));
    }
    let warehouse_id = delivery
        .warehouse_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("仓发缺少发货仓".to_string()))?;
    let balance = db
        .stock_balances()
        .find_by_dimensions(&warehouse_id, &reservation.sku_id, session)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("库存余额不存在，无法发货".to_string()))?;
    if !db
        .stock_reservations()
        .consume_quantity(&reservation.base.id, line.quantity, session)
        .await?
    {
        return Err(Error::BusinessLogicError(
            "预占数量不足或状态不符，无法消耗".to_string(),
        ));
    }
    let entry = StockReservationEntry::new(
        StockReservationEntryId::new(next_id()),
        StockReservationEntryData {
            reservation_id: reservation.base.id.clone().into(),
            entry_type: ReservationEntryType::Consume,
            quantity: line.quantity,
            source_document_id: delivery.base.id.clone(),
        },
    )?;
    db.stock_reservation_entries().create(&entry, session).await?;
    if !db
        .stock_balances()
        .deduct_available(&balance.base.id, line.quantity, session)
        .await?
    {
        return Err(Error::BusinessLogicError("可用库存不足，无法发货".to_string()));
    }
    if !db
        .stock_balances()
        .release_reserved(&balance.base.id, line.quantity, session)
        .await?
    {
        return Err(Error::BusinessLogicError("预占余额不足，无法发货".to_string()));
    }
    let movement = StockMovement::new(
        StockMovementId::new(next_id()),
        StockMovementData {
            warehouse_id,
            sku_id: reservation.sku_id,
            movement_type: MovementType::WarehouseShipOut,
            direction: MovementDirection::Decrease,
            quantity: line.quantity,
            source_document_id: delivery.base.id.clone(),
            source_line_id: Some(line.base.id.clone()),
            reversal_of_movement_id: None,
            fact_no: next_id(),
            occurred_at: *occurred_at,
            recorded_at: *occurred_at,
            recorded_by: actor.id().to_string(),
            source_type: SourceType::Erp,
            source_reference: None,
            reason_code: None,
            reason_text: None,
        },
    )?;
    db.stock_movements().create(&movement, session).await?;
    Ok(())
}

/// 校验过账分配与草稿验收行一一对应且数量一致（§8.2 第 5 条「锁定验收行」）。
///
/// # 参数
/// * `lines` - 草稿验收行
/// * `inputs` - 过账请求行
///
/// # 返回
/// 一致返回 `Ok(())`。
///
/// # 错误
/// 行集合不一致时返回 `ValidationError`。
fn ensure_post_lines_match(
    lines: &[CustomerAcceptanceLine],
    inputs: &[PostAcceptanceLineInput],
) -> Result<()> {
    if lines.len() != inputs.len() {
        return Err(Error::ValidationError("过账分配与验收行数量不一致".to_string()));
    }
    for line in lines {
        let input = inputs
            .iter()
            .find(|input| input.sales_order_line_id == line.sales_order_line_id)
            .ok_or_else(|| Error::ValidationError("过账分配缺少验收行".to_string()))?;
        if input.allocations.is_empty() {
            return Err(Error::ValidationError("验收行缺少履约分配".to_string()));
        }
    }
    Ok(())
}

/// 校验验收行分配守恒（§8.2 第 5 条：分配合计等于通过数量）。
///
/// # 参数
/// * `line` - 草稿验收行
/// * `allocations` - 过账分配
///
/// # 返回
/// 守恒返回 `Ok(())`。
///
/// # 错误
/// 分配合计不等于通过数量时返回 `ValidationError`。
fn ensure_line_allocations_conserved(
    line: &CustomerAcceptanceLine,
    allocations: &[AcceptanceAllocationInput],
) -> Result<()> {
    let mut total = Quantity::from_str("0").unwrap();
    for allocation in allocations {
        total = Quantity::try_from(total.to_decimal() + allocation.allocated_quantity.to_decimal())
            .map_err(Error::Logic)?;
    }
    if total != line.accepted_quantity {
        return Err(Error::ValidationError(
            "验收行分配合计必须等于通过数量".to_string(),
        ));
    }
    Ok(())
}

/// 写入单条验收履约分配并校验净验收上限（§8.2 第 5 条，位于调用方事务内）。
///
/// 校验履约事实存在、属于同一销售明细且处于有效状态；净验收（既有 APPLY −
/// REVERSE + 本次）不得超过该事实的净成功履约数量。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `line_id` - 验收行主键
/// * `allocation` - 分配输入
/// * `acceptance_line` - 验收行（销售明细归属）
/// * `sales_order_id` - 销售单（校验事实归属）
///
/// # 返回
/// 无返回值；写入失败时返回错误。
///
/// # 错误
/// 事实不存在/状态无效/归属不符或净验收超上限时返回 `ValidationError`。
async fn write_acceptance_allocation(
    db: &Database,
    session: &mut mongodb::ClientSession,
    line_id: &str,
    allocation: &AcceptanceAllocationInput,
    acceptance_line: &CustomerAcceptanceLine,
    sales_order_id: &entities::ids::SalesOrderId,
) -> Result<()> {
    let net_successful = load_fulfillment_fact(
        db,
        session,
        allocation.fulfillment_fact_type,
        &allocation.fulfillment_line_id,
        sales_order_id,
    )
    .await?;
    if acceptance_line.sales_order_line_id.to_string()
        != fact_sales_line(
            db,
            session,
            allocation.fulfillment_fact_type,
            &allocation.fulfillment_line_id,
        )
        .await?
    {
        return Err(Error::ValidationError("履约事实不属于本验收明细".to_string()));
    }
    let existing = db
        .fulfillment()
        .allocations_by_fulfillment_fact(
            allocation.fulfillment_fact_type,
            std::slice::from_ref(&allocation.fulfillment_line_id),
            session,
        )
        .await?;
    let mut net_accepted = Quantity::from_str("0").unwrap();
    for existing in existing {
        net_accepted = match existing.allocation_action {
            AllocationAction::Apply => {
                Quantity::try_from(net_accepted.to_decimal() + existing.allocated_quantity.to_decimal())
                    .map_err(Error::Logic)?
            }
            AllocationAction::Reverse => {
                Quantity::try_from(net_accepted.to_decimal() - existing.allocated_quantity.to_decimal())
                    .map_err(Error::Logic)?
            }
        };
    }
    if net_accepted.to_decimal() + allocation.allocated_quantity.to_decimal() > net_successful.to_decimal() {
        return Err(Error::ValidationError(
            "履约事实的净验收数量超过其净成功履约数量".to_string(),
        ));
    }
    let record = AcceptanceFulfillmentAllocation::new(
        AcceptanceFulfillmentAllocationId::new(next_id()),
        AcceptanceFulfillmentAllocationData {
            customer_acceptance_line_id: line_id.to_string().into(),
            fulfillment_fact_type: allocation.fulfillment_fact_type,
            fulfillment_line_id: allocation.fulfillment_line_id.clone(),
            allocation_action: AllocationAction::Apply,
            allocated_quantity: allocation.allocated_quantity,
            reverses_allocation_id: None,
        },
    )?;
    db.acceptance_fulfillment_allocations()
        .create(&record, session)
        .await?;
    Ok(())
}

/// 加载履约事实的净成功数量并校验事实存在与状态有效。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `fact_type` - 履约事实类型
/// * `fact_id` - 履约事实行主键
/// * `sales_order_id` - 销售单（校验归属）
///
/// # 返回
/// 返回净成功履约数量。
///
/// # 错误
/// 事实不存在或状态无效时返回 `ValidationError`。
async fn load_fulfillment_fact(
    db: &Database,
    session: &mut mongodb::ClientSession,
    fact_type: FulfillmentFactType,
    fact_id: &str,
    sales_order_id: &entities::ids::SalesOrderId,
) -> Result<Quantity> {
    match fact_type {
        FulfillmentFactType::Delivery => {
            let line = db
                .delivery_lines()
                .find_by_id(fact_id, session)
                .await?
                .ok_or_else(|| Error::ValidationError("发货事实不存在".to_string()))?;
            let delivery = db
                .deliveries()
                .find_by_id(line.delivery_id.as_ref(), session)
                .await?
                .ok_or_else(|| Error::ValidationError("发货单不存在".to_string()))?;
            if delivery.sales_order_id != *sales_order_id
                || !matches!(delivery.status, DeliveryState::Shipped | DeliveryState::Signed)
            {
                return Err(Error::ValidationError(
                    "发货事实不属于本销售单或状态无效".to_string(),
                ));
            }
            Ok(line.quantity)
        }
        FulfillmentFactType::ElectronicDelivery => {
            let record = db
                .electronic_deliveries()
                .find_by_id(fact_id, session)
                .await?
                .ok_or_else(|| Error::ValidationError("电子交付事实不存在".to_string()))?;
            if record.status != ElectronicDeliveryState::Confirmed {
                return Err(Error::ValidationError("电子交付事实状态无效".to_string()));
            }
            Ok(record.quantity)
        }
        FulfillmentFactType::ServiceFulfillment => {
            let record = db
                .service_fulfillments()
                .find_by_id(fact_id, session)
                .await?
                .ok_or_else(|| Error::ValidationError("服务履约事实不存在".to_string()))?;
            if record.status != ServiceFulfillmentState::Confirmed {
                return Err(Error::ValidationError("服务履约事实状态无效".to_string()));
            }
            Ok(record.quantity)
        }
    }
}

/// 取履约事实所属的销售稳定明细。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `fact_type` - 履约事实类型
/// * `fact_id` - 履约事实行主键
///
/// # 返回
/// 返回销售稳定明细 ID 字符串。
///
/// # 错误
/// 事实不存在时返回 `ValidationError`。
async fn fact_sales_line(
    db: &Database,
    session: &mut mongodb::ClientSession,
    fact_type: FulfillmentFactType,
    fact_id: &str,
) -> Result<String> {
    match fact_type {
        FulfillmentFactType::Delivery => {
            let line = db
                .delivery_lines()
                .find_by_id(fact_id, session)
                .await?
                .ok_or_else(|| Error::ValidationError("发货事实不存在".to_string()))?;
            Ok(line.sales_order_line_id.to_string())
        }
        FulfillmentFactType::ElectronicDelivery => {
            let record = db
                .electronic_deliveries()
                .find_by_id(fact_id, session)
                .await?
                .ok_or_else(|| Error::ValidationError("电子交付事实不存在".to_string()))?;
            Ok(record.sales_order_line_id.to_string())
        }
        FulfillmentFactType::ServiceFulfillment => {
            let record = db
                .service_fulfillments()
                .find_by_id(fact_id, session)
                .await?
                .ok_or_else(|| Error::ValidationError("服务履约事实不存在".to_string()))?;
            Ok(record.sales_order_line_id.to_string())
        }
    }
}

/// 按生效版本行计算采购履约进度（全部收满为已完成，否则部分执行）。
///
/// # 参数
/// * `revision_lines` - 采购生效版本行
/// * `received` - 累计有效收货（按采购版本行分组，含本次）
///
/// # 返回
/// 返回履约进度。
fn compute_po_fulfillment_progress(
    revision_lines: &[entities::purchase_order::PurchaseOrderRevisionLine],
    received: &HashMap<String, Quantity>,
) -> ProgressStatus {
    let zero = Quantity::from_str("0").unwrap();
    let mut total = zero.to_decimal();
    for line in revision_lines {
        if let Some(quantity) = line.quantity {
            total += quantity.to_decimal();
        }
    }
    let mut received_total = zero.to_decimal();
    for line in revision_lines {
        if let Some(quantity) = received.get(&line.base.id) {
            received_total += quantity.to_decimal();
        }
    }
    if total > zero.to_decimal() && received_total >= total {
        ProgressStatus::Completed
    } else {
        ProgressStatus::Partial
    }
}

/// 构建验收工作台分组（销售行 + 可验收事实 + 净数量守恒计算）。
///
/// # 参数
/// * `revision_lines` - 销售版本公共行
/// * `goods_service_lines` - 实物及服务行（数量/单位）
/// * `deliveries` - 有效发货单
/// * `delivery_lines` - 发货行
/// * `electronic` - 已确认电子交付
/// * `service` - 已确认服务履约
/// * `delivery_allocations` - 发货事实的验收分配
/// * `electronic_allocations` - 电子交付事实的验收分配
/// * `service_allocations` - 服务履约事实的验收分配
///
/// # 返回
/// 返回按销售稳定明细分组的工作台视图。
///
/// 事实/分配入参由数据模型 §6.7 固定为三类来源，字段不可压缩。
#[allow(clippy::too_many_arguments)]
fn build_eligibility_groups(
    revision_lines: &[entities::sales_order::SalesOrderRevisionLine],
    goods_service_lines: &[entities::sales_order::SalesOrderGoodsServiceLineRevision],
    deliveries: &[Delivery],
    delivery_lines: &[DeliveryLine],
    electronic: &[ElectronicDelivery],
    service: &[ServiceFulfillment],
    delivery_allocations: &[AcceptanceFulfillmentAllocation],
    electronic_allocations: &[AcceptanceFulfillmentAllocation],
    service_allocations: &[AcceptanceFulfillmentAllocation],
) -> Vec<AcceptanceSalesLineGroupView> {
    let mut groups: HashMap<String, AcceptanceSalesLineGroupView> = HashMap::new();
    for line in revision_lines {
        let goods = goods_service_lines
            .iter()
            .find(|goods| goods.revision_line_id.to_string() == line.base.id);
        groups.insert(
            line.sales_order_line_id.to_string(),
            AcceptanceSalesLineGroupView {
                sales_order_line_id: line.sales_order_line_id.to_string(),
                line_no: line.line_no,
                item_snapshot: line.item_name_snapshot.clone(),
                unit_code: goods.map(|goods| goods.base_unit_code.clone()),
                required_quantity: goods
                    .map(|goods| goods.quantity)
                    .unwrap_or_else(|| Quantity::from_str("0").unwrap()),
                net_accepted_quantity: Quantity::from_str("0").unwrap(),
                fulfillment_facts: Vec::new(),
            },
        );
    }
    let delivery_by_id: HashMap<String, &Delivery> = deliveries
        .iter()
        .map(|delivery| (delivery.base.id.clone(), delivery))
        .collect();
    for line in delivery_lines {
        let delivery = delivery_by_id.get(line.delivery_id.as_ref());
        let allocations = net_allocation_quantity(
            delivery_allocations,
            &line.base.id,
            Quantity::from_str("0").unwrap(),
        );
        let line_id = line.sales_order_line_id.to_string();
        let item_snapshot = group_item_snapshot(&groups, &line_id);
        let unit_code = group_unit_code(&groups, &line_id);
        push_fact(
            &mut groups,
            &line_id,
            EligibleFulfillmentFactView {
                fulfillment_line_id: line.base.id.clone(),
                fulfillment_fact_type: FulfillmentFactType::Delivery,
                fulfillment_no: delivery
                    .map(|delivery| delivery.delivery_no.clone())
                    .unwrap_or_default(),
                sales_order_line_id: line_id.clone(),
                line_no: line.line_no,
                item_snapshot,
                unit_code,
                occurred_at: delivery
                    .and_then(|delivery| delivery.shipped_at)
                    .map(|instant| instant.unix_secs())
                    .unwrap_or_default(),
                net_successful_quantity: line.quantity,
                net_accepted_allocated_quantity: allocations,
                eligible_quantity: Quantity::try_from(line.quantity.to_decimal() - allocations.to_decimal())
                    .unwrap_or_else(|_| Quantity::from_str("0").unwrap()),
                carrier: delivery.and_then(|delivery| delivery.carrier.clone()),
                tracking_no: delivery.and_then(|delivery| delivery.tracking_no.clone()),
            },
        );
    }
    for record in electronic {
        let allocations = net_allocation_quantity(
            electronic_allocations,
            &record.base.id,
            Quantity::from_str("0").unwrap(),
        );
        let line_id = record.sales_order_line_id.to_string();
        let line_no = group_line_no(&groups, &line_id);
        let item_snapshot = group_item_snapshot(&groups, &line_id);
        let unit_code = group_unit_code(&groups, &line_id);
        push_fact(
            &mut groups,
            &line_id,
            EligibleFulfillmentFactView {
                fulfillment_line_id: record.base.id.clone(),
                fulfillment_fact_type: FulfillmentFactType::ElectronicDelivery,
                fulfillment_no: record.fulfillment_no.clone(),
                sales_order_line_id: line_id.clone(),
                line_no,
                item_snapshot,
                unit_code,
                occurred_at: record.fact.occurred_at.unix_secs(),
                net_successful_quantity: record.quantity,
                net_accepted_allocated_quantity: allocations,
                eligible_quantity: Quantity::try_from(
                    record.quantity.to_decimal() - allocations.to_decimal(),
                )
                .unwrap_or_else(|_| Quantity::from_str("0").unwrap()),
                carrier: None,
                tracking_no: None,
            },
        );
    }
    for record in service {
        let allocations = net_allocation_quantity(
            service_allocations,
            &record.base.id,
            Quantity::from_str("0").unwrap(),
        );
        let line_id = record.sales_order_line_id.to_string();
        let line_no = group_line_no(&groups, &line_id);
        let item_snapshot = group_item_snapshot(&groups, &line_id);
        let unit_code = group_unit_code(&groups, &line_id);
        push_fact(
            &mut groups,
            &line_id,
            EligibleFulfillmentFactView {
                fulfillment_line_id: record.base.id.clone(),
                fulfillment_fact_type: FulfillmentFactType::ServiceFulfillment,
                fulfillment_no: record.fulfillment_no.clone(),
                sales_order_line_id: line_id.clone(),
                line_no,
                item_snapshot,
                unit_code,
                occurred_at: record.fact.occurred_at.unix_secs(),
                net_successful_quantity: record.quantity,
                net_accepted_allocated_quantity: allocations,
                eligible_quantity: Quantity::try_from(
                    record.quantity.to_decimal() - allocations.to_decimal(),
                )
                .unwrap_or_else(|_| Quantity::from_str("0").unwrap()),
                carrier: None,
                tracking_no: None,
            },
        );
    }
    let mut groups: Vec<AcceptanceSalesLineGroupView> = groups.into_values().collect();
    groups.sort_by_key(|group| group.line_no);
    groups
}

/// 计算履约事实的净验收分配数量（`APPLY − REVERSE`，正数方向）。
///
/// # 参数
/// * `allocations` - 该事实的全部验收分配
/// * `fulfillment_line_id` - 履约事实行主键
/// * `initial` - 初始值（零）
///
/// # 返回
/// 返回净验收分配数量。
fn net_allocation_quantity(
    allocations: &[AcceptanceFulfillmentAllocation],
    fulfillment_line_id: &str,
    initial: Quantity,
) -> Quantity {
    let mut net = initial;
    for allocation in allocations {
        if allocation.fulfillment_line_id != fulfillment_line_id {
            continue;
        }
        net = match allocation.allocation_action {
            AllocationAction::Apply => {
                Quantity::try_from(net.to_decimal() + allocation.allocated_quantity.to_decimal())
                    .unwrap_or_else(|_| Quantity::from_str("0").unwrap())
            }
            AllocationAction::Reverse => {
                Quantity::try_from(net.to_decimal() - allocation.allocated_quantity.to_decimal())
                    .unwrap_or_else(|_| Quantity::from_str("0").unwrap())
            }
        };
    }
    net
}

/// 把可验收事实并入对应销售行分组（按销售稳定明细）。
///
/// # 参数
/// * `groups` - 分组映射（就地修改）
/// * `sales_order_line_id` - 销售稳定明细
/// * `fact` - 可验收事实
fn push_fact(
    groups: &mut HashMap<String, AcceptanceSalesLineGroupView>,
    sales_order_line_id: &str,
    fact: EligibleFulfillmentFactView,
) {
    if let Some(group) = groups.get_mut(sales_order_line_id) {
        group.fulfillment_facts.push(fact);
    }
}

/// 取分组行号。
fn group_line_no(groups: &HashMap<String, AcceptanceSalesLineGroupView>, line_id: &str) -> u32 {
    groups.get(line_id).map(|group| group.line_no).unwrap_or_default()
}

/// 取分组品名快照。
fn group_item_snapshot(groups: &HashMap<String, AcceptanceSalesLineGroupView>, line_id: &str) -> String {
    groups
        .get(line_id)
        .map(|group| group.item_snapshot.clone())
        .unwrap_or_default()
}

/// 取分组单位快照。
fn group_unit_code(groups: &HashMap<String, AcceptanceSalesLineGroupView>, line_id: &str) -> Option<String> {
    groups.get(line_id).and_then(|group| group.unit_code.clone())
}

/// 构建入库行实体集合（行号从 1 递增，质量结果按数量派生）。
///
/// # 参数
/// * `receipt_id` - 入库单主键
/// * `inputs` - 行输入
///
/// # 返回
/// 返回行实体集合。
///
/// # 错误
/// 行数量约束不合法时返回错误（实体构造）。
fn build_receipt_lines(
    receipt_id: &PurchaseReceiptId,
    inputs: &[PurchaseReceiptLineInput],
) -> Result<Vec<PurchaseReceiptLine>> {
    let mut lines = Vec::with_capacity(inputs.len());
    for (index, input) in inputs.iter().enumerate() {
        let line_no = index as u32 + 1;
        let quality_result = derive_quality_result(input);
        lines.push(
            PurchaseReceiptLine::new(
                PurchaseReceiptLineId::new(next_id()),
                PurchaseReceiptLineData {
                    purchase_receipt_id: receipt_id.clone(),
                    line_no,
                    purchase_order_revision_line_id: input.purchase_order_revision_line_id.clone(),
                    received_quantity: input.received_quantity,
                    qualified_quantity: input.qualified_quantity,
                    rejected_quantity: input.rejected_quantity,
                    quality_result,
                },
            )
            .map_err(Error::Logic)?,
        );
    }
    Ok(lines)
}

/// 按合格/到货数量派生质量结果（§6.7：全部合格/全部不合格/部分合格）。
///
/// # 参数
/// * `input` - 行输入
///
/// # 返回
/// 返回质量结果。
fn derive_quality_result(input: &PurchaseReceiptLineInput) -> QualityResult {
    let zero = Quantity::from_str("0").unwrap();
    let qualified = input.qualified_quantity.to_decimal();
    let rejected = input.rejected_quantity.to_decimal();
    if rejected <= zero.to_decimal() {
        QualityResult::Passed
    } else if qualified <= zero.to_decimal() {
        QualityResult::Rejected
    } else {
        QualityResult::Partial
    }
}

/// 构建发货行实体集合（行号从 1 递增）。
///
/// # 参数
/// * `delivery_id` - 发货单主键
/// * `delivery_type` - 发货类型（决定行级归属校验）
/// * `inputs` - 行输入
///
/// # 返回
/// 返回行实体集合。
///
/// # 错误
/// 行归属与发货类型不一致或数量非正时返回错误（实体构造）。
fn build_delivery_lines(
    delivery_id: &DeliveryId,
    delivery_type: DeliveryType,
    inputs: &[DeliveryLineInput],
) -> Result<Vec<DeliveryLine>> {
    let mut lines = Vec::with_capacity(inputs.len());
    for (index, input) in inputs.iter().enumerate() {
        lines.push(
            DeliveryLine::new(
                DeliveryLineId::new(next_id()),
                DeliveryLineData {
                    delivery_id: delivery_id.clone(),
                    line_no: index as u32 + 1,
                    sales_order_line_id: input.sales_order_line_id.clone(),
                    quantity: input.quantity,
                    stock_reservation_id: input.stock_reservation_id.clone(),
                    purchase_line_sales_allocation_id: input.purchase_line_sales_allocation_id.clone(),
                },
                delivery_type,
            )
            .map_err(Error::Logic)?,
        );
    }
    Ok(lines)
}

/// 构建验收行实体集合（行号从 1 递增）。
///
/// # 参数
/// * `acceptance_id` - 验收单主键
/// * `inputs` - 行输入
///
/// # 返回
/// 返回行实体集合。
///
/// # 错误
/// 行数量为负或说明超长时返回错误（实体构造）。
fn build_acceptance_lines(
    acceptance_id: &CustomerAcceptanceId,
    inputs: &[AcceptanceLineInput],
) -> Result<Vec<CustomerAcceptanceLine>> {
    let mut lines = Vec::with_capacity(inputs.len());
    for (index, input) in inputs.iter().enumerate() {
        lines.push(
            CustomerAcceptanceLine::new(
                CustomerAcceptanceLineId::new(next_id()),
                CustomerAcceptanceLineData {
                    customer_acceptance_id: acceptance_id.clone(),
                    line_no: index as u32 + 1,
                    sales_order_line_id: input.sales_order_line_id.clone(),
                    accepted_quantity: input.accepted_quantity,
                    short_quantity: input.short_quantity,
                    rejected_quantity: input.rejected_quantity,
                    reason: input.reason.clone(),
                    evidence_attachment_id: None,
                },
            )
            .map_err(Error::Logic)?,
        );
    }
    Ok(lines)
}

// ---------------------------------------------------------------- view conversions

impl From<PurchaseReceipt> for PurchaseReceiptView {
    /// 从入库单实体构造视图。
    fn from(receipt: PurchaseReceipt) -> Self {
        Self {
            id: receipt.base.id,
            receipt_no: receipt.receipt_no,
            purchase_order_id: receipt.purchase_order_id.to_string(),
            warehouse_id: receipt.warehouse_id.to_string(),
            status: receipt.status,
            posted_at: receipt.posted_at.map(|instant| instant.unix_secs()),
            version: receipt.base.version,
            created_at: receipt.base.created_at,
        }
    }
}

impl From<PurchaseReceiptLine> for PurchaseReceiptLineView {
    /// 从入库行实体构造视图。
    fn from(line: PurchaseReceiptLine) -> Self {
        Self {
            id: line.base.id,
            line_no: line.line_no,
            purchase_order_revision_line_id: line.purchase_order_revision_line_id.to_string(),
            received_quantity: line.received_quantity,
            qualified_quantity: line.qualified_quantity,
            rejected_quantity: line.rejected_quantity,
            quality_result: line.quality_result,
        }
    }
}

impl From<Delivery> for DeliveryView {
    /// 从发货单实体构造视图。
    fn from(delivery: Delivery) -> Self {
        Self {
            id: delivery.base.id,
            delivery_no: delivery.delivery_no,
            delivery_type: delivery.delivery_type,
            sales_order_id: delivery.sales_order_id.to_string(),
            purchase_order_id: delivery.purchase_order_id.map(|id| id.to_string()),
            warehouse_id: delivery.warehouse_id.map(|id| id.to_string()),
            status: delivery.status,
            carrier: delivery.carrier,
            tracking_no: delivery.tracking_no,
            shipped_at: delivery.shipped_at.map(|instant| instant.unix_secs()),
            version: delivery.base.version,
            created_at: delivery.base.created_at,
        }
    }
}

impl From<DeliveryLine> for DeliveryLineView {
    /// 从发货行实体构造视图。
    fn from(line: DeliveryLine) -> Self {
        Self {
            id: line.base.id,
            line_no: line.line_no,
            sales_order_line_id: line.sales_order_line_id.to_string(),
            quantity: line.quantity,
            stock_reservation_id: line.stock_reservation_id.map(|id| id.to_string()),
            purchase_line_sales_allocation_id: line
                .purchase_line_sales_allocation_id
                .map(|id| id.to_string()),
        }
    }
}

impl From<ElectronicDelivery> for ElectronicDeliveryView {
    /// 从电子交付记录实体构造视图。
    fn from(record: ElectronicDelivery) -> Self {
        Self {
            id: record.base.id,
            fulfillment_no: record.fulfillment_no,
            sales_order_line_id: record.sales_order_line_id.to_string(),
            purchase_order_id: record.purchase_order_id.to_string(),
            purchase_line_sales_allocation_id: record.purchase_line_sales_allocation_id.to_string(),
            quantity: record.quantity,
            result: record.result,
            status: record.status,
            occurred_at: record.fact.occurred_at.unix_secs(),
            recorded_at: record.fact.recorded_at.unix_secs(),
            version: record.base.version,
        }
    }
}

impl From<ServiceFulfillment> for ServiceFulfillmentView {
    /// 从服务履约记录实体构造视图。
    fn from(record: ServiceFulfillment) -> Self {
        Self {
            id: record.base.id,
            fulfillment_no: record.fulfillment_no,
            sales_order_line_id: record.sales_order_line_id.to_string(),
            purchase_order_id: record.purchase_order_id.to_string(),
            purchase_line_sales_allocation_id: record.purchase_line_sales_allocation_id.to_string(),
            quantity: record.quantity,
            result: record.result,
            status: record.status,
            occurred_at: record.fact.occurred_at.unix_secs(),
            recorded_at: record.fact.recorded_at.unix_secs(),
            version: record.base.version,
        }
    }
}

impl From<CustomerAcceptance> for CustomerAcceptanceView {
    /// 从验收单实体构造视图。
    fn from(acceptance: CustomerAcceptance) -> Self {
        Self {
            id: acceptance.base.id,
            acceptance_no: acceptance.acceptance_no,
            sales_order_id: acceptance.sales_order_id.to_string(),
            accepted_at: acceptance.accepted_at.unix_secs(),
            result: acceptance.result,
            status: acceptance.status,
            reversal_of_acceptance_id: acceptance.reversal_of_acceptance_id.map(|id| id.to_string()),
            version: acceptance.base.version,
            created_at: acceptance.base.created_at,
        }
    }
}

impl From<CustomerAcceptanceLine> for CustomerAcceptanceLineView {
    /// 从验收行实体构造视图。
    fn from(line: CustomerAcceptanceLine) -> Self {
        Self {
            id: line.base.id,
            line_no: line.line_no,
            sales_order_line_id: line.sales_order_line_id.to_string(),
            accepted_quantity: line.accepted_quantity,
            short_quantity: line.short_quantity,
            rejected_quantity: line.rejected_quantity,
            reason: line.reason,
        }
    }
}

impl From<AcceptanceFulfillmentAllocation> for AcceptanceAllocationView {
    /// 从验收履约分配实体构造视图。
    fn from(allocation: AcceptanceFulfillmentAllocation) -> Self {
        Self {
            id: allocation.base.id,
            customer_acceptance_line_id: allocation.customer_acceptance_line_id.to_string(),
            fulfillment_fact_type: allocation.fulfillment_fact_type,
            fulfillment_line_id: allocation.fulfillment_line_id,
            allocation_action: allocation.allocation_action,
            allocated_quantity: allocation.allocated_quantity,
            reverses_allocation_id: allocation.reverses_allocation_id.map(|id| id.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_acceptance_lines, build_delivery_lines, build_receipt_lines, compute_po_fulfillment_progress,
        derive_quality_result, ensure_line_allocations_conserved, ensure_po_fulfillable, reservation_shares,
        AcceptanceAllocationInput, AcceptanceLineInput, DeliveryLineInput, PurchaseReceiptLineInput,
    };
    use entities::fulfillment::{DeliveryType, FulfillmentFactType, PurchaseReceiptLineData, QualityResult};
    use entities::ids::{
        CustomerAcceptanceId, CustomerAcceptanceLineId, DeliveryId, PurchaseLineSalesAllocationId,
        PurchaseOrderId, PurchaseOrderRevisionId, PurchaseOrderRevisionLineId, PurchaseReceiptId,
        SalesOrderId, SalesOrderLineId, SkuId, StockReservationId, SupplierAccountId,
    };
    use entities::money::Quantity;
    use entities::purchase_order::{
        FulfillmentResponsibility, ProgressStatus, PurchaseOrder, PurchaseOrderData, PurchaseType,
    };
    use std::collections::HashMap;
    use std::str::FromStr;

    fn passed_line() -> PurchaseReceiptLineInput {
        PurchaseReceiptLineInput {
            purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("porl-1"),
            received_quantity: Quantity::from_str("10").unwrap(),
            qualified_quantity: Quantity::from_str("10").unwrap(),
            rejected_quantity: Quantity::from_str("0").unwrap(),
        }
    }

    #[test]
    fn quality_result_is_derived_from_quantities() {
        assert_eq!(derive_quality_result(&passed_line()), QualityResult::Passed);
        let rejected = PurchaseReceiptLineInput {
            qualified_quantity: Quantity::from_str("0").unwrap(),
            rejected_quantity: Quantity::from_str("10").unwrap(),
            ..passed_line()
        };
        assert_eq!(derive_quality_result(&rejected), QualityResult::Rejected);
        let partial = PurchaseReceiptLineInput {
            qualified_quantity: Quantity::from_str("9").unwrap(),
            rejected_quantity: Quantity::from_str("1").unwrap(),
            ..passed_line()
        };
        assert_eq!(derive_quality_result(&partial), QualityResult::Partial);
    }

    #[test]
    fn receipt_lines_are_built_with_incrementing_line_no_and_validation() {
        let lines = build_receipt_lines(
            &PurchaseReceiptId::new("r-1"),
            &[
                passed_line(),
                PurchaseReceiptLineInput {
                    purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("porl-2"),
                    received_quantity: Quantity::from_str("5").unwrap(),
                    qualified_quantity: Quantity::from_str("5").unwrap(),
                    rejected_quantity: Quantity::from_str("0").unwrap(),
                },
            ],
        )
        .unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].line_no, 1);
        assert_eq!(lines[1].line_no, 2);
        let over_sum = PurchaseReceiptLineInput {
            qualified_quantity: Quantity::from_str("9.5").unwrap(),
            rejected_quantity: Quantity::from_str("1").unwrap(),
            ..passed_line()
        };
        assert!(build_receipt_lines(&PurchaseReceiptId::new("r-2"), &[over_sum]).is_err());
        let _ = PurchaseReceiptLineData {
            purchase_receipt_id: PurchaseReceiptId::new("r-1"),
            line_no: 1,
            purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("porl-1"),
            received_quantity: Quantity::from_str("10").unwrap(),
            qualified_quantity: Quantity::from_str("9").unwrap(),
            rejected_quantity: Quantity::from_str("1").unwrap(),
            quality_result: QualityResult::Partial,
        };
    }

    #[test]
    fn reservation_shares_split_proportionally_and_absorb_rounding() {
        let allocation_quantities = vec![
            Quantity::from_str("3").unwrap(),
            Quantity::from_str("2").unwrap(),
            Quantity::from_str("5").unwrap(),
        ];
        let shares = reservation_shares(
            Quantity::from_str("7.5").unwrap(),
            &allocation_quantities,
            Quantity::from_str("10").unwrap(),
        )
        .unwrap();
        let total: Quantity = shares
            .iter()
            .fold(Quantity::from_str("0").unwrap(), |acc, quantity| {
                Quantity::try_from(acc.to_decimal() + quantity.to_decimal()).unwrap()
            });
        assert_eq!(total, Quantity::from_str("7.5").unwrap());
        assert_eq!(shares[0], Quantity::from_str("2.25").unwrap());
        assert_eq!(shares[1], Quantity::from_str("1.5").unwrap());
        assert_eq!(shares[2], Quantity::from_str("3.75").unwrap());
    }

    #[test]
    fn acceptance_lines_conservation_is_checked() {
        let line = entities::fulfillment::CustomerAcceptanceLine::new(
            CustomerAcceptanceLineId::new("line-1"),
            entities::fulfillment::CustomerAcceptanceLineData {
                customer_acceptance_id: CustomerAcceptanceId::new("acc-1"),
                line_no: 1,
                sales_order_line_id: SalesOrderLineId::new("so-line-1"),
                accepted_quantity: Quantity::from_str("5").unwrap(),
                short_quantity: Quantity::from_str("0").unwrap(),
                rejected_quantity: Quantity::from_str("0").unwrap(),
                reason: None,
                evidence_attachment_id: None,
            },
        )
        .unwrap();
        let ok = vec![AcceptanceAllocationInput {
            fulfillment_line_id: "dl-1".to_string(),
            fulfillment_fact_type: FulfillmentFactType::Delivery,
            allocated_quantity: Quantity::from_str("5").unwrap(),
        }];
        assert!(ensure_line_allocations_conserved(&line, &ok).is_ok());
        let not_conserved = vec![AcceptanceAllocationInput {
            fulfillment_line_id: "dl-1".to_string(),
            fulfillment_fact_type: FulfillmentFactType::Delivery,
            allocated_quantity: Quantity::from_str("4").unwrap(),
        }];
        assert!(ensure_line_allocations_conserved(&line, &not_conserved).is_err());
    }

    #[test]
    fn delivery_lines_enforce_type_ownership() {
        let ok = build_delivery_lines(
            &DeliveryId::new("d-1"),
            DeliveryType::WarehouseShip,
            &[DeliveryLineInput {
                sales_order_line_id: SalesOrderLineId::new("so-line-1"),
                quantity: Quantity::from_str("2").unwrap(),
                stock_reservation_id: Some(StockReservationId::new("rsv-1")),
                purchase_line_sales_allocation_id: None,
            }],
        )
        .unwrap();
        assert_eq!(ok.len(), 1);
        let wrong = build_delivery_lines(
            &DeliveryId::new("d-2"),
            DeliveryType::WarehouseShip,
            &[DeliveryLineInput {
                sales_order_line_id: SalesOrderLineId::new("so-line-1"),
                quantity: Quantity::from_str("2").unwrap(),
                stock_reservation_id: None,
                purchase_line_sales_allocation_id: Some(PurchaseLineSalesAllocationId::new("a-1")),
            }],
        );
        assert!(wrong.is_err(), "仓发不得携带直发分配");
    }

    #[test]
    fn acceptance_lines_are_built_and_validated() {
        let lines = build_acceptance_lines(
            &CustomerAcceptanceId::new("acc-1"),
            &[AcceptanceLineInput {
                sales_order_line_id: SalesOrderLineId::new("so-line-1"),
                accepted_quantity: Quantity::from_str("9").unwrap(),
                short_quantity: Quantity::from_str("1").unwrap(),
                rejected_quantity: Quantity::from_str("0").unwrap(),
                reason: None,
                allocations: vec![],
            }],
        )
        .unwrap();
        assert_eq!(lines[0].accepted_quantity, Quantity::from_str("9").unwrap());
    }

    #[test]
    fn po_fulfillment_progress_tracks_completion() {
        let mut received = HashMap::new();
        received.insert("porl-1".to_string(), Quantity::from_str("6").unwrap());
        assert_eq!(
            compute_po_fulfillment_progress(&[], &received),
            ProgressStatus::Partial,
            "无商品行且无累计收货视为部分执行"
        );
        received.insert("rev-line-1".to_string(), Quantity::from_str("10").unwrap());
        let lines = vec![entities::purchase_order::PurchaseOrderRevisionLine::new(
            PurchaseOrderRevisionLineId::new("rev-line-1"),
            entities::purchase_order::PurchaseOrderRevisionLineData {
                purchase_order_revision_id: PurchaseOrderRevisionId::new("rev-1"),
                line_no: 1,
                line_type: entities::purchase_order::PurchaseLineType::ItemService,
                procurement_confirmation_line_id: Some(entities::ids::ProcurementConfirmationLineId::new(
                    "pcl-1",
                )),
                sku_id: Some(SkuId::new("sku-1")),
                sku_revision_id: None,
                product_name_snapshot: Some("商品".to_string()),
                specification_snapshot: Some("规格".to_string()),
                quantity: Some(Quantity::from_str("10").unwrap()),
                base_unit_code: Some("PCS".to_string()),
                unit_cost_gross: Some(entities::money::UnitPrice::from_str("10.0000").unwrap()),
                gross_amount: entities::money::Amount::from_str("100.00").unwrap(),
                net_amount: entities::money::Amount::from_str("87.00").unwrap(),
                tax_amount: entities::money::Amount::from_str("13.00").unwrap(),
                input_tax_rate: Some(entities::money::Rate::from_str("0.130000").unwrap()),
                expected_delivery_date: None,
            },
        )
        .unwrap()];
        assert_eq!(
            compute_po_fulfillment_progress(&lines, &received),
            ProgressStatus::Completed,
            "全部收满视为已完成"
        );
    }

    #[test]
    fn po_fulfillable_guards_status() {
        let po = PurchaseOrder::new(
            PurchaseOrderId::new("po-1"),
            PurchaseOrderData {
                purchase_no: "PO-1".to_string(),
                sales_order_id: SalesOrderId::new("so-1"),
                supplier_id: SupplierAccountId::new("sup-1"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "NET-30".to_string(),
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
            },
            "admin-1",
        )
        .unwrap();
        assert!(ensure_po_fulfillable(&po).is_err(), "草稿不可履约");
    }
}
