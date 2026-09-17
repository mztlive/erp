//! 履约delivery请求及单域查询 DTO。
use erp_core::ids::{
    PurchaseLineSalesAllocationId, PurchaseOrderId, SalesOrderId, SalesOrderLineId, StockReservationId,
    WarehouseId,
};
use erp_core::money::Quantity;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::{DELIVERY_SORT_FIELDS, PageParams, non_blank, normalize_paging};
use crate::Result;
use crate::entity::fulfillment::{DeliveryState, DeliveryType};

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

/// 保存发货最终草稿并过账的原子命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PostDeliveryRequest {
    /// 期望的发货单版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 最终物流承运方；缺省表示保持草稿值。
    pub carrier: Option<String>,
    /// 最终物流单号；缺省表示保持草稿值。
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
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
        Ok(DeliveryListQuery {
            sales_order_id: self.sales_order_id.clone(),
            status: self.status,
            paging: normalize_paging(
                &self.sort_by,
                &self.sort_dir,
                self.page,
                self.page_size,
                DELIVERY_SORT_FIELDS,
            )?,
        })
    }
}
