//! 采购单对象中心与应付汇总视图。

use super::*;

/// 采购单对象中心视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseOrderCenterView {
    /// 实体主键。
    pub id: String,
    /// 采购单号。
    pub purchase_no: String,
    /// 主状态。
    pub status: PurchaseOrderStatus,
    /// 财务审核状态。
    pub review_status: PurchaseReviewStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 来源销售单。
    pub sales_order_id: String,
    /// 来源销售单业务单号。
    pub sales_order_no: String,
    /// 供应商。
    pub supplier_id: String,
    /// 供应商名称快照。
    pub supplier_name: String,
    /// 采购类型。
    pub purchase_type: PurchaseType,
    /// 付款条件。
    pub payment_term_code: String,
    /// 履约责任。
    pub fulfillment_responsibility: FulfillmentResponsibility,
    /// 当前采购单责任人账号 ID。
    pub owner_user_id: String,
    /// 当前采购单责任人展示名。
    pub owner_name: String,
    /// 仓库履约冻结的目标收货仓。
    pub target_warehouse_id: Option<String>,
    /// 付款进度。
    pub payment_progress: ProgressStatus,
    /// 收票进度。
    pub invoice_progress: ProgressStatus,
    /// 履约进度。
    pub fulfillment_progress: ProgressStatus,
    /// 当前待财务审核的不可变提交。
    pub current_submission_id: Option<String>,
    /// 当前生效版本。
    pub current_revision_id: Option<String>,
    /// 当前生效版本号。
    pub revision_no: Option<u32>,
    /// 当前内容来源（`DRAFT`/`SUBMISSION`/`REVISION`）。
    pub content_source: String,
    /// 当前内容行。
    pub lines: Vec<PurchaseOrderLineView>,
    /// 从本采购内容精确关联的销售版本读取；不授予普通销售单或合同详情资格。
    pub source_sales_order: Option<PurchaseSourceSalesOrderView>,
    /// 当前内容表头汇总。
    pub totals: TotalsView,
    /// 生效版本的销售分配。
    pub allocations: Vec<PurchaseSalesAllocationView>,
    /// 本采购单的变更单列表。
    pub changes: Vec<PurchaseChangeSummaryView>,
    /// 应付往来子账汇总（采购单生效形成应付后存在，否则为空）。
    pub payable_summary: Option<PurchaseOrderPayableSummaryView>,
    /// 统一只读审批结构。客户端不得据此选择定义或审批人。
    pub approval: DocumentApprovalView,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 采购上下文限定读取的来源销售版本。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseSourceSalesOrderView {
    /// 来源销售稳定身份及业务编号。
    pub sales_order_id: String,
    /// 来源销售业务编号。
    pub sales_order_no: String,
    /// 当前销售商业状态；正文商业内容仍来自指定不可变版本。
    pub status: String,
    /// 指定销售版本身份。
    pub revision_id: String,
    /// 指定销售版本号。
    pub revision_no: u32,
    /// 销售版本冻结的客户名称。
    pub customer_name: String,
    /// 当前销售责任人姓名。
    pub sales_owner_name: Option<String>,
    /// 指定销售版本冻结的合同编号；后补合同不覆盖旧版本。
    pub contract_no: Option<String>,
    /// 指定销售版本完整金额。
    pub totals: TotalsView,
    /// 指定版本全部实物服务明细，成交价不读取公司 SKU 参考价。
    pub lines: Vec<PurchaseSourceSalesLineView>,
    /// 指定销售版本对应的合同或建单凭证安全元数据。
    pub materials: Vec<PurchaseSourceSalesMaterialView>,
    /// 材料目录当前无法安全读取；价格和版本正文仍可独立核对。
    #[serde(default)]
    pub materials_unavailable: bool,
}

/// 采购上下文销售材料目录；存储地址、版本和指纹仅保留在服务器。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseSourceSalesMaterialView {
    /// 文件资产引用，只能用于同一采购上下文的下载路径。
    pub file_asset_id: String,
    /// 材料类型：`CONTRACT` 或 `EVIDENCE`。
    pub kind: String,
    /// 原始文件名称。
    pub file_name: String,
    /// 文件内容类型。
    pub content_type: String,
    /// 文件字节数。
    pub byte_size: u64,
}

/// 采购上下文中的销售成交明细。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseSourceSalesLineView {
    /// 不可变销售版本行身份，用于采购行精确关联。
    pub sales_order_revision_line_id: String,
    /// 稳定销售行身份。
    pub sales_order_line_id: String,
    /// 销售展示行号。
    pub line_no: u32,
    /// 冻结商品名称。
    pub item_name: String,
    /// 冻结规格。
    pub specification: Option<String>,
    /// 销售数量。
    pub quantity: String,
    /// 销售单位。
    pub unit: String,
    /// 已成交含税销售单价。
    pub unit_price_gross: String,
    /// 该销售行完整含税金额，不是采购数量分摊收入。
    pub gross_amount: String,
}

/// 采购单应付往来子账汇总（按采购单维度，来自应付子账派生；未生效时为空）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseOrderPayableSummaryView {
    /// 应付未结（含税）。
    pub payable_open_amount: Amount,
    /// 已付并核销（含税）。
    pub paid_allocated_amount: Amount,
    /// 已收票并核销（含税）。
    pub purchase_invoice_allocated_amount: Amount,
}
