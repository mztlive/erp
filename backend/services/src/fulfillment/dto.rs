//! 域 D16 `fulfillment` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；数量一律十进制字符串
//! （`entities::money::Quantity` 自定义序列化）。页面：W06 客户验收、
//! W09 收货与发货/交付与代发。
//!
//! 履约对象快照（电子交付接收对象、服务地点）以不透明值传输：服务端用
//! `app.secret` 作为 HMAC 密钥计算查询指纹后落库；明文字段级加密由边界
//! （P4 前端或接入层）在传入前完成，P3 不引入新的加密原语（地基修订候选）。

use entities::fulfillment::{
    AcceptanceResult, AllocationAction, CustomerAcceptanceState, DeliveryState, DeliveryType,
    ElectronicDeliveryState, FulfillmentFactType, FulfillmentResult, PurchaseReceiptState,
    ServiceFulfillmentState,
};
use entities::ids::{
    FileAssetId, PurchaseLineSalesAllocationId, PurchaseOrderId, PurchaseOrderRevisionLineId, SalesOrderId,
    SalesOrderLineId, StockReservationId, WarehouseId,
};
use entities::money::Quantity;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{page_or_default, page_size_or_default};

/// 采购入库单列表允许的排序字段白名单（api-contract §4：Service 层校验，禁止任意字段透传）。
pub(crate) const PURCHASE_RECEIPT_SORT_FIELDS: &[&str] = &["created_at", "posted_at"];
/// 发货单列表允许的排序字段白名单。
pub(crate) const DELIVERY_SORT_FIELDS: &[&str] = &["created_at", "shipped_at"];
/// 电子交付列表允许的排序字段白名单。
pub(crate) const ELECTRONIC_DELIVERY_SORT_FIELDS: &[&str] = &["occurred_at", "recorded_at", "created_at"];
/// 服务履约列表允许的排序字段白名单。
pub(crate) const SERVICE_FULFILLMENT_SORT_FIELDS: &[&str] = &["occurred_at", "recorded_at", "created_at"];
/// 客户验收单列表允许的排序字段白名单。
pub(crate) const CUSTOMER_ACCEPTANCE_SORT_FIELDS: &[&str] = &["accepted_at", "created_at"];

/// 排序方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    /// 升序。
    Asc,
    /// 降序。
    Desc,
}

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验，`&'static str` 保证来源只可能是白名单）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。
///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `allowed_fields` - 白名单
///
/// # 返回
/// 返回 `(排序字段, 方向)`；未提供时默认 `("created_at", Desc)`。
///
/// # 错误
/// 字段不在白名单或方向不是 `asc`/`desc` 时返回 `ValidationError`。
pub(crate) fn normalize_sort(
    sort_by: &Option<String>,
    sort_dir: &Option<String>,
    allowed_fields: &'static [&'static str],
) -> Result<(&'static str, SortDir)> {
    let sort_by = match sort_by
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(field) => allowed_fields
            .iter()
            .find(|allowed| **allowed == field)
            .copied()
            .ok_or_else(|| Error::ValidationError(format!("不支持的排序字段: {field}")))?,
        None => "created_at",
    };
    let sort_dir = match sort_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some("asc") => SortDir::Asc,
        Some("desc") => SortDir::Desc,
        Some(other) => return Err(Error::ValidationError(format!("非法排序方向: {other}"))),
        None => SortDir::Desc,
    };
    Ok((sort_by, sort_dir))
}

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
#[derive(Debug, Clone, Serialize)]
pub struct PageView<T> {
    /// 当前页数据。
    pub items: Vec<T>,
    /// 满足筛选条件的总数（非当前页条数）。
    pub total: i64,
    /// 当前页码（1 起）。
    pub page: u64,
    /// 请求的分页大小。
    pub page_size: u32,
}

/// 校验文本去除首尾空白后非空（validator 的 `length(min=1)` 对纯空白字符串
/// 不生效，空单号需要按「空白视为空」拒绝，落入 HTTP 400）。
fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

// ---------------------------------------------------------------- purchase_receipt

