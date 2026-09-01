//! 域 D15 `purchase_order` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳，业务日期 `YYYY-MM-DD`；
//! 金额/数量以字符串传输（`entities::money` 的 serde 字符串形态）。
//!
//! 与 `erp-client/features/purchase-orders/api.ts` 的差异（契约变更）：
//! - 列表状态枚举沿用实体代码（`PENDING_FINANCE_REVIEW`/`PARTIALLY_EXECUTED`/
//!   `VOIDED`），前端 mock 使用 `PENDING_REVIEW`/`PARTIAL`/`VOID`；
//! - 列表/详情同时返回 `sales_order_id` 与 `sales_order_no`：前者只用于路由，
//!   后者是用户可见的跨单据业务引用，禁止把内部 ID 当单号展示；
//! - 草稿 `purchase_no` 为空，首次提交事务分配不可复用正式号；
//! - 表单类写操作（创建/保存/提交/审核）统一返回稳定业务结果，不再返回
//!   `FormalActionResponse` 信封（由 HTTP 统一信封承载）。

use entities::money::Amount;
pub use entities::purchase_order::SupplySourceType;
use entities::purchase_order::{
    digest_parts, normalize_requested_lines, payload_fingerprint, DraftLineEdit, FulfillmentResponsibility,
    ProgressStatus, PurchaseLineType, PurchaseOrderStatus, PurchaseOrderSubmissionLine, PurchaseReviewStatus,
    PurchaseType, RequestedLine, SourcingAssignment, SourcingAssignmentSet,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 作废草稿命令的服务端固定审计动作（同时参与请求指纹）。
pub(crate) const VOID_ACTION: &str = "purchase_order.void";
/// 保存草稿命令的服务端固定审计动作（同时参与请求指纹）。
pub(crate) const SAVE_ACTION: &str = "purchase_order.update";
/// 依据创建命令的服务端固定审计动作（同时参与请求指纹）。
pub(crate) const CREATE_ACTION: &str = "purchase_order.create_from_basis";
/// 选源创建命令的服务端固定审计动作（同时参与请求指纹）。
pub(crate) const CREATE_SOURCING_ACTION: &str = "purchase_order.create_from_sourcing";
/// 提交命令的服务端固定审计动作。
pub(crate) const PURCHASE_SUBMIT_ACTION: &str = "purchase_order.submit";

/// 采购单列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const PURCHASE_ORDER_SORT_FIELDS: &[&str] = &["created_at", "purchase_no"];

/// 排序方向。
pub use crate::query::SortDir;

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
pub(crate) use crate::query::normalize_sort;

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use crate::query::PageView;

/// 校验文本去除首尾空白后非空（validator 的 `length(min=1)` 对纯空白字符串不生效）。
use crate::query::non_blank;

/// 采购单列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PurchaseOrderListParams {
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
pub(crate) struct PurchaseOrderListQuery {
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
    pub(crate) fn normalized(&self) -> Result<PurchaseOrderListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, PURCHASE_ORDER_SORT_FIELDS)?;
        Ok(PurchaseOrderListQuery {
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

/// 采购单列表行视图（契约形状：`purchaseOrderId`/`purchaseNo`/`status` 等）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseOrderListItemView {
    /// 实体主键。
    pub id: String,
    /// 采购单号。
    pub purchase_no: String,
    /// 来源实物及服务销售单。
    pub sales_order_id: String,
    /// 来源销售单业务单号。
    pub sales_order_no: String,
    /// 唯一供应商。
    pub supplier_id: String,
    /// 供应商名称（D07 主体修订快照）。
    pub supplier_name: String,
    /// 采购类型。
    pub purchase_type: PurchaseType,
    /// 付款条件代码（实体 `payment_term_code`）。
    pub payment_term_code: String,
    /// 当前采购单负责人账号 ID。
    pub owner_user_id: Option<String>,
    /// 当前采购单负责人展示名（账号不存在时回落账号 ID）。
    pub owner_name: String,
    /// 主状态。
    pub status: PurchaseOrderStatus,
    /// 财务审核状态。
    pub review_status: PurchaseReviewStatus,
    /// 含税行汇总（字符串，未生效时为零值）。
    pub gross_amount: String,
    /// 不含税行汇总。
    pub net_amount: String,
    /// 税额行汇总。
    pub tax_amount: String,
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
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
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

/// 采购工作面可安全展示的动作阻断摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseActionBlockerView {
    /// 被阻断的动作代码。
    pub action: String,
    /// 结构化阻断码。
    pub code: String,
    /// 面向用户的安全说明。
    pub message: String,
}

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

/// 单据详情返回的统一只读审批结构。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalView {
    /// `PROCESS_REQUIRED` 或 `NO_APPROVAL`。
    pub requirement: String,
    /// 创建时冻结的定义摘要；未绑定为空。
    pub definition: Option<DocumentApprovalDefinitionView>,
    /// 已启动后的实例摘要；未提交为空。
    pub instance: Option<DocumentApprovalInstanceView>,
    /// 有界最近历史。
    pub recent_history: Vec<DocumentApprovalHistoryItemView>,
    /// 完整历史分页游标。
    pub history_page: DocumentApprovalHistoryPageView,
    /// 服务端允许的动作；不含选择定义或审批人。
    pub allowed_actions: Vec<String>,
}

/// 绑定定义只读摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalDefinitionView {
    /// 定义主键。
    pub id: String,
    /// 定义名称。
    pub name: String,
    /// 定义业务版本。
    pub version: u32,
    /// 节点摘要。单据详情不展开审批人。
    pub nodes: Vec<DocumentApprovalNodeView>,
}

/// 定义节点只读摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalNodeView {
    /// 节点键。
    pub key: String,
    /// 节点名称。
    pub name: String,
}

/// 运行实例只读摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalInstanceView {
    /// 实例主键。
    pub id: String,
    /// 实例状态。
    pub status: String,
    /// 当前轮次。
    pub current_round_no: u32,
    /// 当前节点键。
    pub current_node: Option<String>,
    /// 当前节点名称。
    pub current_node_name: Option<String>,
    /// 当前审批人。
    pub current_assignee: Option<String>,
    /// 当前审批人显示名。
    pub current_assignee_name: Option<String>,
    /// 最近驳回原因。
    pub latest_rejection: Option<String>,
    /// 绑定定义业务版本。
    pub process_version: Option<u32>,
    /// 受阻代码；非 BLOCKED 为空。
    pub blocker_code: Option<String>,
}

/// 有界历史项。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalHistoryItemView {
    /// 执行主键。
    pub execution_id: String,
    /// 轮次。
    pub round_no: u32,
    /// 实例内执行序号。
    pub execution_no: u32,
    /// 节点键。
    pub node_key: String,
    /// 节点名称。
    pub node_name: String,
    /// 结束结果。
    pub result: String,
    /// 审批人显示名。
    pub assignee_name: Option<String>,
    /// 决定人。
    pub decided_by: Option<String>,
    /// 决定原因。
    pub decision_reason: Option<String>,
    /// 决定时间（unix 秒）。
    pub decided_at: Option<i64>,
}

/// 完整历史分页。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalHistoryPageView {
    /// 下一页游标。
    pub next_cursor: Option<String>,
    /// 是否还有更多。
    pub has_more: bool,
}

