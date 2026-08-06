//! 域 D17 `inventory` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；数量一律十进制字符串
//! （`entities::money::Quantity` 自定义序列化）。契约目标形状（items/total/
//! page/page_size）与 D01 source_registry 保持一致，本域按域内对象名提供
//! 各列表视图。

use entities::ids::{SalesOrderLineId, SkuId, WarehouseId};
use entities::inventory::{
    AdjustmentReasonType, MovementDirection, MovementType, ReservationStatus, StockAdjustmentState,
};
use entities::money::Quantity;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{page_or_default, page_size_or_default};

/// 库存余额列表允许的排序字段白名单（api-contract §4：Service 层校验，禁止任意字段透传）。
pub(crate) const STOCK_BALANCE_SORT_FIELDS: &[&str] = &["sku_id", "created_at"];
/// 库存流水列表允许的排序字段白名单。
pub(crate) const STOCK_MOVEMENT_SORT_FIELDS: &[&str] = &["occurred_at", "recorded_at", "created_at"];
/// 库存预占列表允许的排序字段白名单。
pub(crate) const STOCK_RESERVATION_SORT_FIELDS: &[&str] = &["created_at", "updated_at"];
/// 库存调整单列表允许的排序字段白名单。
pub(crate) const STOCK_ADJUSTMENT_SORT_FIELDS: &[&str] = &["created_at", "adjustment_no"];

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

/// 库存余额列表视图（W10 台账，含仓库与 SKU 基础信息展示字段）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StockBalanceView {
    /// 实体主键。
    pub id: String,
    /// 仓库。
    pub warehouse_id: String,
    /// 仓库代码（D11 跨域投影）。
    pub warehouse_code: String,
    /// 仓库名称（D11 当前修订快照）。
    pub warehouse_name: String,
    /// SKU。
    pub sku_id: String,
    /// SKU 编号（D10 稳定身份）。
    pub sku_code: String,
    /// SKU 名称（D10 当前修订快照）。
    pub sku_name: String,
    /// 规格快照（D10 当前修订快照，可能为空）。
    pub spec_summary: Option<String>,
    /// 账面现存。
    pub on_hand_quantity: Quantity,
    /// 有效预占。
    pub reserved_quantity: Quantity,
    /// 可用数量。
    pub available_quantity: Quantity,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 已应用最后流水。
    pub last_movement_id: Option<String>,
    /// 最后流水业务时间（秒级时间戳）。
    pub last_movement_at: Option<i64>,
    /// 是否存在有效预占。
    pub has_active_reservation: bool,
}

/// 库存流水列表视图（正式事实）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StockMovementView {
    /// 实体主键。
    pub id: String,
    /// 仓库。
    pub warehouse_id: String,
    /// SKU。
    pub sku_id: String,
    /// 流水类型。
    pub movement_type: MovementType,
    /// 流水方向。
    pub direction: MovementDirection,
    /// 正数数量。
    pub quantity: Quantity,
    /// 来源单据标识。
    pub source_document_id: String,
    /// 来源单据行标识。
    pub source_line_id: Option<String>,
    /// 业务实际发生时间（秒级时间戳）。
    pub occurred_at: i64,
    /// ERP 记录时间（秒级时间戳）。
    pub recorded_at: i64,
}

/// 库存预占列表视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StockReservationView {
    /// 实体主键。
    pub id: String,
    /// 仓库。
    pub warehouse_id: String,
    /// SKU。
    pub sku_id: String,
    /// 唯一归属销售明细。
    pub sales_order_line_id: String,
    /// 当前有效预占。
    pub reserved_quantity: Quantity,
    /// 已消耗数量。
    pub consumed_quantity: Quantity,
    /// 已释放数量。
    pub released_quantity: Quantity,
    /// 预占状态。
    pub status: ReservationStatus,
    /// 乐观锁版本。
    pub version: u64,
}

/// 库存调整单列表视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StockAdjustmentView {
    /// 实体主键。
    pub id: String,
    /// 调整单号。
    pub adjustment_no: String,
    /// 仓库。
    pub warehouse_id: String,
    /// 调整原因类型。
    pub reason_type: AdjustmentReasonType,
    /// 当前状态。
    pub status: StockAdjustmentState,
    /// 仓储经办人。
    pub prepared_by: String,
    /// 仓储复核人。
    pub reviewed_by: Option<String>,
    /// 成本影响确认人。
    pub finance_reviewed_by: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 库存调整明细视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StockAdjustmentLineView {
    /// 实体主键。
    pub id: String,
    /// 调整 SKU。
    pub sku_id: String,
    /// 调整数量。
    pub quantity: Quantity,
    /// 调整方向。
    pub direction: MovementDirection,
}