/// 采购入库行输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseReceiptLineInput {
    /// 采购明细。
    pub purchase_order_revision_line_id: PurchaseOrderRevisionLineId,
    /// 到货数量。
    pub received_quantity: Quantity,
    /// 合格数量。
    pub qualified_quantity: Quantity,
    /// 不合格数量。
    pub rejected_quantity: Quantity,
}

/// 采购入库单创建请求（表头 + 行一次提交，初始状态为草稿）。
///
/// 客户端不得提交定义 ID 或审批人；未知字段失败关闭。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreatePurchaseReceiptRequest {
    /// 采购入库单号（全局唯一）。
    #[validate(custom(function = "non_blank", message = "采购入库单号不能为空"))]
    pub receipt_no: String,
    /// 来源采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 入库仓。
    pub warehouse_id: WarehouseId,
    /// 入库行（1–200 行）。
    #[validate(length(min = 1, max = 200, message = "入库行数必须在1-200之间"))]
    pub lines: Vec<PurchaseReceiptLineInput>,
}

/// 采购入库单更新请求（携带乐观锁版本；仅草稿可更新）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdatePurchaseReceiptRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 入库仓；缺省表示不修改。
    pub warehouse_id: Option<WarehouseId>,
}

/// 采购入库行视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseReceiptLineView {
    /// 实体主键。
    pub id: String,
    /// 稳定行号。
    pub line_no: u32,
    /// 采购明细。
    pub purchase_order_revision_line_id: String,
    /// 到货数量。
    pub received_quantity: Quantity,
    /// 合格数量。
    pub qualified_quantity: Quantity,
    /// 不合格数量。
    pub rejected_quantity: Quantity,
    /// 质量结果。
    pub quality_result: entities::fulfillment::QualityResult,
}

/// 采购入库单列表视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseReceiptView {
    /// 实体主键。
    pub id: String,
    /// 采购入库单号。
    pub receipt_no: String,
    /// 来源采购单。
    pub purchase_order_id: String,
    /// 入库仓。
    pub warehouse_id: String,
    /// 当前状态。
    pub status: PurchaseReceiptState,
    /// 入库过账时间（秒级时间戳）。
    pub posted_at: Option<i64>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 采购入库单详情视图（表头 + 行）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseReceiptDetailView {
    /// 表头。
    pub receipt: PurchaseReceiptView,
    /// 入库行。
    pub lines: Vec<PurchaseReceiptLineView>,
}