/// 撤回采购变更审批请求。原因必填。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CancelPurchaseChangeApprovalRequest {
    /// 期望的单据乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 非空撤回原因。
    #[validate(length(min = 1, max = 512, message = "撤回原因不能为空"))]
    pub reason: String,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 撤回采购单审批请求。原因必填。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CancelPurchaseOrderApprovalRequest {
    /// 期望的单据乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 非空撤回原因。
    #[validate(length(min = 1, max = 512, message = "撤回原因不能为空"))]
    pub reason: String,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
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

/// 采购创建依据查询参数。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreationBasisListParams {
    /// 可选来源销售单；从销售详情或工作台进入时用于收窄范围。
    pub sales_order_id: Option<String>,
    /// 可选供给分配任务；提供时必须是当前账号拥有的开放任务。
    pub work_item_id: Option<String>,
}

/// 采购创建依据行视图（销售当前版本行 + 当前采购剩余量）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CreationBasisLineView {
    /// 销售稳定行身份。
    pub sales_order_line_id: String,
    /// 销售当前版本行身份。
    pub sales_order_revision_line_id: String,
    /// 销售当前版本内的业务行号。
    pub sales_line_no: u32,
    /// 确认供应商。
    pub supplier_id: String,
    /// 销售当前版本目标数量。
    pub sales_quantity: String,
    /// 当前采购覆盖数量。
    pub covered_quantity: String,
    /// 当前采购剩余数量。
    pub remaining_quantity: String,
    /// 本供应商当前最大可创建数量，等于 `min(remaining, available)`。
    pub max_create_quantity: String,
    /// 兼容展示字段，值等于 `max_create_quantity`。
    pub confirmed_quantity: String,
    /// 最新含税成本。
    pub latest_cost_gross: String,
    /// 进项税率。
    pub input_tax_rate: String,
    /// 采购预计交付日预填值（`YYYY-MM-DD`）。
    pub expected_delivery_date: String,
    /// 销售对客户承诺的最晚交付日（`YYYY-MM-DD`）。
    pub sales_delivery_deadline: String,
    /// 商品名称快照（销售提交行侧联查，缺失时为空）。
    pub product_name: Option<String>,
    /// 规格快照。
    pub specification: Option<String>,
    /// 销售单位快照。
    pub unit: Option<String>,
    /// 含税行金额（按确认数量与成本逐行舍入）。
    pub gross_amount: String,
}

/// 采购创建依据视图（已生效销售单 × 合格供给供应商，§7.4 选源建单入口）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CreationBasisView {
    /// 当前账号拥有且冻结本依据销售行范围的开放供给分配任务。
    pub work_item_id: String,
    /// 精确创建依据（任务、销售当前版本、供应商、采购类型、付款条件、履约责任及剩余量指纹）。
    pub basis_id: String,
    /// 供给来源。
    pub source_type: SupplySourceType,
    /// 被确认的销售单。
    pub sales_order_id: String,
    /// 销售单号。
    pub sales_order_no: String,
    /// 销售当前版本冻结的客户名称。
    pub customer_name: String,
    /// 销售当前版本冻结的合同编号；无合同时为空。
    pub contract_no: Option<String>,
    /// 销售单负责人展示名；账号档案缺失时为空。
    pub sales_owner_name: Option<String>,
    /// 目标销售当前版本。
    pub sales_order_revision_id: String,
    /// 供应商。
    pub supplier_id: String,
    /// 供应商名称。
    pub supplier_name: String,
    /// 现有库存来源的余额主键；采购来源为空。
    pub stock_balance_id: Option<String>,
    /// 现有库存来源的仓库主键；采购来源为空。
    pub warehouse_id: Option<String>,
    /// 现有库存来源的仓库名称；采购来源为空。
    pub warehouse_name: Option<String>,
    /// 来源当前总可供量；库存为余额可用量，未声明上限的采购来源为空。
    pub source_available_quantity: Option<String>,
    /// 采购类型（由商品稳定业务类型确定）。
    pub purchase_type: String,
    /// 履约责任（由采购在商品类型允许范围内选择）。
    pub fulfillment_responsibility: String,
    /// 付款条件（供应商商业资料快照，缺省 `NET-30`；不含经营类目）。
    pub payment_term_code: String,
    /// 供应商经营类目；未登记时为空。
    pub business_category: Option<String>,
    /// 可拆入本单的已确认分行。
    pub lines: Vec<CreationBasisLineView>,
    /// 含税行汇总（只汇总已舍入行金额）。
    pub estimated_gross: String,
}

/// 依据建单的单行本次数量。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreatePurchaseOrderLineRequest {
    /// 销售稳定行身份。
    #[validate(custom(function = "non_blank", message = "销售行不能为空"))]
    pub sales_order_line_id: String,
    /// 本次创建数量；事务内必须大于零且不超过最新可创建数量。
    #[validate(custom(function = "non_blank", message = "本次数量不能为空"))]
    pub quantity: String,
    /// 采购确认的预计交付日，不得晚于销售承诺期限。
    #[validate(custom(function = "non_blank", message = "预计交付日不能为空"))]
    pub expected_delivery_date: String,
}

/// 依据创建采购单请求（精确拆单维度 + 逐行本次数量）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreatePurchaseOrderFromBasisRequest {
    /// 冻结本次销售行责任范围的开放供给分配任务。
    #[validate(custom(function = "non_blank", message = "供给分配任务不能为空"))]
    pub work_item_id: String,
    /// 采购创建依据（当前任务范围内的精确供应商拆分）。
    #[validate(custom(function = "non_blank", message = "创建依据不能为空"))]
    pub basis_id: String,
    /// 采购类型。
    pub purchase_type: PurchaseType,
    /// 付款条件（受控码表代码）。
    #[validate(custom(function = "non_blank", message = "付款条件不能为空"))]
    pub payment_term_code: String,
    /// 仓库履约的目标收货仓；非仓库履约必须为空。
    #[serde(default)]
    #[validate(length(min = 1, max = 128, message = "目标仓库长度必须在1-128个字符之间"))]
    pub target_warehouse_id: Option<String>,
    /// 本次采购明细；允许只创建依据中的部分行或部分数量。
    #[validate(length(min = 1, max = 200, message = "本次采购明细必须在1-200行之间"), nested)]
    pub lines: Vec<CreatePurchaseOrderLineRequest>,
    /// 幂等键（同一命令重复创建返回同一采购单）。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

impl CreatePurchaseOrderFromBasisRequest {
    /// 规范化并校验逐行本次采购数量。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回去除首尾空白、数量已类型化且稳定行不重复的请求行。
    ///
    /// # 错误
    /// 稳定行重复、数量非法或数量不大于零时返回参数验证错误。
    ///
    /// # 关键业务约束
    /// 同一稳定销售行在一次命令中只能出现一次；字符串类型化与集合规则由
    /// `entities::purchase_order::creation_basis` 承担，本方法只负责协议错误映射。
    pub(super) fn normalized_lines(&self) -> Result<Vec<RequestedLine>> {
        let mut parsed = Vec::with_capacity(self.lines.len());
        for line in &self.lines {
            parsed.push(
                RequestedLine::parse(
                    &line.sales_order_line_id,
                    &line.quantity,
                    &line.expected_delivery_date,
                )
                .map_err(|error| Error::ValidationError(error.to_string()))?,
            );
        }
        normalize_requested_lines(&parsed).map_err(|error| Error::ValidationError(error.to_string()))
    }

