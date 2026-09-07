//! 履约purchase_receipt请求及单域查询 DTO。
use super::non_blank;
use super::{normalize_sort, PageParams, PURCHASE_RECEIPT_SORT_FIELDS};
use crate::entity::fulfillment::PurchaseReceiptState;
use crate::Result;
use application_core::{page_or_default, page_size_or_default};
use erp_core::ids::{PurchaseOrderId, PurchaseOrderRevisionLineId, WarehouseId};
use erp_core::money::Quantity;
use serde::{Deserialize, Serialize};
use validator::Validate;

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

/// 保存采购入库最终草稿并过账的原子命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PostPurchaseReceiptRequest {
    /// 期望的采购入库单版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 最终入库仓；缺省表示保持草稿值。
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
    pub quality_result: crate::entity::fulfillment::QualityResult,
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