/// 采购入库单列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PurchaseReceiptListParams {
    /// 来源采购单筛选。
    pub purchase_order_id: Option<PurchaseOrderId>,
    /// 单据状态筛选。
    pub status: Option<PurchaseReceiptState>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`posted_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的采购入库单列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PurchaseReceiptListQuery {
    /// 来源采购单筛选。
    pub purchase_order_id: Option<PurchaseOrderId>,
    /// 单据状态筛选。
    pub status: Option<PurchaseReceiptState>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl PurchaseReceiptListParams {
    /// 归一化采购入库单列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<PurchaseReceiptListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, PURCHASE_RECEIPT_SORT_FIELDS)?;
        Ok(PurchaseReceiptListQuery {
            purchase_order_id: self.purchase_order_id.clone(),
            status: self.status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

// ----------------------------------------------------------------------- delivery

/// 发货行输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryLineInput {
    /// 销售稳定明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 发货数量。
    pub quantity: Quantity,
    /// 仓发消耗的预占；直发为空。
    pub stock_reservation_id: Option<StockReservationId>,
    /// 供应商直发必填的采购到销售分配；仓发为空。
    pub purchase_line_sales_allocation_id: Option<PurchaseLineSalesAllocationId>,
}

/// 发货单创建请求（表头 + 行一次提交，初始状态为草稿）。
///
/// 客户端不得提交定义 ID 或审批人；未知字段失败关闭。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateDeliveryRequest {
    /// 履约发货单号（全局唯一）。
    #[validate(custom(function = "non_blank", message = "发货单号不能为空"))]
    pub delivery_no: String,
    /// 发货类型（仓发/供应商直发，创建后不可修改）。
    pub delivery_type: DeliveryType,
    /// 销售单。
    pub sales_order_id: SalesOrderId,
    /// 供应商直发时的采购来源；仓发为空。
    pub purchase_order_id: Option<PurchaseOrderId>,
    /// 入库仓；仓发必填，直发为空。
    pub warehouse_id: Option<WarehouseId>,
    /// 物流承运方。
    pub carrier: Option<String>,
    /// 物流单号。
    pub tracking_no: Option<String>,
    /// 发货行（1–200 行）。
    #[validate(length(min = 1, max = 200, message = "发货行数必须在1-200之间"))]
    pub lines: Vec<DeliveryLineInput>,
}

/// 发货单更新请求（携带乐观锁版本；仅草稿可更新）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateDeliveryRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 物流承运方；缺省表示不修改。
    pub carrier: Option<String>,
    /// 物流单号；缺省表示不修改。
    pub tracking_no: Option<String>,
}

/// 发货行视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeliveryLineView {
    /// 实体主键。
    pub id: String,
    /// 稳定行号。
    pub line_no: u32,
    /// 销售稳定明细。
    pub sales_order_line_id: String,
    /// 发货数量。
    pub quantity: Quantity,
    /// 仓发消耗的预占。
    pub stock_reservation_id: Option<String>,
    /// 供应商直发的采购到销售分配。
    pub purchase_line_sales_allocation_id: Option<String>,
}

/// 发货单列表视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeliveryView {
    /// 实体主键。
    pub id: String,
    /// 履约发货单号。
    pub delivery_no: String,
    /// 发货类型。
    pub delivery_type: DeliveryType,
    /// 销售单。
    pub sales_order_id: String,
    /// 供应商直发时的采购来源。
    pub purchase_order_id: Option<String>,
    /// 仓发时的入库仓。
    pub warehouse_id: Option<String>,
    /// 当前状态。
    pub status: DeliveryState,
    /// 物流承运方。
    pub carrier: Option<String>,
    /// 物流单号。
    pub tracking_no: Option<String>,
    /// 发货时间（秒级时间戳）。
    pub shipped_at: Option<i64>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 发货单详情视图（表头 + 行）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeliveryDetailView {
    /// 表头。
    pub delivery: DeliveryView,
    /// 发货行。
    pub lines: Vec<DeliveryLineView>,
}

/// 发货单列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DeliveryListParams {
    /// 销售单筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 单据状态筛选。
    pub status: Option<DeliveryState>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`shipped_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的发货单列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeliveryListQuery {
    /// 销售单筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 单据状态筛选。
    pub status: Option<DeliveryState>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl DeliveryListParams {
    /// 归一化发货单列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<DeliveryListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, DELIVERY_SORT_FIELDS)?;
        Ok(DeliveryListQuery {
            sales_order_id: self.sales_order_id.clone(),
            status: self.status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

// ------------------------------------------------------------- electronic_delivery

/// 电子交付记录创建请求（初始状态为草稿，确认后不可覆盖）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateElectronicDeliveryRequest {
    /// 履约记录号（全局唯一）。
    #[validate(custom(function = "non_blank", message = "履约记录号不能为空"))]
    pub fulfillment_no: String,
    /// 销售责任明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 对应采购行到销售行的明确分配。
    pub purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId,
    /// 必要交付对象的加密/脱敏快照（不透明值，由边界生成）。
    #[validate(custom(function = "non_blank", message = "交付对象快照不能为空"))]
    pub recipient_snapshot: String,
    /// 交付数量。
    pub quantity: Quantity,
    /// 履约结果。
    pub result: FulfillmentResult,
    /// 实际交付时间（秒级时间戳）。
    pub occurred_at: i64,
    /// 业务凭证。
    pub evidence_attachment_id: Option<FileAssetId>,
}

/// 电子交付记录列表视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ElectronicDeliveryView {
    /// 实体主键。
    pub id: String,
    /// 履约记录号。
    pub fulfillment_no: String,
    /// 销售责任明细。
    pub sales_order_line_id: String,
    /// 采购单。
    pub purchase_order_id: String,
    /// 对应采购行到销售行的明确分配。
    pub purchase_line_sales_allocation_id: String,
    /// 交付数量。
    pub quantity: Quantity,
    /// 履约结果。
    pub result: FulfillmentResult,
    /// 当前状态。
    pub status: ElectronicDeliveryState,
    /// 实际交付时间（秒级时间戳）。
    pub occurred_at: i64,
    /// ERP 记录时间（秒级时间戳）。
    pub recorded_at: i64,
    /// 乐观锁版本。
    pub version: u64,
}

/// 电子交付记录列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ElectronicDeliveryListParams {
    /// 销售责任明细筛选。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 记录状态筛选。
    pub status: Option<ElectronicDeliveryState>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`occurred_at`/`recorded_at`/`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的电子交付记录列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ElectronicDeliveryListQuery {
    /// 销售责任明细筛选。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 记录状态筛选。
    pub status: Option<ElectronicDeliveryState>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl ElectronicDeliveryListParams {
    /// 归一化电子交付记录列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<ElectronicDeliveryListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, ELECTRONIC_DELIVERY_SORT_FIELDS)?;
        Ok(ElectronicDeliveryListQuery {
            sales_order_line_id: self.sales_order_line_id.clone(),
            status: self.status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

// ----------------------------------------------------------- service_fulfillment

/// 线下服务履约记录创建请求（初始状态为草稿，确认后不可覆盖）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateServiceFulfillmentRequest {
    /// 履约记录号（全局唯一）。
    #[validate(custom(function = "non_blank", message = "履约记录号不能为空"))]
    pub fulfillment_no: String,
    /// 销售责任明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 对应采购行到销售行的明确分配。
    pub purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId,
    /// 必要交付对象的加密/脱敏快照（不透明值，由边界生成）。
    #[validate(custom(function = "non_blank", message = "交付对象快照不能为空"))]
    pub recipient_snapshot: String,
    /// 服务数量。
    pub quantity: Quantity,
    /// 履约结果。
    pub result: FulfillmentResult,
    /// 服务地点加密/脱敏值（不透明值，由边界生成）。
    #[validate(custom(function = "non_blank", message = "服务地点不能为空"))]
    pub service_location: String,
    /// 服务开始时间（秒级时间戳）。
    pub service_started_at: Option<i64>,
    /// 服务结束时间（秒级时间戳）。
    pub service_ended_at: Option<i64>,
    /// 完成说明。
    pub completion_note: Option<String>,
    /// 实际服务时间（秒级时间戳）。
    pub occurred_at: i64,
    /// 业务凭证。
    pub evidence_attachment_id: Option<FileAssetId>,
}

/// 线下服务履约记录列表视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ServiceFulfillmentView {
    /// 实体主键。
    pub id: String,
    /// 履约记录号。
    pub fulfillment_no: String,
    /// 销售责任明细。
    pub sales_order_line_id: String,
    /// 采购单。
    pub purchase_order_id: String,
    /// 对应采购行到销售行的明确分配。
    pub purchase_line_sales_allocation_id: String,
    /// 服务数量。
    pub quantity: Quantity,
    /// 履约结果。
    pub result: FulfillmentResult,
    /// 当前状态。
    pub status: ServiceFulfillmentState,
    /// 实际服务时间（秒级时间戳）。
    pub occurred_at: i64,
    /// ERP 记录时间（秒级时间戳）。
    pub recorded_at: i64,
    /// 乐观锁版本。
    pub version: u64,
}

/// 线下服务履约记录列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ServiceFulfillmentListParams {
    /// 销售责任明细筛选。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 记录状态筛选。
    pub status: Option<ServiceFulfillmentState>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`occurred_at`/`recorded_at`/`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的线下服务履约记录列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServiceFulfillmentListQuery {
    /// 销售责任明细筛选。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 记录状态筛选。
    pub status: Option<ServiceFulfillmentState>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl ServiceFulfillmentListParams {
    /// 归一化线下服务履约记录列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<ServiceFulfillmentListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, SERVICE_FULFILLMENT_SORT_FIELDS)?;
        Ok(ServiceFulfillmentListQuery {
            sales_order_line_id: self.sales_order_line_id.clone(),
            status: self.status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

// ------------------------------------------------------------ customer_acceptance

/// 验收履约分配输入（验收行对履约事实的分配）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptanceAllocationInput {
    /// 履约事实行（发货行/电子交付/服务履约的主键，跨域多态引用）。
    pub fulfillment_line_id: String,
    /// 履约事实类型。
    pub fulfillment_fact_type: FulfillmentFactType,
    /// 正数验收数量。
    pub allocated_quantity: Quantity,
}

/// 客户验收行输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptanceLineInput {
    /// 验收明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 通过数量。
    pub accepted_quantity: Quantity,
    /// 短少数量。
    pub short_quantity: Quantity,
    /// 拒收数量。
    pub rejected_quantity: Quantity,
    /// 依据说明。
    pub reason: Option<String>,
    /// 对履约事实的分配（过账时按行守恒校验）。
    pub allocations: Vec<AcceptanceAllocationInput>,
}

/// 客户验收单创建请求（表头 + 行一次提交，初始状态为草稿）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateCustomerAcceptanceRequest {
    /// 客户验收单号（全局唯一）。
    #[validate(custom(function = "non_blank", message = "客户验收单号不能为空"))]
    pub acceptance_no: String,
    /// 销售单。
    pub sales_order_id: SalesOrderId,
    /// 验收时间（秒级时间戳）。
    pub accepted_at: i64,
    /// 验收结果。
    pub result: AcceptanceResult,
    /// 验收行（1–200 行）。
    #[validate(length(min = 1, max = 200, message = "验收行数必须在1-200之间"))]
    pub lines: Vec<AcceptanceLineInput>,
}

/// 客户验收过账请求（携带逐行分配；通过/短少/拒收数量以草稿行为准）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PostCustomerAcceptanceRequest {
    /// 逐行对履约事实的分配（行内合计必须等于该行通过数量）。
    #[validate(length(min = 1, max = 200, message = "验收行数必须在1-200之间"))]
    pub lines: Vec<PostAcceptanceLineInput>,
}

/// 客户验收过账的逐行输入。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PostAcceptanceLineInput {
    /// 验收明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 对履约事实的分配。
    #[validate(length(min = 1, max = 200, message = "分配行数必须在1-200之间"))]
    pub allocations: Vec<AcceptanceAllocationInput>,
}

/// 客户验收冲正请求（误录时新增反向验收及反向分配）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReverseCustomerAcceptanceRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝冲正（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 冲正原因说明。
    #[validate(custom(function = "non_blank", message = "冲正原因不能为空"))]
    pub reason_text: String,
}