    /// 计算创建命令请求指纹（不含原始幂等键）。
    ///
    /// # 参数
    /// * `lines` - 已规范化并排序的请求行
    ///
    /// # 返回
    /// 返回不包含原始幂等键的 SHA-256 指纹。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 同一幂等键用于不同依据、范围或数量时必须冲突；摘要形态与存量收据一致，
    /// 修改会破坏幂等兼容。
    pub(super) fn request_fingerprint(&self, lines: &[RequestedLine]) -> String {
        let mut parts = vec![
            self.work_item_id.trim().to_string(),
            self.basis_id.trim().to_string(),
            self.purchase_type.as_str().to_string(),
            self.payment_term_code.trim().to_string(),
            self.target_warehouse_id
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .to_string(),
        ];
        parts.extend(lines.iter().map(|line| {
            format!(
                "{}|{}|{}",
                line.sales_order_line_id, line.quantity, line.expected_delivery_date
            )
        }));
        digest_parts(parts)
    }
}

/// 创建采购单结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CreatePurchaseOrderResult {
    /// 采购单主键。
    pub purchase_order_id: String,
    /// 采购单号。
    pub purchase_no: String,
    /// 乐观锁版本。
    pub lock_version: u64,
    /// 是否复用已有创建结果（幂等重放）。
    pub replayed: bool,
    /// 业务引用。
    pub reference: String,
}

/// 供给分配的一条精确来源与数量。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SourcingLineAssignment {
    /// 销售稳定行身份。
    #[validate(custom(function = "non_blank", message = "销售行不能为空"))]
    pub sales_order_line_id: String,
    /// 本行选用的精确依据，绑定库存余额或采购供给及其履约责任。
    #[validate(custom(function = "non_blank", message = "履约方案不能为空"))]
    pub basis_id: String,
    /// 供给来源；旧客户端缺省为供应商采购。
    #[serde(default)]
    pub source_type: SupplySourceType,
    /// 采购且由仓库履约时的目标收货仓；其他供给来源必须为空。
    #[serde(default)]
    #[validate(length(min = 1, max = 128, message = "目标仓库长度必须在1-128个字符之间"))]
    pub target_warehouse_id: Option<String>,
    /// 本次分配数量；事务内必须大于零且不超过最新可分配数量。
    #[validate(custom(function = "non_blank", message = "本次分配数量不能为空"))]
    pub quantity: String,
    /// 预计交付日；采购来源不得晚于销售承诺期限，库存来源保留同一请求形状。
    #[validate(custom(function = "non_blank", message = "预计交付日不能为空"))]
    pub expected_delivery_date: String,
}

/// 一次确认库存与采购供给分配的请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreatePurchaseOrdersFromSourcingRequest {
    /// 冻结本次销售行责任范围的开放供给分配任务。
    #[validate(custom(function = "non_blank", message = "供给分配任务不能为空"))]
    pub work_item_id: String,
    /// 来源销售单。
    #[validate(custom(function = "non_blank", message = "销售单不能为空"))]
    pub sales_order_id: String,
    /// 已选定的供给分配；同一销售行允许按库存与采购依据拆分。
    #[validate(
        length(min = 1, max = 200, message = "本次供给分配明细必须在1-200行之间"),
        nested
    )]
    pub lines: Vec<SourcingLineAssignment>,
    /// 幂等键（同一命令重复提交时返回原库存预占与采购单）。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

impl CreatePurchaseOrdersFromSourcingRequest {
    /// 规范化并校验逐行选源分配。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回字符串已类型化、同销售行同依据不重复且稳定排序的选源集合。
    ///
    /// # 错误
    /// 销售行或依据空白、数量或预计交付日非法、现有库存另行指定目标仓、
    /// 数量不大于零或同一销售行重复使用同一依据时返回参数验证错误。
    ///
    /// # 关键业务约束
    /// 字符串类型化与集合规则由 `entities::purchase_order::sourcing_plan`
    /// 承担，本方法只负责协议错误映射；同一销售行允许按库存与采购依据拆分。
    pub(super) fn sourcing_assignments(&self) -> Result<SourcingAssignmentSet> {
        let mut parsed = Vec::with_capacity(self.lines.len());
        for line in &self.lines {
            parsed.push(
                SourcingAssignment::parse(
                    &line.sales_order_line_id,
                    &line.basis_id,
                    line.source_type,
                    line.target_warehouse_id.as_deref(),
                    &line.quantity,
                    &line.expected_delivery_date,
                )
                .map_err(|error| Error::ValidationError(error.to_string()))?,
            );
        }
        SourcingAssignmentSet::normalize(&parsed).map_err(|error| Error::ValidationError(error.to_string()))
    }

    /// 计算整批选源创建命令请求指纹（不含原始幂等键）。
    ///
    /// # 参数
    /// * `assignments` - 已规范化并排序的选源行
    ///
    /// # 返回
    /// 返回不包含原始幂等键的 SHA-256 指纹。
    ///
    /// # 错误
    /// 指纹载荷序列化失败时返回内部错误。
    ///
    /// # 关键业务约束
    /// 同一幂等键用于不同任务、销售单、供应商或数量时必须冲突；摘要形态与
    /// 存量收据一致，修改会破坏幂等兼容。
    pub(super) fn request_fingerprint(&self, assignments: &[SourcingAssignment]) -> Result<String> {
        let payload = assignments
            .iter()
            .map(|line| {
                (
                    line.sales_order_line_id.clone(),
                    line.basis_id.clone(),
                    line.source_type,
                    line.target_warehouse_id.clone(),
                    line.quantity.to_string(),
                    line.expected_delivery_date.to_string(),
                )
            })
            .collect::<Vec<_>>();
        payload_fingerprint(
            CREATE_SOURCING_ACTION,
            self.sales_order_id.trim(),
            &(self.work_item_id.trim(), payload),
        )
        .map_err(Into::into)
    }
}

/// 供给分配确认结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CreatePurchaseOrdersFromSourcingResult {
    /// 本次创建并提交或幂等回放的采购单。
    pub orders: Vec<CreatePurchaseOrderResult>,
    /// 本次建立或幂等回放的现有库存销售预占。
    pub stock_reservations: Vec<ExistingStockReservationResult>,
    /// 本次同步后的供给分配任务状态；部分分配固定保持 `OPEN`。
    pub work_item_status: String,
    /// 是否复用已有供给分配结果（幂等重放）。
    pub replayed: bool,
    /// 业务引用，指向来源销售单。
    pub reference: String,
}

/// 现有库存供给分配结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExistingStockReservationResult {
    /// 库存预占主键。
    pub stock_reservation_id: String,
    /// 销售稳定行主键。
    pub sales_order_line_id: String,
    /// 库存余额主键。
    pub stock_balance_id: String,
    /// 仓库主键。
    pub warehouse_id: String,
    /// 本次预占数量。
    pub quantity: String,
}

/// 保存采购草稿请求（表头 + 完整行替换；金额由服务端逐行计算）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SavePurchaseOrderDraftRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 付款条件；缺省表示不修改。
    pub payment_term_code: Option<String>,
    /// 完整行集合（兼容服务端调用；与 `line_patches` 二选一）。
    #[serde(default)]
    pub lines: Vec<SavePurchaseOrderLine>,
    /// 以当前草稿行 ID 为键的可编辑字段快照，由服务端在事务内合并冻结来源字段。
    #[serde(default)]
    pub line_patches: Vec<SavePurchaseOrderLinePatch>,
    /// 幂等键（同内容重复保存返回同一结果）。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    #[validate(length(max = 128, message = "幂等键过长"))]
    pub idempotency_key: String,
}

