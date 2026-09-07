//! 履约electronic_delivery请求及单域查询 DTO。
use super::non_blank;
use super::{normalize_sort, PageParams, ELECTRONIC_DELIVERY_SORT_FIELDS};
use crate::entity::fulfillment::{ElectronicDeliveryState, FulfillmentResult};
use crate::Result;
use application_core::{page_or_default, page_size_or_default};
use erp_core::ids::{FileAssetId, PurchaseLineSalesAllocationId, PurchaseOrderId, SalesOrderLineId};
use erp_core::money::Quantity;
use serde::{Deserialize, Serialize};
use validator::Validate;

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