/// 客户验收单列表视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerAcceptanceView {
    /// 实体主键。
    pub id: String,
    /// 客户验收单号。
    pub acceptance_no: String,
    /// 销售单。
    pub sales_order_id: String,
    /// 验收时间（秒级时间戳）。
    pub accepted_at: i64,
    /// 验收结果。
    pub result: AcceptanceResult,
    /// 当前状态。
    pub status: CustomerAcceptanceState,
    /// 误录验收的反向事实。
    pub reversal_of_acceptance_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 客户验收行视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerAcceptanceLineView {
    /// 实体主键。
    pub id: String,
    /// 稳定行号。
    pub line_no: u32,
    /// 验收明细。
    pub sales_order_line_id: String,
    /// 通过数量。
    pub accepted_quantity: Quantity,
    /// 短少数量。
    pub short_quantity: Quantity,
    /// 拒收数量。
    pub rejected_quantity: Quantity,
    /// 依据说明。
    pub reason: Option<String>,
}

/// 验收履约分配视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AcceptanceAllocationView {
    /// 实体主键。
    pub id: String,
    /// 验收结果行。
    pub customer_acceptance_line_id: String,
    /// 履约事实类型。
    pub fulfillment_fact_type: FulfillmentFactType,
    /// 履约事实行。
    pub fulfillment_line_id: String,
    /// 分配动作。
    pub allocation_action: AllocationAction,
    /// 正数验收数量。
    pub allocated_quantity: Quantity,
    /// 反向分配引用的原分配。
    pub reverses_allocation_id: Option<String>,
}