impl SavePurchaseOrderDraftRequest {
    /// 校验保存请求只使用一种行载荷。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 完整行与行补丁恰好提供一种时返回 `Ok(())`。
    ///
    /// # 错误
    /// 两种都提供或都缺失，或任一补丁字段非法时返回校验错误。
    pub(crate) fn ensure_shape(&self) -> Result<()> {
        if self.lines.is_empty() == self.line_patches.is_empty() {
            return Err(Error::ValidationError(
                "完整采购行与草稿行补丁必须且只能提供一种".to_string(),
            ));
        }
        for patch in &self.line_patches {
            patch.validate()?;
        }
        Ok(())
    }

    /// 生成已规范化草稿行：完整行原样保留顺序返回；补丁路径把客户端可编辑
    /// 字段合并到服务端冻结的草稿来源行。
    ///
    /// # 参数
    /// * `existing` - 服务端当前草稿行
    ///
    /// # 返回
    /// 返回可进入领域校验与写入的完整行集合。
    ///
    /// # 错误
    /// 补丁未覆盖全部当前草稿行、包含重复行补丁、行类型被改写或行已变化时
    /// 返回校验或冲突错误。
    pub(crate) fn resolve_lines(
        &self,
        existing: &[PurchaseOrderSubmissionLine],
    ) -> Result<Vec<SavePurchaseOrderLine>> {
        if !self.lines.is_empty() {
            return Ok(self.lines.clone());
        }
        SavePurchaseOrderLinePatch::resolve_all(&self.line_patches, existing)
    }

    /// 计算保存草稿请求指纹（不含原始幂等键）。
    ///
    /// # 参数
    /// * `purchase_order_id` - 当前路径采购单 ID
    ///
    /// # 返回
    /// 返回不包含原始幂等键的稳定 SHA-256 指纹。
    ///
    /// # 错误
    /// 指纹载荷无法序列化时返回内部错误。
    ///
    /// # 关键业务约束
    /// 行顺序影响提交行序号，因此必须保留；付款条件按现有校验语义去除首尾
    /// 空白；摘要形态与存量收据一致，修改会破坏幂等兼容。
    pub(crate) fn request_fingerprint(&self, purchase_order_id: &str) -> Result<String> {
        let payload = SaveDraftFingerprintPayload {
            expected_lock_version: self.expected_lock_version,
            payment_term_code: self.payment_term_code.as_deref().map(str::trim),
            lines: &self.lines,
            line_patches: &self.line_patches,
        };
        payload_fingerprint(SAVE_ACTION, purchase_order_id, &payload).map_err(Into::into)
    }
}

/// 保存草稿请求指纹载荷。
#[derive(Serialize)]
struct SaveDraftFingerprintPayload<'a> {
    /// 客户端期望乐观锁版本。
    expected_lock_version: u64,
    /// 按业务语义规范化的付款条件。
    payment_term_code: Option<&'a str>,
    /// 保留顺序的完整草稿行。
    lines: &'a [SavePurchaseOrderLine],
    /// 保留顺序的草稿行可编辑字段快照。
    line_patches: &'a [SavePurchaseOrderLinePatch],
}

/// 采购草稿行可编辑字段快照。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SavePurchaseOrderLinePatch {
    /// 当前草稿提交行 ID。
    #[validate(custom(function = "non_blank", message = "草稿行 ID 不能为空"))]
    pub line_id: String,
    /// 行类型；必须与服务端当前草稿一致。
    pub line_type: PurchaseLineType,
    /// 商品/服务采购数量。
    pub quantity: Option<String>,
    /// 商品/服务含税单价；物流费用行表示含税费用金额。
    pub unit_cost_gross: Option<String>,
    /// 进项税率。
    pub input_tax_rate: Option<String>,
}

impl SavePurchaseOrderLinePatch {
    /// 将草稿行补丁合并为完整采购行，冻结字段只从服务端当前草稿取得。
    ///
    /// # 参数
    /// * `patches` - 客户端可编辑字段快照
    /// * `existing` - 服务端当前草稿行
    ///
    /// # 返回
    /// 返回合并后的完整行集合。
    ///
    /// # 错误
    /// 补丁未覆盖全部当前草稿行、包含重复行补丁、行类型被改写或行已变化时
    /// 返回校验或冲突错误。
    pub(crate) fn resolve_all(
        patches: &[Self],
        existing: &[PurchaseOrderSubmissionLine],
    ) -> Result<Vec<SavePurchaseOrderLine>> {
        if patches.len() != existing.len() {
            return Err(Error::ValidationError(
                "采购草稿行补丁必须覆盖全部当前草稿行".to_string(),
            ));
        }
        let mut patch_map = HashMap::with_capacity(patches.len());
        for patch in patches {
            if patch_map
                .insert(patch.line_id.trim().to_string(), patch)
                .is_some()
            {
                return Err(Error::ValidationError("采购草稿包含重复行补丁".to_string()));
            }
        }

        existing
            .iter()
            .map(|line| {
                let patch = patch_map
                    .remove(&line.base.id)
                    .ok_or_else(|| Error::ConflictError("采购草稿行已变化，请刷新后重试".to_string()))?;
                if patch.line_type != line.line_type {
                    return Err(Error::ValidationError("采购草稿行类型不可修改".to_string()));
                }
                let is_item = line.line_type == PurchaseLineType::ItemService;
                let quantity = if is_item {
                    patch
                        .quantity
                        .clone()
                        .or_else(|| line.quantity.map(|value| value.to_string()))
                } else {
                    None
                };
                Ok(SavePurchaseOrderLine {
                    line_type: line.line_type,
                    procurement_confirmation_line_id: line
                        .procurement_confirmation_line_id
                        .as_ref()
                        .map(ToString::to_string),
                    sku_id: line.sku_id.as_ref().map(ToString::to_string),
                    sku_revision_id: line.sku_revision_id.as_ref().map(ToString::to_string),
                    product_name: line.product_name_snapshot.clone(),
                    specification: line.specification_snapshot.clone(),
                    quantity: quantity.clone(),
                    base_unit_code: if is_item {
                        line.base_unit_code.clone()
                    } else {
                        None
                    },
                    unit_cost_gross: if is_item {
                        patch
                            .unit_cost_gross
                            .clone()
                            .or_else(|| line.unit_cost_gross.map(|value| value.to_string()))
                    } else {
                        None
                    },
                    input_tax_rate: patch
                        .input_tax_rate
                        .clone()
                        .or_else(|| line.input_tax_rate.map(|value| value.to_string())),
                    expected_delivery_date: if is_item {
                        line.expected_delivery_date.map(|value| value.to_string())
                    } else {
                        None
                    },
                    sales_order_line_id: line.sales_order_line_id.as_ref().map(ToString::to_string),
                    sales_order_revision_line_id: line
                        .sales_order_revision_line_id
                        .as_ref()
                        .map(ToString::to_string),
                    sales_order_submission_line_id: line
                        .sales_order_submission_line_id
                        .as_ref()
                        .map(ToString::to_string),
                    allocated_quantity: if is_item { quantity } else { None },
                    gross_amount: if is_item {
                        None
                    } else {
                        patch
                            .unit_cost_gross
                            .clone()
                            .or_else(|| Some(line.gross_amount.to_string()))
                    },
                })
            })
            .collect()
    }
}

/// 草稿行写入项。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SavePurchaseOrderLine {
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
    /// 基础单位数量（商品行为必填字符串）。
    pub quantity: Option<String>,
    /// 单位代码。
    pub base_unit_code: Option<String>,
    /// 含税采购单价（商品行为必填）。
    pub unit_cost_gross: Option<String>,
    /// 进项税率（缺省 0）。
    pub input_tax_rate: Option<String>,
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
    /// 物流费用行含税金额（物流行为必填；商品行忽略）。
    pub gross_amount: Option<String>,
}

