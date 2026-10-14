//! 采购单对象中心与应付汇总视图。

use erp_procurement::entity::purchase_order::{
    PurchaseFreightWaiver, PurchaseSourceResolution, SourceImpactReason, SupplyPriceBasis,
};
use erp_sales::entity::sales_order::SalesDeliveryAllocation;

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
    /// 当前入库目标仓展示名。
    pub target_warehouse_name: Option<String>,
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
    /// 来源销售变更对本采购的影响；采购引用当前销售版本且无待处理记录时为空。
    pub source_update: Option<PurchaseSourceUpdateView>,
    /// 选源时冻结的价格依据与配送判定；费用采购单为空。
    pub delivery: Option<PurchaseDeliverySummaryView>,
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

/// 采购单冻结的价格依据与逐行配送判定。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseDeliverySummaryView {
    /// 价格依据。
    pub price_basis: SupplyPriceBasis,
    /// 逐行配送判定。
    pub lines: Vec<PurchaseDeliveryJudgementView>,
}

/// 一条采购商品的配送判定与运费跟进状态。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseDeliveryJudgementView {
    /// 稳定销售行。
    pub sales_order_line_id: String,
    /// 判定时的配送模板名称；未设置模板时为空。
    pub template_name: Option<String>,
    /// 判定时的模板版本号。
    pub template_revision_no: Option<u64>,
    /// 是否运费另计。
    pub freight_extra: bool,
    /// 运费另计尚未追加物流费用采购且未标记供应商不收运费。
    pub freight_pending: bool,
    /// "供应商不收运费"标记。
    pub waiver: Option<PurchaseFreightWaiver>,
}

/// 来源销售变更对本采购的影响（第一期 §6.3“销售变更对采购的影响”第 10 条）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseSourceUpdateView {
    /// 采购依据待更新：存在未处理且仍受影响的依据更新记录。
    pub affected: bool,
    /// 草稿已按新销售版本修改，重新提交审批后处理完成。
    pub awaiting_submission: bool,
    /// 未处理记录主键；不受影响时为空。
    pub record_id: Option<String>,
    /// 未处理记录版本，登记维持原采购时回传。
    pub record_version: Option<u64>,
    /// 是否因数量归零或减少暂停付款。
    pub blocks_payment: bool,
    /// 未处理记录的影响原因。
    pub reasons: Vec<SourceImpactReason>,
    /// 采购引用的销售版本号。
    pub purchase_sales_revision_no: Option<u32>,
    /// 销售当前版本号。
    pub current_sales_revision_no: Option<u32>,
    /// 采购引用的销售版本已不是当前版本。
    pub sales_revision_outdated: bool,
    /// 最近一次处理方式代码；尚无已处理记录时为空。
    pub last_resolution: Option<PurchaseSourceResolution>,
    /// 最近一次处理方式中文名。
    pub last_resolution_label: Option<String>,
    /// 最近一次维持原采购的原因。
    pub last_maintain_note: Option<String>,
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
    /// 本销售行的完整交付分配；不表示该采购单自动承担全部数量。
    pub delivery_allocations: Vec<SalesDeliveryAllocation>,
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
