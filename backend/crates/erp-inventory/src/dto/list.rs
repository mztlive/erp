//! 库存列表查询参数与归一化条件。

use application_core::{QueryIds, page_or_default, page_size_or_default};
use erp_core::ids::{SalesOrderLineId, SkuId, WarehouseId};
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::{
    PageParams, STOCK_ADJUSTMENT_SORT_FIELDS, STOCK_BALANCE_SORT_FIELDS, STOCK_MOVEMENT_SORT_FIELDS,
    STOCK_RESERVATION_SORT_FIELDS, normalize_sort,
};
use crate::entity::inventory::{MovementDirection, MovementType, ReservationStatus, StockAdjustmentState};
use crate::error::{Error, Result};

/// 余额可用量筛选；所有条件均在分页前应用。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StockAvailability {
    /// 不限制数量。
    All,
    /// 可用量等于零。
    Zero,
    /// 可用量严格大于零。
    Positive,
    /// 存在有效预占。
    Reserved,
}

/// 库存余额列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct StockBalanceListParams {
    /// SKU 编码、当前名称或规格的字面量关键词。
    #[validate(length(max = 200))]
    pub q: Option<String>,
    /// 精确定位余额。
    pub balance_id: Option<String>,
    /// 可用量条件，在分页和计数前执行。
    pub availability: Option<StockAvailability>,

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
    /// 跨页必须携带当前授权指纹。
    pub scope_version: Option<String>,
}

/// 归一化后的库存余额列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StockBalanceListQuery {
    /// SKU 编码、当前名称或规格的字面量关键词。
    pub q: Option<String>,
    /// 精确定位余额。
    pub balance_id: Option<String>,
    /// 可用量条件，在分页和计数前执行。
    pub availability: Option<StockAvailability>,

    /// 仓库筛选。
    pub warehouse_id: Option<WarehouseId>,
    /// SKU 筛选。
    pub sku_id: Option<SkuId>,
    /// 分页与排序参数。
    pub paging: PageParams,
    /// 跨页范围版本。
    pub scope_version: Option<String>,
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
            q: application_core::normalized_text(self.q.as_deref()),
            balance_id: application_core::normalized_text(self.balance_id.as_deref()),
            availability: self.availability,
            warehouse_id: self.warehouse_id.clone(),
            sku_id: self.sku_id.clone(),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
            scope_version: super::normalized_scope_version(self.scope_version.as_deref())?,
        })
    }
}

/// 库存流水列表查询参数。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct StockMovementListParams {
    /// SKU 编码、当前名称或规格的字面量关键词。
    #[validate(length(max = 200))]
    pub q: Option<String>,

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
    /// 经办人（`recorded_by`），逗号分隔，最多 100 项。
    pub operator_user_ids: Option<QueryIds>,
    /// 跨页必须携带当前授权指纹。
    pub scope_version: Option<String>,
}

/// 归一化后的库存流水列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StockMovementListQuery {
    /// SKU 编码、当前名称或规格的字面量关键词。
    pub q: Option<String>,

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
    /// 经办人筛选。
    pub operator_user_ids: Option<Vec<String>>,
    /// 跨页范围版本。
    pub scope_version: Option<String>,
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
        if let (Some(from), Some(to)) = (self.occurred_from, self.occurred_to)
            && from > to
        {
            return Err(Error::ValidationError("发生时间区间下界不得晚于上界".to_string()));
        }
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, STOCK_MOVEMENT_SORT_FIELDS)?;
        Ok(StockMovementListQuery {
            q: application_core::normalized_text(self.q.as_deref()),
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
            operator_user_ids: super::normalized_user_ids(self.operator_user_ids.as_ref())?,
            scope_version: super::normalized_scope_version(self.scope_version.as_deref())?,
        })
    }
}

/// 库存预占列表查询参数。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct StockReservationListParams {
    /// SKU 编码、当前名称或规格的字面量关键词。
    #[validate(length(max = 200))]
    pub q: Option<String>,

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
    /// SKU 编码、当前名称或规格的字面量关键词。
    pub q: Option<String>,

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
            q: application_core::normalized_text(self.q.as_deref()),
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct StockAdjustmentListParams {
    /// SKU 编码、当前名称或规格的字面量关键词。
    #[validate(length(max = 200))]
    pub q: Option<String>,
    /// 精确定位调整单。
    pub adjustment_id: Option<String>,
    /// 任一明细包含该 SKU。
    pub sku_id: Option<SkuId>,

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
    /// 经办人（`prepared_by`）。
    pub operator_user_ids: Option<QueryIds>,
    /// 申请人（审批快照 `submitted_by`）。
    pub applicant_user_ids: Option<QueryIds>,
    /// 当前开放审批人。
    pub handler_user_ids: Option<QueryIds>,
    /// 跨页必须携带当前授权指纹。
    pub scope_version: Option<String>,
}

/// 归一化后的库存调整单列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StockAdjustmentListQuery {
    /// SKU 编码、当前名称或规格的字面量关键词。
    pub q: Option<String>,
    /// 精确定位调整单。
    pub adjustment_id: Option<String>,
    /// 任一明细包含该 SKU。
    pub sku_id: Option<SkuId>,

    /// 仓库筛选。
    pub warehouse_id: Option<WarehouseId>,
    /// 单据状态筛选。
    pub status: Option<StockAdjustmentState>,
    /// 分页与排序参数。
    pub paging: PageParams,
    /// 经办人筛选。
    pub operator_user_ids: Option<Vec<String>>,
    /// 申请人筛选。
    pub applicant_user_ids: Option<Vec<String>>,
    /// 当前审批人筛选。
    pub handler_user_ids: Option<Vec<String>>,
    /// 跨页范围版本。
    pub scope_version: Option<String>,
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
            q: application_core::normalized_text(self.q.as_deref()),
            adjustment_id: application_core::normalized_text(self.adjustment_id.as_deref()),
            sku_id: self.sku_id.clone(),
            warehouse_id: self.warehouse_id.clone(),
            status: self.status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
            operator_user_ids: super::normalized_user_ids(self.operator_user_ids.as_ref())?,
            applicant_user_ids: super::normalized_user_ids(self.applicant_user_ids.as_ref())?,
            handler_user_ids: super::normalized_user_ids(self.handler_user_ids.as_ref())?,
            scope_version: super::normalized_scope_version(self.scope_version.as_deref())?,
        })
    }
}