impl SavePurchaseOrderLine {
    /// 转换为领域草稿行编辑请求。
    ///
    /// 字符串字段原样传递；空白、数量与引用校验由
    /// `entities::purchase_order::validate_draft_line_edits` 统一完成。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回与自身字段一致的草稿行编辑请求。
    ///
    /// # 错误
    /// 无。
    pub(crate) fn to_draft_edit(&self) -> DraftLineEdit {
        DraftLineEdit {
            line_type: self.line_type,
            quantity: self.quantity.clone(),
            allocated_quantity: self.allocated_quantity.clone(),
            procurement_confirmation_line_id: self.procurement_confirmation_line_id.clone(),
            sku_id: self.sku_id.clone(),
            sku_revision_id: self.sku_revision_id.clone(),
            sales_order_line_id: self.sales_order_line_id.clone(),
            sales_order_revision_line_id: self.sales_order_revision_line_id.clone(),
            sales_order_submission_line_id: self.sales_order_submission_line_id.clone(),
        }
    }
}

/// 保存草稿结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SavePurchaseOrderDraftResult {
    /// 新乐观锁版本。
    pub lock_version: u64,
    /// 表头金额汇总（逐行舍入后汇总）。
    pub totals: TotalsView,
    /// 业务引用。
    pub reference: String,
}

/// 作废采购草稿请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct VoidPurchaseOrderRequest {
    /// 期望的采购单乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 作废原因。
    #[validate(custom(function = "non_blank", message = "作废原因不能为空"))]
    #[validate(length(max = 512, message = "作废原因过长"))]
    pub reason: String,
    /// 业务请求幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    #[validate(length(max = 128, message = "幂等键过长"))]
    pub idempotency_key: String,
}

impl VoidPurchaseOrderRequest {
    /// 计算作废请求指纹（不含原始幂等键）。
    ///
    /// # 参数
    /// * `purchase_order_id` - 当前路径采购单 ID
    ///
    /// # 返回
    /// 返回不包含原始幂等键的稳定 SHA-256 指纹。
    ///
    /// # 错误
    /// 指纹载荷无法序列化时返回内部错误。
    ///
    /// # 关键业务约束
    /// 作废原因按实际审计语义去除首尾空白，期望版本仍属于请求载荷；摘要形态
    /// 与存量收据一致，修改会破坏幂等兼容。
    pub(crate) fn request_fingerprint(&self, purchase_order_id: &str) -> Result<String> {
        let payload = VoidDraftFingerprintPayload {
            expected_lock_version: self.expected_lock_version,
            reason: self.reason.trim(),
        };
        payload_fingerprint(VOID_ACTION, purchase_order_id, &payload).map_err(Into::into)
    }
}

/// 作废请求指纹载荷。
#[derive(Serialize)]
struct VoidDraftFingerprintPayload<'a> {
    /// 客户端期望乐观锁版本。
    expected_lock_version: u64,
    /// 去除首尾空白后的作废原因。
    reason: &'a str,
}

/// 作废采购草稿结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct VoidPurchaseOrderResult {
    /// 采购单主键。
    pub purchase_order_id: String,
    /// 作废后的稳定状态。
    pub status: String,
    /// 作废后的乐观锁版本。
    pub lock_version: u64,
    /// 是否命中已经完成的作废结果。
    pub replayed: bool,
    /// 业务引用。
    pub reference: String,
}

/// 提交财务审核请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SubmitPurchaseOrderRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 付款条件只用于确认未改写创建时冻结值。
    pub payment_term_code: Option<String>,
    /// 提交时一并保存的草稿行可编辑字段；为空表示直接冻结当前草稿。
    #[serde(default)]
    pub line_patches: Vec<SavePurchaseOrderLinePatch>,
    /// 幂等键（重复提交只产生一条正式提交）。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    #[validate(length(max = 128, message = "幂等键过长"))]
    pub idempotency_key: String,
}

impl SubmitPurchaseOrderRequest {
    /// 计算提交请求指纹（不含原始幂等键）。
    ///
    /// # 参数
    /// * `purchase_order_id` - 当前路径采购单 ID
    ///
    /// # 返回
    /// 返回不包含原始幂等键的稳定 SHA-256 指纹。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 同一幂等键用于不同期望版本或草稿补丁时必须冲突；形态文本按字段顺序显式
    /// 编码（不依赖 Debug 派生），摘要形态与存量收据一致，修改会破坏幂等兼容。
    pub(crate) fn request_fingerprint(&self, purchase_order_id: &str) -> String {
        digest_parts([
            purchase_order_id.to_string(),
            self.expected_lock_version.to_string(),
            submit_request_shape(&self.payment_term_code, &self.line_patches),
        ])
    }
}

/// 构造提交请求的稳定形态文本。
///
/// 历史提交指纹直接使用 Rust Debug 派生输出（`format!("{:?}|{:?}", ...)`），
/// DTO 字段改名会静默改变指纹并破坏存量收据回放；本函数按字段顺序显式重放
/// 同一字节形态，字段改名不再影响指纹。
fn submit_request_shape(
    payment_term_code: &Option<String>,
    line_patches: &[SavePurchaseOrderLinePatch],
) -> String {
    let patches = line_patches
        .iter()
        .map(|patch| {
            format!(
                "SavePurchaseOrderLinePatch {{ line_id: {:?}, line_type: {}, quantity: {:?}, unit_cost_gross: {:?}, input_tax_rate: {:?} }}",
                patch.line_id,
                submit_line_type_shape(patch.line_type),
                patch.quantity,
                patch.unit_cost_gross,
                patch.input_tax_rate,
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{:?}|[{}]", payment_term_code, patches)
}

/// 返回行类型的历史 Debug 派生字节形态。
///
/// # 参数
/// * `line_type` - 采购行类型
///
/// # 返回
/// 返回与历史 Debug 派生输出一致的固定字符串。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 输出必须与提交指纹存量形态逐字节一致，禁止改名或改变大小写。
fn submit_line_type_shape(line_type: PurchaseLineType) -> &'static str {
    match line_type {
        PurchaseLineType::ItemService => "ItemService",
        PurchaseLineType::LogisticsFee => "LogisticsFee",
    }
}

/// 提交财务审核结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SubmitPurchaseOrderResult {
    /// 采购单主键。
    pub purchase_order_id: String,
    /// 采购单号。
    pub purchase_no: String,
    /// 形成的不可变提交。
    pub submission_id: String,
    /// 提交序号。
    pub submission_no: String,
    /// 审核待办。
    pub work_item_id: String,
    /// 审核待办自身的乐观锁版本。
    pub task_version: u64,
    /// 待办锁定的不可变采购提交版本。
    pub subject_version: String,
    /// 新乐观锁版本。
    pub lock_version: u64,
    /// 业务引用。
    pub reference: String,
}

/// 财务审核结果（通过/驳回共用形状）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseReviewResult {
    /// 已完成的审核待办。
    pub work_item_id: String,
    /// 待办终态，固定为 `COMPLETED`。
    pub work_item_status: String,
    /// 完成后的待办版本。
    pub task_version: String,
    /// 本次审核锁定的不可变采购提交版本。
    pub subject_version: String,
    /// 审核结论（`APPROVED`/`REJECTED`）。
    pub review_result: String,
    /// 通过时形成的生效版本。
    pub revision_id: Option<String>,
    /// 通过时形成的版本号。
    pub revision_no: Option<u32>,
    /// 通过时形成的应付分录。
    pub payable_entry_id: Option<String>,
    /// 新乐观锁版本。
    pub lock_version: u64,
    /// 业务引用。
    pub reference: String,
}

