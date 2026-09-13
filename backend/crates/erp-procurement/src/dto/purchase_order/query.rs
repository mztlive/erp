use crate::entity::purchase_order::{PurchaseLineType, PurchaseOrderStatus, PurchaseReviewStatus};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::Result;
use application_core::{normalized_text, page_or_default, page_size_or_default};

use super::{normalize_sort, SortDir};

/// 采购单列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub const PURCHASE_ORDER_SORT_FIELDS: &[&str] = &["created_at", "purchase_no"];

/// 归一化后的分页查询参数（Service → Repository 共用）。
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

/// 采购单列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PurchaseOrderListParams {
    /// 当前业务负责人 ID，逗号分隔，最多 100 项；只收窄授权结果。
    pub owner_user_ids: Option<application_core::QueryIds>,
    /// 采购单号模糊匹配。
    pub q: Option<String>,
    /// 来源销售单筛选。
    pub sales_order_id: Option<String>,
    /// 供应商筛选。
    pub supplier_id: Option<String>,
    /// 主状态筛选。
    pub status: Option<PurchaseOrderStatus>,
    /// 财务审核状态筛选。
    pub review_status: Option<PurchaseReviewStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`purchase_no`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的采购单列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurchaseOrderListQuery {
    /// 当前负责人精确身份条件。
    pub owner_user_ids: Option<application_core::QueryIds>,
    /// 采购单号模糊匹配。
    pub q: Option<String>,
    /// 来源销售单。
    pub sales_order_id: Option<String>,
    /// 供应商。
    pub supplier_id: Option<String>,
    /// 主状态。
    pub status: Option<PurchaseOrderStatus>,
    /// 财务审核状态。
    pub review_status: Option<PurchaseReviewStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl PurchaseOrderListParams {
    /// 归一化采购单列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub fn normalized(&self) -> Result<PurchaseOrderListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, PURCHASE_ORDER_SORT_FIELDS)?;
        Ok(PurchaseOrderListQuery {
            owner_user_ids: self.owner_user_ids.clone(),
            q: normalized_text(self.q.as_deref()),
            sales_order_id: normalized_text(self.sales_order_id.as_deref()),
            supplier_id: normalized_text(self.supplier_id.as_deref()),
            status: self.status,
            review_status: self.review_status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 采购单明细行视图（草稿/提交/版本三类内容共用形状）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseOrderLineView {
    /// 行实体主键。
    pub line_id: String,
    /// 行号（从 1 递增）。
    pub line_no: u32,
    /// 行类型。
    pub line_type: PurchaseLineType,
    /// 商品/服务行对应的采购二次确认分行；物流费用行为空。
    pub procurement_confirmation_line_id: Option<String>,
    /// 商品行引用的 SKU。
    pub sku_id: Option<String>,
    /// 商品行引用的 SKU 版本。
    pub sku_revision_id: Option<String>,
    /// 商品名称快照。
    pub product_name: Option<String>,
    /// 规格快照。
    pub specification: Option<String>,
    /// 基础单位数量。
    pub quantity: Option<String>,
    /// 单位代码。
    pub base_unit_code: Option<String>,
    /// 含税采购单价。
    pub unit_cost_gross: Option<String>,
    /// 进项税率。
    pub input_tax_rate: Option<String>,
    /// 含税行金额。
    pub gross_amount: String,
    /// 不含税行金额。
    pub net_amount: String,
    /// 税额。
    pub tax_amount: String,
    /// 预计交期（`YYYY-MM-DD`）。
    pub expected_delivery_date: Option<String>,
    /// 商品行对应的销售稳定行。
    pub sales_order_line_id: Option<String>,
    /// 商品行对应的销售当前版本行。
    pub sales_order_revision_line_id: Option<String>,
    /// 商品行对应的历史销售提交行。
    pub sales_order_submission_line_id: Option<String>,
    /// 商品行对应的分配数量。
    pub allocated_quantity: Option<String>,
}

/// 采购行→销售行分配视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseSalesAllocationView {
    /// 分配实体主键。
    pub id: String,
    /// 采购版本明细。
    pub purchase_order_revision_line_id: String,
    /// 被满足的销售版本明细。
    pub sales_order_revision_line_id: String,
    /// 分配数量。
    pub allocated_quantity: String,
    /// 分配采购成本（含税）。
    pub allocated_cost_gross: String,
    /// 分配采购成本（不含税）。
    pub allocated_cost_net: String,
}

/// 采购变更单摘要视图（对象中心的变更子区）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseChangeSummaryView {
    /// 变更单主键。
    pub change_id: String,
    /// 变更单状态。
    pub status: String,
    /// 基准版本。
    pub base_revision_id: String,
    /// 生效后形成的新采购版本（未生效为空）。
    pub effective_revision_id: Option<String>,
    /// 变更原因。
    pub reason: String,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 表头金额汇总视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TotalsView {
    /// 含税金额。
    pub gross: String,
    /// 不含税金额。
    pub net: String,
    /// 税额。
    pub tax: String,
}