/// 库存调整单详情视图（表头 + 明细 + 过账产生的正式流水）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StockAdjustmentDetailView {
    /// 表头。
    pub adjustment: StockAdjustmentView,
    /// 调整明细。
    pub lines: Vec<StockAdjustmentLineView>,
    /// 过账形成的正式库存流水（未过账为空）。
    pub posted_movements: Vec<StockMovementView>,
}

/// 库存余额详情视图（W10 详情：余额 + 最近流水 + 有效预占 + 未过账调整）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StockBalanceDetailView {
    /// 余额行。
    pub balance: StockBalanceView,
    /// 最近 8 条库存流水（按业务时间倒序）。
    pub recent_movements: Vec<StockMovementView>,
    /// 有效预占（未释放/未全消耗）。
    pub active_reservations: Vec<StockReservationView>,
    /// 未过账的调整单（草稿/待复核/待确认/驳回）。
    pub pending_adjustments: Vec<StockAdjustmentView>,
}

/// 库存余额列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct StockBalanceListParams {
    /// 仓库筛选。
    pub warehouse_id: Option<WarehouseId>,
    /// SKU 筛选。
    pub sku_id: Option<SkuId>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`sku_id`/`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的库存余额列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StockBalanceListQuery {
    /// 仓库筛选。
    pub warehouse_id: Option<WarehouseId>,
    /// SKU 筛选。
    pub sku_id: Option<SkuId>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl StockBalanceListParams {
    /// 归一化库存余额列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<StockBalanceListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, STOCK_BALANCE_SORT_FIELDS)?;
        Ok(StockBalanceListQuery {
            warehouse_id: self.warehouse_id.clone(),
            sku_id: self.sku_id.clone(),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 库存流水列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct StockMovementListParams {
    /// 仓库筛选。
    pub warehouse_id: Option<WarehouseId>,
    /// SKU 筛选。
    pub sku_id: Option<SkuId>,
    /// 流水类型筛选。
    pub movement_type: Option<MovementType>,
    /// 流水方向筛选。
    pub direction: Option<MovementDirection>,
    /// 发生时间下界（含，秒级时间戳）。
    pub occurred_from: Option<i64>,
    /// 发生时间上界（含，秒级时间戳）。
    pub occurred_to: Option<i64>,
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

/// 归一化后的库存流水列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StockMovementListQuery {
    /// 仓库筛选。
    pub warehouse_id: Option<WarehouseId>,
    /// SKU 筛选。
    pub sku_id: Option<SkuId>,
    /// 流水类型筛选。
    pub movement_type: Option<MovementType>,
    /// 流水方向筛选。
    pub direction: Option<MovementDirection>,
    /// 发生时间下界。
    pub occurred_from: Option<i64>,
    /// 发生时间上界。
    pub occurred_to: Option<i64>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl StockMovementListParams {
    /// 归一化库存流水列表查询参数。
    ///
    /// 时间区间校验（下界不晚于上界）、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 时间区间倒挂、排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<StockMovementListQuery> {
        if let (Some(from), Some(to)) = (self.occurred_from, self.occurred_to) {
            if from > to {
                return Err(Error::ValidationError("发生时间区间下界不得晚于上界".to_string()));
            }
        }
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, STOCK_MOVEMENT_SORT_FIELDS)?;
        Ok(StockMovementListQuery {
            warehouse_id: self.warehouse_id.clone(),
            sku_id: self.sku_id.clone(),
            movement_type: self.movement_type,
            direction: self.direction,
            occurred_from: self.occurred_from,
            occurred_to: self.occurred_to,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 库存预占列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct StockReservationListParams {
    /// 仓库筛选。
    pub warehouse_id: Option<WarehouseId>,
    /// SKU 筛选。
    pub sku_id: Option<SkuId>,
    /// 预占状态筛选。
    pub status: Option<ReservationStatus>,
    /// 唯一归属销售明细筛选。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`updated_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的库存预占列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StockReservationListQuery {
    /// 仓库筛选。
    pub warehouse_id: Option<WarehouseId>,
    /// SKU 筛选。
    pub sku_id: Option<SkuId>,
    /// 预占状态筛选。
    pub status: Option<ReservationStatus>,
    /// 唯一归属销售明细筛选。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl StockReservationListParams {
    /// 归一化库存预占列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<StockReservationListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, STOCK_RESERVATION_SORT_FIELDS)?;
        Ok(StockReservationListQuery {
            warehouse_id: self.warehouse_id.clone(),
            sku_id: self.sku_id.clone(),
            status: self.status,
            sales_order_line_id: self.sales_order_line_id.clone(),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 库存调整单列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct StockAdjustmentListParams {
    /// 仓库筛选。
    pub warehouse_id: Option<WarehouseId>,
    /// 单据状态筛选。
    pub status: Option<StockAdjustmentState>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`adjustment_no`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的库存调整单列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StockAdjustmentListQuery {
    /// 仓库筛选。
    pub warehouse_id: Option<WarehouseId>,
    /// 单据状态筛选。
    pub status: Option<StockAdjustmentState>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl StockAdjustmentListParams {
    /// 归一化库存调整单列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<StockAdjustmentListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, STOCK_ADJUSTMENT_SORT_FIELDS)?;
        Ok(StockAdjustmentListQuery {
            warehouse_id: self.warehouse_id.clone(),
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

/// 库存调整明细输入（创建时随调整单提交）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockAdjustmentLineInput {
    /// 调整 SKU。
    pub sku_id: SkuId,
    /// 调整数量（正数）。
    pub quantity: Quantity,
    /// 调整方向。
    pub direction: MovementDirection,
}

/// 库存调整单创建请求（HTTP 契约：表头 + 明细一次提交，初始状态为草稿）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateStockAdjustmentRequest {
    /// 调整单号（全局唯一）。
    #[validate(length(min = 1, max = 64, message = "调整单号长度必须在1-64之间"))]
    pub adjustment_no: String,
    /// 仓库。
    pub warehouse_id: WarehouseId,
    /// 调整原因类型。
    pub reason_type: AdjustmentReasonType,
    /// 调整明细（1–100 行）。
    #[validate(length(min = 1, max = 100, message = "调整明细行数必须在1-100之间"))]
    pub lines: Vec<StockAdjustmentLineInput>,
}

/// 库存调整单更新请求（携带乐观锁版本；仅草稿/驳回可更新）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateStockAdjustmentRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 调整原因类型；缺省表示不修改。
    pub reason_type: Option<AdjustmentReasonType>,
}

/// 库存调整单提交仓储复核请求（携带仓储复核人，岗位分离由实体校验）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SubmitStockAdjustmentRequest {
    /// 仓储复核人（账号或系统身份）。
    #[validate(length(min = 1, max = 128, message = "仓储复核人不能为空"))]
    pub reviewed_by: String,
}

/// 仓储复核通过、提交财务确认请求（携带成本影响确认人）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ApproveStockAdjustmentRequest {
    /// 成本影响确认人（账号或系统身份）。
    #[validate(length(min = 1, max = 128, message = "成本影响确认人不能为空"))]
    pub finance_reviewed_by: String,
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_sort, SortDir, StockAdjustmentListParams, StockBalanceListParams, StockMovementListParams,
        StockReservationListParams,
    };
    use entities::ids::{SalesOrderLineId, SkuId, WarehouseId};
    use entities::inventory::{MovementType, ReservationStatus, StockAdjustmentState};
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("quantity".to_string()), &None, &["sku_id", "created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" sku_id ".to_string()),
            &Some(" asc ".to_string()),
            &["sku_id", "created_at"],
        )
        .unwrap();
        assert_eq!(field, "sku_id");
        assert_eq!(direction, SortDir::Asc);

        let (field, direction) = normalize_sort(&None, &None, &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Desc);
    }

    #[test]
    fn balance_list_params_normalize_paging_and_filters() {
        let params = StockBalanceListParams {
            warehouse_id: Some(WarehouseId::new("wh-1")),
            sku_id: Some(SkuId::new("sku-1")),
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("sku_id".to_string()),
            sort_dir: Some("asc".to_string()),
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.warehouse_id.as_deref(), Some("wh-1"));
        assert_eq!(query.sku_id.as_deref(), Some("sku-1"));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.page_size, 50);
        assert_eq!(query.paging.sort_by, "sku_id");
    }

    #[test]
    fn movement_list_params_reject_inverted_time_range() {
        let params = StockMovementListParams {
            warehouse_id: None,
            sku_id: None,
            movement_type: Some(MovementType::PurchaseReceiptIn),
            direction: None,
            occurred_from: Some(1_800_000_000),
            occurred_to: Some(1_700_000_000),
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.normalized().is_err(), "时间区间倒挂必须拒绝");
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let balance = StockBalanceListParams {
            warehouse_id: None,
            sku_id: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(balance.validate().is_err());

        let reservation = StockReservationListParams {
            warehouse_id: None,
            sku_id: None,
            status: Some(ReservationStatus::Active),
            sales_order_line_id: Some(SalesOrderLineId::new("so-line-1")),
            page: Some(1),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(reservation.validate().is_err());

        let adjustment = StockAdjustmentListParams {
            warehouse_id: None,
            status: Some(StockAdjustmentState::PendingWarehouseReview),
            page: Some(1),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(adjustment.validate().is_err());
    }
}