/// 发起采购变更请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct StartPurchaseChangeRequest {
    /// 期望的采购单乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 采购变化原因。
    #[validate(custom(function = "non_blank", message = "变更原因不能为空"))]
    pub reason: String,
    /// 幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 发起采购变更结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StartPurchaseChangeResult {
    /// 变更单主键。
    pub change_id: String,
    /// 基准版本。
    pub base_revision_id: String,
    /// 基准版本号。
    pub base_revision_no: u32,
    /// 采购单新乐观锁版本。
    pub lock_version: u64,
    /// 业务引用。
    pub reference: String,
}

/// 提交采购变更目标内容请求（完整头、行及销售分配）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SubmitPurchaseChangeRequest {
    /// 期望的变更单乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 目标付款条件；缺省沿用基准版本快照。
    pub payment_term_code: Option<String>,
    /// 目标完整行集合。
    pub lines: Vec<SavePurchaseOrderLine>,
    /// 幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 采购变更提交结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseChangeSubmitResult {
    /// 变更单主键。
    pub change_id: String,
    /// 形成的不可变目标提交。
    pub submission_id: String,
    /// 提交序号。
    pub submission_no: String,
    /// 变更单状态。
    pub status: String,
    /// 变更单新乐观锁版本。
    pub lock_version: u64,
    /// 业务引用。
    pub reference: String,
}

/// 采购变更生效请求（§8.1.3：基准版本校验 + 新版本 + 差额 + 指针推进）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct EffectPurchaseChangeRequest {
    /// 期望的变更单乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 目标提交。
    #[validate(custom(function = "non_blank", message = "提交ID不能为空"))]
    pub submission_id: String,
    /// 幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 采购变更生效结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseChangeEffectResult {
    /// 变更单主键。
    pub change_id: String,
    /// 形成的新采购版本。
    pub revision_id: String,
    /// 新版本号。
    pub revision_no: u32,
    /// 追加的应付差额分录。
    pub payable_delta_entry_id: Option<String>,
    /// 采购单新乐观锁版本。
    pub purchase_order_lock_version: u64,
    /// 业务引用。
    pub reference: String,
}

