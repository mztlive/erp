//! 履约service_fulfillment请求及单域查询 DTO。
use application_core::{page_or_default, page_size_or_default};
use erp_core::ids::{FileAssetId, PurchaseLineSalesAllocationId, PurchaseOrderId, SalesOrderLineId};
use erp_core::money::Quantity;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::{PageParams, SERVICE_FULFILLMENT_SORT_FIELDS, non_blank, normalize_sort};
use crate::Result;
use crate::entity::fulfillment::{FulfillmentResult, ServiceFulfillmentState};

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

/// 确认线下服务履约的原子命令。
///
/// 采购审核只生成占位草稿；确认时必须写入地点、时间窗、完成说明、数量和
/// 图片凭证。`evidence_attachment_id` 可以是已登记资产，或本次 multipart
/// 的 `pending-file:` 临时引用。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ConfirmServiceFulfillmentRequest {
    /// 期望的服务履约记录版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 履约结果。
    pub result: FulfillmentResult,
    /// 完成说明。
    #[validate(custom(function = "non_blank", message = "完成说明不能为空"))]
    pub completion_note: String,
    /// 服务地点（不透明值，由边界传入）。
    #[validate(custom(function = "non_blank", message = "服务地点不能为空"))]
    pub service_location: String,
    /// 服务开始时间（秒级时间戳）。
    pub service_started_at: i64,
    /// 服务结束时间（秒级时间戳）。
    pub service_ended_at: i64,
    /// 本次完成数量。
    pub quantity: Quantity,
    /// 现场图片凭证。
    pub evidence_attachment_id: FileAssetId,
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