/// 客户验收单详情视图（表头 + 行 + 分配）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerAcceptanceDetailView {
    /// 表头。
    pub acceptance: CustomerAcceptanceView,
    /// 验收行。
    pub lines: Vec<CustomerAcceptanceLineView>,
    /// 验收履约分配。
    pub allocations: Vec<AcceptanceAllocationView>,
}

/// 客户验收单列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CustomerAcceptanceListParams {
    /// 销售单筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 单据状态筛选。
    pub status: Option<CustomerAcceptanceState>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`accepted_at`/`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的客户验收单列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CustomerAcceptanceListQuery {
    /// 销售单筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 单据状态筛选。
    pub status: Option<CustomerAcceptanceState>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl CustomerAcceptanceListParams {
    /// 归一化客户验收单列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<CustomerAcceptanceListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, CUSTOMER_ACCEPTANCE_SORT_FIELDS)?;
        Ok(CustomerAcceptanceListQuery {
            sales_order_id: self.sales_order_id.clone(),
            status: self.status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 可验收履约事实视图（W06：服务端扣除冲正后的净数量与守恒分配）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EligibleFulfillmentFactView {
    /// 履约事实行主键。
    pub fulfillment_line_id: String,
    /// 履约事实类型。
    pub fulfillment_fact_type: FulfillmentFactType,
    /// 发货类型（仅发货事实；仓发/直发展示区分用，其他事实为空）。
    pub delivery_type: Option<DeliveryType>,
    /// 履约单号（发货单号/履约记录号）。
    pub fulfillment_no: String,
    /// 销售稳定明细。
    pub sales_order_line_id: String,
    /// 行号。
    pub line_no: u32,
    /// 品名快照。
    pub item_snapshot: String,
    /// 单位快照。
    pub unit_code: Option<String>,
    /// 履约发生时间（秒级时间戳）。
    pub occurred_at: i64,
    /// 净成功履约数量（冲正后）。
    pub net_successful_quantity: Quantity,
    /// 已净验收分配数量（APPLY − REVERSE）。
    pub net_accepted_allocated_quantity: Quantity,
    /// 本次最多可验收数量（守恒）。
    pub eligible_quantity: Quantity,
    /// 物流承运方（发货事实）。
    pub carrier: Option<String>,
    /// 物流单号（发货事实）。
    pub tracking_no: Option<String>,
}

/// 验收销售明细分组视图（W06 销售行 + 可验收事实）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AcceptanceSalesLineGroupView {
    /// 销售稳定明细。
    pub sales_order_line_id: String,
    /// 行号。
    pub line_no: u32,
    /// 品名快照。
    pub item_snapshot: String,
    /// 单位快照。
    pub unit_code: Option<String>,
    /// 应履约数量。
    pub required_quantity: Quantity,
    /// 净已验收数量。
    pub net_accepted_quantity: Quantity,
    /// 可验收履约事实。
    pub fulfillment_facts: Vec<EligibleFulfillmentFactView>,
}

/// 客户验收工作台视图（W06：销售明细 + 可验收事实 + 验收历史）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AcceptanceEligibilityView {
    /// 销售单。
    pub sales_order_id: String,
    /// 销售行分组。
    pub sales_lines: Vec<AcceptanceSalesLineGroupView>,
    /// 验收历史（已过账与已冲正，按验收时间倒序）。
    pub history: Vec<CustomerAcceptanceView>,
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_sort, CreateDeliveryRequest, CreatePurchaseReceiptRequest, DeliveryListParams,
        DeliveryView, PurchaseReceiptListParams, PurchaseReceiptView, SortDir,
    };
    use entities::fulfillment::{DeliveryState, DeliveryType, PurchaseReceiptState};
    use entities::ids::SalesOrderId;
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("quantity".to_string()), &None, &["created_at", "posted_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" posted_at ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "posted_at"],
        )
        .unwrap();
        assert_eq!(field, "posted_at");
        assert_eq!(direction, SortDir::Asc);

        let (field, direction) = normalize_sort(&None, &None, &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Desc);
    }

    #[test]
    fn list_params_normalize_paging_and_reject_unbounded_page_size() {
        let receipt = PurchaseReceiptListParams {
            purchase_order_id: None,
            status: Some(PurchaseReceiptState::Posted),
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("created_at".to_string()),
            sort_dir: Some("asc".to_string()),
        };
        let query = receipt.normalized().unwrap();
        assert_eq!(query.status, Some(PurchaseReceiptState::Posted));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.page_size, 50);

        let invalid = PurchaseReceiptListParams {
            purchase_order_id: None,
            status: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(invalid.validate().is_err());

        let delivery = DeliveryListParams {
            sales_order_id: Some(SalesOrderId::new("so-1")),
            status: Some(DeliveryState::Shipped),
            page: Some(1),
            page_size: Some(30),
            sort_by: None,
            sort_dir: None,
        };
        let query = delivery.normalized().unwrap();
        assert_eq!(query.sales_order_id.as_deref(), Some("so-1"));
        assert_eq!(query.paging.page_size, 30);
    }

    /// 采购收货创建请求拒绝定义 ID / 审批人；视图不暴露审批区。
    #[test]
    fn purchase_receipt_create_and_view_have_no_approval_surface() {
        let valid = serde_json::json!({
            "receipt_no": "PR-1",
            "purchase_order_id": "po-1",
            "warehouse_id": "wh-1",
            "lines": [{
                "purchase_order_revision_line_id": "porl-1",
                "received_quantity": "10",
                "qualified_quantity": "10",
                "rejected_quantity": "0"
            }]
        });
        assert!(serde_json::from_value::<CreatePurchaseReceiptRequest>(valid).is_ok());
        let forged = serde_json::json!({
            "receipt_no": "PR-1",
            "purchase_order_id": "po-1",
            "warehouse_id": "wh-1",
            "lines": [{
                "purchase_order_revision_line_id": "porl-1",
                "received_quantity": "10",
                "qualified_quantity": "10",
                "rejected_quantity": "0"
            }],
            "definition_id": "forged",
            "assignee": "forged"
        });
        assert!(serde_json::from_value::<CreatePurchaseReceiptRequest>(forged).is_err());

        let view = PurchaseReceiptView {
            id: "pr-1".into(),
            receipt_no: "PR-1".into(),
            purchase_order_id: "po-1".into(),
            warehouse_id: "wh-1".into(),
            status: PurchaseReceiptState::Draft,
            posted_at: None,
            version: 1,
            created_at: 1,
        };
        let value = serde_json::to_value(&view).expect("视图可序列化");
        let object = value.as_object().expect("视图为对象");
        assert!(!object.contains_key("approval"));
        assert!(!object.contains_key("definition_id"));
        assert!(!object.contains_key("assignee"));
    }

    /// 发货创建请求拒绝定义 ID / 审批人；视图不暴露审批区。
    #[test]
    fn delivery_create_and_view_have_no_approval_surface() {
        let valid = serde_json::json!({
            "delivery_no": "DV-1",
            "delivery_type": "WAREHOUSE_SHIP",
            "sales_order_id": "so-1",
            "warehouse_id": "wh-1",
            "lines": [{
                "sales_order_line_id": "so-line-1",
                "quantity": "2",
                "stock_reservation_id": "rsv-1"
            }]
        });
        assert!(serde_json::from_value::<CreateDeliveryRequest>(valid).is_ok());
        let forged = serde_json::json!({
            "delivery_no": "DV-1",
            "delivery_type": "WAREHOUSE_SHIP",
            "sales_order_id": "so-1",
            "warehouse_id": "wh-1",
            "lines": [{
                "sales_order_line_id": "so-line-1",
                "quantity": "2",
                "stock_reservation_id": "rsv-1"
            }],
            "definition_id": "forged",
            "assignee": "forged"
        });
        assert!(serde_json::from_value::<CreateDeliveryRequest>(forged).is_err());

        let view = DeliveryView {
            id: "dv-1".into(),
            delivery_no: "DV-1".into(),
            delivery_type: DeliveryType::WarehouseShip,
            sales_order_id: "so-1".into(),
            purchase_order_id: None,
            warehouse_id: Some("wh-1".into()),
            status: DeliveryState::Draft,
            carrier: None,
            tracking_no: None,
            shipped_at: None,
            version: 1,
            created_at: 1,
        };
        let value = serde_json::to_value(&view).expect("视图可序列化");
        let object = value.as_object().expect("视图为对象");
        assert!(!object.contains_key("approval"));
        assert!(!object.contains_key("definition_id"));
        assert!(!object.contains_key("assignee"));
    }
}