/// 采购变更单列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PurchaseChangeOrderListParams {
    /// 原采购单筛选。
    pub purchase_order_id: Option<String>,
    /// 状态筛选。
    pub status: Option<String>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 采购变更单视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseChangeOrderView {
    /// 变更单主键。
    pub id: String,
    /// 原采购单。
    pub purchase_order_id: String,
    /// 基准版本。
    pub base_revision_id: String,
    /// 变更原因。
    pub reason: String,
    /// 状态。
    pub status: String,
    /// 当前不可变目标提交。
    pub current_submission_id: Option<String>,
    /// 生效后形成的新采购版本。
    pub effective_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 统一只读审批结构。客户端不得据此选择定义或审批人。
    pub approval: DocumentApprovalView,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use entities::common::time::BusinessDate;
    use entities::money::Quantity;
    use entities::purchase_order::PurchaseOrderStatus;
    use serde_json::json;
    use validator::Validate;

    use super::{
        normalize_sort, submit_request_shape, CreatePurchaseOrderFromBasisRequest,
        CreatePurchaseOrderLineRequest, CreatePurchaseOrdersFromSourcingRequest, PurchaseLineType,
        PurchaseOrderListParams, SavePurchaseOrderDraftRequest, SavePurchaseOrderLine,
        SavePurchaseOrderLinePatch, SortDir, SourcingLineAssignment, SubmitPurchaseOrderRequest,
        SupplySourceType, VoidPurchaseOrderRequest,
    };
    use crate::errors::Error;
    use entities::purchase_order::{PurchaseOrderSubmissionLine, RequestedLine, SourcingAssignment};

    /// 作废路径指纹金值：锁定历史算法绝对摘要。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 指纹算法或载荷形态变化导致摘要漂移时测试失败。
    #[test]
    fn void_fingerprint_golden() {
        let request = VoidPurchaseOrderRequest {
            expected_lock_version: 4,
            reason: " 重复采购 ".to_string(),
            idempotency_key: "void-key-1".to_string(),
        };
        assert_eq!(
            request.request_fingerprint("po-1").unwrap(),
            "bcb616e27bcce0b0d3e09c5a26d1ac15d386f7672ebcf85757b0c52d10761b8b"
        );
    }

    /// 保存草稿路径指纹金值：锁定历史算法绝对摘要。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 指纹算法或载荷形态变化导致摘要漂移时测试失败。
    #[test]
    fn save_fingerprint_golden() {
        let request = SavePurchaseOrderDraftRequest {
            expected_lock_version: 3,
            payment_term_code: Some(" NET-30 ".to_string()),
            lines: vec![SavePurchaseOrderLine {
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: None,
                sku_id: Some("sku-1".to_string()),
                sku_revision_id: Some("sku-rev-1".to_string()),
                product_name: Some("产品".to_string()),
                specification: None,
                quantity: Some("1".to_string()),
                base_unit_code: Some("EA".to_string()),
                unit_cost_gross: Some("10".to_string()),
                input_tax_rate: Some("0.13".to_string()),
                expected_delivery_date: Some("2026-08-25".to_string()),
                sales_order_line_id: Some("sales-line-1".to_string()),
                sales_order_revision_line_id: Some("sales-revision-line-1".to_string()),
                sales_order_submission_line_id: Some("sales-submission-line-1".to_string()),
                allocated_quantity: Some("1".to_string()),
                gross_amount: None,
            }],
            line_patches: vec![],
            idempotency_key: "save-key-1".to_string(),
        };
        assert_eq!(
            request.request_fingerprint("po-1").unwrap(),
            "28ffd5720e0a0bd90a07358d39f81a313c3b10746c037a927ceb4b635068a8f5"
        );
    }

    /// 依据创建路径指纹金值：锁定历史算法绝对摘要。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 指纹算法或载荷形态变化导致摘要漂移时测试失败。
    #[test]
    fn create_fingerprint_golden() {
        let request = CreatePurchaseOrderFromBasisRequest {
            work_item_id: "wi-1".to_string(),
            basis_id: "basis-1".to_string(),
            purchase_type: entities::purchase_order::PurchaseType::Physical,
            payment_term_code: "NET-30".to_string(),
            target_warehouse_id: Some("wh-1".to_string()),
            lines: vec![CreatePurchaseOrderLineRequest {
                sales_order_line_id: "sol-1".to_string(),
                quantity: "10".to_string(),
                expected_delivery_date: "2026-08-25".to_string(),
            }],
            idempotency_key: "create-key-1".to_string(),
        };
        let lines = vec![RequestedLine {
            sales_order_line_id: "sol-1".to_string(),
            quantity: Quantity::from_str("10").unwrap(),
            expected_delivery_date: BusinessDate::from_str("2026-08-25").unwrap(),
        }];
        assert_eq!(
            request.request_fingerprint(&lines),
            "e225f0fda8a15fdf6b332cf1bb97ee893e9f0fd9092367986446a4e3f87f5a2f"
        );
    }

    /// 选源创建路径指纹金值：锁定历史算法绝对摘要。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 指纹算法或载荷形态变化导致摘要漂移时测试失败。
    #[test]
    fn sourcing_fingerprint_golden() {
        let request = CreatePurchaseOrdersFromSourcingRequest {
            work_item_id: "wi-1".to_string(),
            sales_order_id: "so-1".to_string(),
            lines: vec![
                SourcingLineAssignment {
                    sales_order_line_id: "sol-1".to_string(),
                    basis_id: "basis-1".to_string(),
                    source_type: SupplySourceType::Purchase,
                    target_warehouse_id: Some("wh-1".to_string()),
                    quantity: "10".to_string(),
                    expected_delivery_date: "2026-08-25".to_string(),
                },
                SourcingLineAssignment {
                    sales_order_line_id: "sol-2".to_string(),
                    basis_id: "basis-2".to_string(),
                    source_type: SupplySourceType::ExistingStock,
                    target_warehouse_id: None,
                    quantity: "5".to_string(),
                    expected_delivery_date: "2026-08-26".to_string(),
                },
            ],
            idempotency_key: "sourcing-key-1".to_string(),
        };
        let assignments = vec![
            SourcingAssignment {
                sales_order_line_id: "sol-1".to_string(),
                basis_id: "basis-1".to_string(),
                source_type: SupplySourceType::Purchase,
                target_warehouse_id: Some("wh-1".to_string()),
                quantity: Quantity::from_str("10").unwrap(),
                expected_delivery_date: BusinessDate::from_str("2026-08-25").unwrap(),
            },
            SourcingAssignment {
                sales_order_line_id: "sol-2".to_string(),
                basis_id: "basis-2".to_string(),
                source_type: SupplySourceType::ExistingStock,
                target_warehouse_id: None,
                quantity: Quantity::from_str("5").unwrap(),
                expected_delivery_date: BusinessDate::from_str("2026-08-26").unwrap(),
            },
        ];
        assert_eq!(
            request.request_fingerprint(&assignments).unwrap(),
            "189640d37d07d808e3fc76001481775cade3f948f9873ddfdf9af4ecd87cfa1b"
        );
    }

    /// 提交路径指纹金值：锁定历史算法绝对摘要。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 指纹算法或形态编码变化导致摘要漂移时测试失败。
    #[test]
    fn submit_fingerprint_golden() {
        let request = SubmitPurchaseOrderRequest {
            expected_lock_version: 3,
            payment_term_code: Some("NET-30".to_string()),
            line_patches: vec![SavePurchaseOrderLinePatch {
                line_id: "line-1".to_string(),
                line_type: PurchaseLineType::ItemService,
                quantity: Some("10".to_string()),
                unit_cost_gross: Some("100.00".to_string()),
                input_tax_rate: Some("0.13".to_string()),
            }],
            idempotency_key: "submit-key-1".to_string(),
        };
        assert_eq!(
            request.request_fingerprint("po-1"),
            "377e75901732461d599f88b8297478c5c9bb9c0dfbe34d0506b701f12d4f1ee6"
        );
    }

    /// 提交形态文本必须与历史 Debug 派生字节逐字一致。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 显式编码与历史 Debug 输出不一致时测试失败。
    #[test]
    fn submit_shape_matches_historical_debug_bytes() {
        let payment_term_code = Some("NET-30".to_string());
        let line_patches = vec![SavePurchaseOrderLinePatch {
            line_id: "line-1".to_string(),
            line_type: PurchaseLineType::ItemService,
            quantity: Some("10".to_string()),
            unit_cost_gross: Some("100.00".to_string()),
            input_tax_rate: Some("0.13".to_string()),
        }];
        assert_eq!(
            submit_request_shape(&payment_term_code, &line_patches),
            format!("{:?}|{:?}", payment_term_code, line_patches)
        );
        let empty_term = None;
        let empty_patches = vec![];
        assert_eq!(
            submit_request_shape(&empty_term, &empty_patches),
            format!("{:?}|{:?}", empty_term, empty_patches)
        );
        let logistics = vec![SavePurchaseOrderLinePatch {
            line_id: "line-2".to_string(),
            line_type: PurchaseLineType::LogisticsFee,
            quantity: None,
            unit_cost_gross: Some("50".to_string()),
            input_tax_rate: None,
        }];
        assert_eq!(
            submit_request_shape(&None, &logistics),
            format!("{:?}|{:?}", None::<String>, logistics)
        );
    }

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("amount".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" purchase_no ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "purchase_no"],
        )
        .unwrap();
        assert_eq!(field, "purchase_no");
        assert_eq!(direction, SortDir::Asc);

        let (field, direction) = normalize_sort(&None, &None, &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Desc);
    }

    #[test]
    fn list_params_normalize_paging_filters_and_sort_defaults() {
        let params = PurchaseOrderListParams {
            q: Some(" PO-2026 ".to_string()),
            sales_order_id: None,
            supplier_id: Some(" sup-1 ".to_string()),
            status: Some(PurchaseOrderStatus::PendingFinanceReview),
            review_status: None,
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.q.as_deref(), Some("PO-2026"));
        assert_eq!(query.supplier_id.as_deref(), Some("sup-1"));
        assert_eq!(query.status, Some(PurchaseOrderStatus::PendingFinanceReview));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params = PurchaseOrderListParams {
            q: None,
            sales_order_id: None,
            supplier_id: None,
            status: None,
            review_status: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn create_request_rejects_blank_basis_and_keys() {
        let request: super::CreatePurchaseOrderFromBasisRequest = serde_json::from_value(json!({
            "work_item_id": "wi-1",
            "basis_id": "   ",
            "purchase_type": "PHYSICAL",
            "payment_term_code": "NET-30",
            "lines": [{
                "sales_order_line_id": "sol-1",
                "quantity": "1",
                "expected_delivery_date": "2026-09-01"
            }],
            "idempotency_key": "k-1",
        }))
        .unwrap();
        assert!(request.validate().is_err());
    }

    #[test]
    fn sourcing_line_defaults_legacy_requests_to_purchase() {
        let line: super::SourcingLineAssignment = serde_json::from_value(json!({
            "sales_order_line_id": "sol-1",
            "basis_id": "basis-1",
            "quantity": "1",
            "expected_delivery_date": "2026-09-01"
        }))
        .unwrap();
        assert_eq!(line.source_type, SupplySourceType::Purchase);
    }

    /// 构造当前草稿商品行。
    fn draft_line(id: &str, stable_line_id: &str, quantity: &str) -> PurchaseOrderSubmissionLine {
        use entities::ids::{
            ProcurementConfirmationLineId, PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId,
            SalesOrderLineId, SalesOrderRevisionLineId, SalesOrderSubmissionLineId, SkuId, SkuRevisionId,
        };
        use entities::money::{Rate, UnitPrice};
        use entities::purchase_order::PurchaseOrderSubmissionLineData;
        let quantity = Quantity::from_str(quantity).unwrap();
        let (gross, net, tax) = entities::money::line_amounts(
            UnitPrice::from_str("5").unwrap(),
            quantity,
            Rate::from_str("0").unwrap(),
        );
        PurchaseOrderSubmissionLine::new(
            PurchaseOrderSubmissionLineId::new(id),
            PurchaseOrderSubmissionLineData {
                purchase_order_submission_id: PurchaseOrderSubmissionId::new("sub-1"),
                line_no: 1,
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("pcl-1")),
                sku_id: Some(SkuId::new("sku-1")),
                sku_revision_id: Some(SkuRevisionId::new("skur-1")),
                product_name_snapshot: Some("商品".to_string()),
                specification_snapshot: Some("规格".to_string()),
                quantity: Some(quantity),
                base_unit_code: Some("件".to_string()),
                unit_cost_gross: Some(UnitPrice::from_str("5").unwrap()),
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
                input_tax_rate: Some(Rate::from_str("0").unwrap()),
                expected_delivery_date: None,
                sales_order_line_id: Some(SalesOrderLineId::new(stable_line_id)),
                sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new("sorl-1")),
                sales_order_submission_line_id: Some(SalesOrderSubmissionLineId::new("sosl-1")),
                allocated_quantity: Some(quantity),
            },
        )
        .unwrap()
    }

    /// 构造空行载荷的保存请求。
    fn draft_request(line_patches: Vec<SavePurchaseOrderLinePatch>) -> SavePurchaseOrderDraftRequest {
        SavePurchaseOrderDraftRequest {
            expected_lock_version: 1,
            payment_term_code: None,
            lines: vec![],
            line_patches,
            idempotency_key: "save-key-1".to_string(),
        }
    }

    /// 完整行与行补丁必须且只能提供一种。
    #[test]
    fn save_request_shape_requires_exactly_one_line_payload() {
        let empty = draft_request(vec![]);
        assert!(empty.ensure_shape().is_err());

        let mut both = empty.clone();
        both.lines = vec![SavePurchaseOrderLine {
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: None,
            sku_id: None,
            sku_revision_id: None,
            product_name: None,
            specification: None,
            quantity: Some("1".to_string()),
            base_unit_code: None,
            unit_cost_gross: None,
            input_tax_rate: None,
            expected_delivery_date: None,
            sales_order_line_id: None,
            sales_order_revision_line_id: None,
            sales_order_submission_line_id: None,
            allocated_quantity: Some("1".to_string()),
            gross_amount: None,
        }];
        both.line_patches = vec![SavePurchaseOrderLinePatch {
            line_id: "subl-1".to_string(),
            line_type: PurchaseLineType::ItemService,
            quantity: None,
            unit_cost_gross: None,
            input_tax_rate: None,
        }];
        assert!(both.ensure_shape().is_err());

        let mut lines_only = both.clone();
        lines_only.line_patches = vec![];
        assert!(lines_only.ensure_shape().is_ok());

        let mut patches_only = both.clone();
        patches_only.lines = vec![];
        assert!(patches_only.ensure_shape().is_ok());

        let mut blank_patch = patches_only.clone();
        blank_patch.line_patches[0].line_id = "  ".to_string();
        assert!(blank_patch.ensure_shape().is_err());
    }

    /// 完整行路径原样返回，补丁路径合并冻结字段与可编辑字段。
    #[test]
    fn resolve_lines_merges_patches_with_frozen_draft_fields() {
        let existing = vec![draft_line("subl-1", "sol-1", "2")];
        let mut request = draft_request(vec![SavePurchaseOrderLinePatch {
            line_id: " subl-1 ".to_string(),
            line_type: PurchaseLineType::ItemService,
            quantity: Some(" 5 ".to_string()),
            unit_cost_gross: Some("8".to_string()),
            input_tax_rate: Some("0.13".to_string()),
        }]);
        let merged = request.resolve_lines(&existing).unwrap();
        assert_eq!(merged.len(), 1);
        let line = &merged[0];
        // 补丁字段原样合并；空白由领域校验统一规范化。
        assert_eq!(line.quantity.as_deref(), Some(" 5 "));
        assert_eq!(line.allocated_quantity.as_deref(), Some(" 5 "));
        assert_eq!(line.unit_cost_gross.as_deref(), Some("8"));
        assert_eq!(line.input_tax_rate.as_deref(), Some("0.13"));
        assert_eq!(line.sku_id.as_deref(), Some("sku-1"));
        assert_eq!(line.sales_order_line_id.as_deref(), Some("sol-1"));
        assert_eq!(line.sales_order_submission_line_id.as_deref(), Some("sosl-1"));
        assert_eq!(line.product_name.as_deref(), Some("商品"));

        request.line_patches[0].line_id = "subl-1".to_string();
        request.lines = vec![SavePurchaseOrderLine {
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: None,
            sku_id: Some("sku-9".to_string()),
            sku_revision_id: None,
            product_name: None,
            specification: None,
            quantity: Some("9".to_string()),
            base_unit_code: None,
            unit_cost_gross: None,
            input_tax_rate: None,
            expected_delivery_date: None,
            sales_order_line_id: Some("sol-9".to_string()),
            sales_order_revision_line_id: None,
            sales_order_submission_line_id: None,
            allocated_quantity: Some("9".to_string()),
            gross_amount: None,
        }];
        let full = request.resolve_lines(&existing).unwrap();
        assert_eq!(full[0].quantity.as_deref(), Some("9"));
        assert_eq!(full[0].sales_order_line_id.as_deref(), Some("sol-9"));
    }

    /// 补丁必须覆盖全部当前草稿行且不得重复。
    #[test]
    fn resolve_line_patches_rejects_partial_and_duplicate_patches() {
        let existing = vec![
            draft_line("subl-1", "sol-1", "2"),
            draft_line("subl-2", "sol-2", "1"),
        ];
        let patch = |line_id: &str| SavePurchaseOrderLinePatch {
            line_id: line_id.to_string(),
            line_type: PurchaseLineType::ItemService,
            quantity: None,
            unit_cost_gross: None,
            input_tax_rate: None,
        };
        let partial = draft_request(vec![patch("subl-1")]);
        assert!(SavePurchaseOrderLinePatch::resolve_all(&partial.line_patches, &existing).is_err());

        let duplicate = draft_request(vec![patch("subl-1"), patch("subl-1")]);
        assert!(SavePurchaseOrderLinePatch::resolve_all(&duplicate.line_patches, &existing).is_err());

        let unknown = draft_request(vec![patch("subl-9"), patch("subl-2")]);
        assert!(matches!(
            SavePurchaseOrderLinePatch::resolve_all(&unknown.line_patches, &existing),
            Err(Error::ConflictError(_))
        ));

        let mut changed_type = draft_request(vec![patch("subl-1"), patch("subl-2")]);
        changed_type.line_patches[0].line_type = PurchaseLineType::LogisticsFee;
        assert!(SavePurchaseOrderLinePatch::resolve_all(&changed_type.line_patches, &existing).is_err());
    }

    /// 草稿行编辑请求保留全部字段供领域校验。
    #[test]
    fn to_draft_edit_carries_all_line_fields() {
        let line = SavePurchaseOrderLine {
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some("pcl-1".to_string()),
            sku_id: Some("sku-1".to_string()),
            sku_revision_id: Some("skur-1".to_string()),
            product_name: Some("商品".to_string()),
            specification: Some("规格".to_string()),
            quantity: Some("5".to_string()),
            base_unit_code: Some("件".to_string()),
            unit_cost_gross: Some("8".to_string()),
            input_tax_rate: Some("0.13".to_string()),
            expected_delivery_date: Some("2026-09-01".to_string()),
            sales_order_line_id: Some("sol-1".to_string()),
            sales_order_revision_line_id: Some("sorl-1".to_string()),
            sales_order_submission_line_id: Some("sosl-1".to_string()),
            allocated_quantity: Some("5".to_string()),
            gross_amount: None,
        };
        let edit = line.to_draft_edit();
        assert_eq!(edit.line_type, PurchaseLineType::ItemService);
        assert_eq!(edit.quantity.as_deref(), Some("5"));
        assert_eq!(edit.allocated_quantity.as_deref(), Some("5"));
        assert_eq!(edit.sku_id.as_deref(), Some("sku-1"));
        assert_eq!(edit.sales_order_line_id.as_deref(), Some("sol-1"));
        assert_eq!(edit.sales_order_submission_line_id.as_deref(), Some("sosl-1"));
    }
}
