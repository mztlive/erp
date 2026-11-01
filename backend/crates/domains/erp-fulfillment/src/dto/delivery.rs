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
use crate::entity::fulfillment::{DeliveryState, DeliveryTrackingEntry, DeliveryType};

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
    /// 包裹对应的销售明细、物流号和可选承运商。
    #[serde(default)]
    pub tracking_entries: Vec<DeliveryTrackingEntry>,
    /// 发货行（1–200 行）。
    #[validate(length(min = 1, max = 200, message = "发货行数必须在1-200之间"))]
    pub lines: Vec<DeliveryLineInput>,
}

/// 发货单更新请求（携带乐观锁版本；仅草稿可更新）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct UpdateDeliveryRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 完整包裹明细关联；缺省不修改，空数组清空。
    pub tracking_entries: Option<Vec<DeliveryTrackingEntry>>,
}

/// 保存发货最终草稿并过账的原子命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PostDeliveryRequest {
    /// 期望的发货单版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 最终完整包裹明细关联；缺省保持原值，空数组清空。
    pub tracking_entries: Option<Vec<DeliveryTrackingEntry>>,
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
    /// 完整包裹明细关联；没有关联时为空，不推断历史单号归属。
    pub tracking_entries: Vec<DeliveryTrackingEntry>,
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
    /// # 参数
    /// 无。
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

#[cfg(test)]
mod tests {
    use super::{PostDeliveryRequest, UpdateDeliveryRequest};

    /// 物流编辑只接受完整明细包裹关联，旧表头物流字段不接受。
    #[test]
    fn delivery_tracking_requests_reject_unknown_legacy_fields() {
        for field in ["tracking_numbers", "tracking_no", "carrier"] {
            let mut value = serde_json::json!({"version":1});
            value.as_object_mut().unwrap().insert(field.into(), serde_json::json!("old"));
            assert!(serde_json::from_value::<UpdateDeliveryRequest>(value.clone()).is_err());
            assert!(serde_json::from_value::<PostDeliveryRequest>(value).is_err());
        }
    }

    /// 缺省保持现状与显式空数组清空包裹有独立语义。
    #[test]
    fn preserves_omitted_entries_and_explicit_empty_clear() {
        let omitted: UpdateDeliveryRequest =
            serde_json::from_value(serde_json::json!({"version":1})).unwrap();
        let clear: UpdateDeliveryRequest =
            serde_json::from_value(serde_json::json!({"version":1,"tracking_entries":[]})).unwrap();
        assert_eq!(omitted.tracking_entries, None);
        assert_eq!(clear.tracking_entries, Some(Vec::new()));
        let entries: PostDeliveryRequest =
            serde_json::from_value(serde_json::json!({"version":1,"tracking_entries":[
                {"sales_order_line_id":"line-1","tracking_no":"A","carrier":"货拉拉"},
                {"sales_order_line_id":"line-2","tracking_no":"A","carrier":"顺丰"}
            ]}))
            .unwrap();
        assert_eq!(entries.tracking_entries.unwrap().len(), 2);
    }
}
