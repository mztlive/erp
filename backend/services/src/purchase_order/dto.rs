//! 域 D15 `purchase_order` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳，业务日期 `YYYY-MM-DD`；
//! 金额/数量以字符串传输（`entities::money` 的 serde 字符串形态）。
//!
//! 与 `erp-client/features/purchase-orders/api.ts` 的差异（契约变更）：
//! - 列表状态枚举沿用实体代码（`PENDING_FINANCE_REVIEW`/`PARTIALLY_EXECUTED`/
//!   `VOIDED`），前端 mock 使用 `PENDING_REVIEW`/`PARTIAL`/`VOID`；
//! - 列表/详情不返回 `sales_order_no`（销售单号属 D13，不在 D15 跨域依赖清单，
//!   前端以 `sales_order_id` 标识来源）；
//! - 草稿 `purchase_no` 为空，首次提交事务分配不可复用正式号；
//! - 表单类写操作（创建/保存/提交/审核）统一返回稳定业务结果，不再返回
//!   `FormalActionResponse` 信封（由 HTTP 统一信封承载）。

use entities::purchase_order::{
    FulfillmentResponsibility, ProgressStatus, PurchaseLineType, PurchaseOrderStatus, PurchaseReviewStatus,
    PurchaseType,
};
use entities::work_item::{AssignmentSource, WorkItemStatus, WorkItemType};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};
use crate::work_item::{ProcessingState, WorkItemAllowedAction};

/// 采购单列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const PURCHASE_ORDER_SORT_FIELDS: &[&str] = &["created_at", "purchase_no"];

/// 排序方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    /// 升序。
    Asc,
    /// 降序。
    Desc,
}

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
pub(crate) fn normalize_sort(
    sort_by: &Option<String>,
    sort_dir: &Option<String>,
    allowed_fields: &'static [&'static str],
) -> Result<(&'static str, SortDir)> {
    let sort_by = match sort_by
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(field) => allowed_fields
            .iter()
            .find(|allowed| **allowed == field)
            .copied()
            .ok_or_else(|| Error::ValidationError(format!("不支持的排序字段: {field}")))?,
        None => "created_at",
    };
    let sort_dir = match sort_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some("asc") => SortDir::Asc,
        Some("desc") => SortDir::Desc,
        Some(other) => return Err(Error::ValidationError(format!("非法排序方向: {other}"))),
        None => SortDir::Desc,
    };
    Ok((sort_by, sort_dir))
}

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
#[derive(Debug, Clone, Serialize)]
pub struct PageView<T> {
    /// 当前页数据。
    pub items: Vec<T>,
    /// 满足筛选条件的总数（非当前页条数）。
    pub total: i64,
    /// 当前页码（1 起）。
    pub page: u64,
    /// 请求的分页大小。
    pub page_size: u32,
}

/// 校验文本去除首尾空白后非空（validator 的 `length(min=1)` 对纯空白字符串不生效）。
fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

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
    /// 唯一供应商。
    pub supplier_id: String,
    /// 供应商名称（D07 主体修订快照）。
    pub supplier_name: String,
    /// 采购类型。
    pub purchase_type: PurchaseType,
    /// 付款条件代码（实体 `payment_term_code`）。
    pub payment_term_code: String,
    /// 采购单负责人展示名（创建人账号姓名；账号不存在时回落账号 ID）。
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
    /// 商品行对应的销售提交行。
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

/// 采购单详情中的财务审核责任事实。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseReviewWorkItemView {
    /// 待办稳定身份。
    pub work_item_id: String,
    /// 固定任务类型。
    pub work_item_type: WorkItemType,
    /// 待办自身的乐观锁版本。
    pub task_version: u64,
    /// 锁定的不可变采购提交版本。
    pub subject_version: String,
    /// 待办生命周期状态。
    pub status: WorkItemStatus,
    /// 责任分派模式。
    pub assignment_source_unused: AssignmentSource,
    /// 责任角色。
    pub owner_role: String,
    /// 责任组织。
    pub owner_organization_id: String,
    /// 当前个人责任人；责任池未开始处理时为空。
    pub owner_user_id: Option<String>,
    /// 当前处理状态；当前非审批步骤待办固定为 `READY`。
    pub processing_state: ProcessingState,
    /// 服务端处理阻断摘要。
    pub action_blockers: Vec<PurchaseActionBlockerView>,
    /// 仅由通用任务责任协议执行的动作。
    pub responsibility_actions: Vec<WorkItemAllowedAction>,
    /// 仅由 W08 强类型审核命令执行的领域动作。
    pub domain_allowed_actions: Vec<PurchaseReviewDomainAction>,
}

/// 采购财务审核工作面允许提交的领域决定。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PurchaseReviewDomainAction {
    /// 提交审核通过决定。
    Approve,
    /// 提交审核驳回决定。
    Reject,
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
    /// 当前开放的财务审核责任；统一审批后为空。
    pub review_work_item: Option<PurchaseReviewWorkItemView>,
    /// 统一只读审批结构。客户端不得据此选择定义或审批人。
    pub approval: DocumentApprovalView,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
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
    /// 当前审批人。
    pub current_assignee: Option<String>,
    /// 最近驳回原因。
    pub latest_rejection: Option<String>,
}

/// 有界历史项。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalHistoryItemView {
    /// 执行主键。
    pub execution_id: String,
    /// 轮次。
    pub round_no: u32,
    /// 节点键。
    pub node_key: String,
    /// 结束结果。
    pub result: String,
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

/// 采购创建依据行视图（W07 已确认分行 + 销售提交行归属）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CreationBasisLineView {
    /// 采购二次确认分行。
    pub procurement_confirmation_line_id: String,
    /// 被确认的销售提交行。
    pub sales_order_submission_line_id: String,
    /// 确认供应商。
    pub supplier_id: String,
    /// 确认可供数量。
    pub confirmed_quantity: String,
    /// 最新含税成本。
    pub latest_cost_gross: String,
    /// 进项税率。
    pub input_tax_rate: String,
    /// 预计交期（`YYYY-MM-DD`）。
    pub expected_delivery_date: String,
    /// 商品名称快照（销售提交行侧联查，缺失时为空）。
    pub product_name: Option<String>,
    /// 规格快照。
    pub specification: Option<String>,
    /// 含税行金额（按确认数量与成本逐行舍入）。
    pub gross_amount: String,
}

/// 采购创建依据视图（已通过且未完全消费的采购确认）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CreationBasisView {
    /// 采购确认批次（W07 结果）。
    pub basis_id: String,
    /// 被确认的销售单。
    pub sales_order_id: String,
    /// 被确认的销售提交。
    pub submission_id: String,
    /// 供应商。
    pub supplier_id: String,
    /// 供应商名称。
    pub supplier_name: String,
    /// 付款条件（供应商商业资料快照，缺省 `NET-30`）。
    pub payment_term_code: String,
    /// 可拆入本单的已确认分行。
    pub lines: Vec<CreationBasisLineView>,
    /// 含税行汇总（只汇总已舍入行金额）。
    pub estimated_gross: String,
}

/// 依据创建采购单请求（客户端形状 `{basisId, idempotencyKey}` 的扩展：拆单维度必填）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreatePurchaseOrderFromBasisRequest {
    /// 采购创建依据（已通过的采购确认）。
    #[validate(custom(function = "non_blank", message = "创建依据不能为空"))]
    pub basis_id: String,
    /// 采购类型。
    pub purchase_type: PurchaseType,
    /// 付款条件（受控码表代码）。
    #[validate(custom(function = "non_blank", message = "付款条件不能为空"))]
    pub payment_term_code: String,
    /// 幂等键（同一依据重复创建返回同一采购单）。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
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
    /// 是否复用已有草稿（幂等重放）。
    pub replayed: bool,
    /// 业务引用。
    pub reference: String,
}

/// 保存采购草稿请求（表头 + 完整行替换；金额由服务端逐行计算）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SavePurchaseOrderDraftRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 付款条件；缺省表示不修改。
    pub payment_term_code: Option<String>,
    /// 完整行集合（整表替换草稿明细）。
    pub lines: Vec<SavePurchaseOrderLine>,
    /// 幂等键（同内容重复保存返回同一结果）。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
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
    /// 商品行对应的销售提交行。
    pub sales_order_submission_line_id: Option<String>,
    /// 商品行对应的分配数量。
    pub allocated_quantity: Option<String>,
    /// 物流费用行含税金额（物流行为必填；商品行忽略）。
    pub gross_amount: Option<String>,
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

/// 提交财务审核请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SubmitPurchaseOrderRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 幂等键（重复提交只产生一条正式提交）。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    #[validate(length(max = 128, message = "幂等键过长"))]
    pub idempotency_key: String,
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

/// 采购财务审核决定类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PurchaseOrderReviewDecisionResult {
    /// 审核通过并形成正式采购版本、应付与成本事实。
    Approved,
    /// 审核驳回并把采购对象恢复为可编辑草稿。
    Rejected,
}

impl PurchaseOrderReviewDecisionResult {
    /// 返回稳定协议代码。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "APPROVED",
            Self::Rejected => "REJECTED",
        }
    }
}

/// W08 财务审核的完整领域决定。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PurchaseOrderReviewDecisionCommand {
    /// 路径必须指向的采购单。
    #[validate(custom(function = "non_blank", message = "采购单ID不能为空"))]
    #[validate(length(max = 128, message = "采购单ID过长"))]
    pub purchase_order_id: String,
    /// 待审核的不可变提交。
    #[validate(custom(function = "non_blank", message = "提交ID不能为空"))]
    #[validate(length(max = 128, message = "提交ID过长"))]
    pub submission_id: String,
    /// 期望的采购单乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_purchase_order_lock_version: u64,
    /// 唯一审核结论分支。
    pub review_result: PurchaseOrderReviewDecisionResult,
    /// 驳回原因；仅 `REJECTED` 分支必填，`APPROVED` 分支禁止携带。
    #[validate(length(max = 64, message = "驳回原因代码过长"))]
    pub reason_code: Option<String>,
    /// 补充说明。
    #[validate(length(max = 512, message = "补充说明过长"))]
    pub comment: Option<String>,
}

impl PurchaseOrderReviewDecisionCommand {
    /// 校验审核结论分支专属字段。
    pub(crate) fn validate_branch(&self) -> Result<()> {
        match (self.review_result, self.reason_code.as_deref()) {
            (PurchaseOrderReviewDecisionResult::Approved, None) => Ok(()),
            (PurchaseOrderReviewDecisionResult::Approved, Some(_)) => {
                Err(Error::ValidationError("审核通过分支不得携带驳回原因".to_string()))
            }
            (PurchaseOrderReviewDecisionResult::Rejected, Some(reason)) if !reason.trim().is_empty() => {
                Ok(())
            }
            (PurchaseOrderReviewDecisionResult::Rejected, _) => Err(Error::ValidationError(
                "审核驳回分支必须携带结构化原因代码".to_string(),
            )),
        }
    }
}

/// W08 唯一采购财务审核命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ReviewPurchaseOrderCommand {
    /// 当前正式审核待办。
    #[validate(custom(function = "non_blank", message = "审核待办ID不能为空"))]
    #[validate(length(max = 128, message = "审核待办ID过长"))]
    pub work_item_id: String,
    /// 最近一次查询返回的不透明任务版本。
    #[validate(custom(function = "non_blank", message = "待办版本不能为空"))]
    #[validate(length(max = 20, message = "待办版本过长"))]
    pub expected_task_version: String,
    /// 待办冻结的不可变采购提交版本。
    #[validate(custom(function = "non_blank", message = "提交版本不能为空"))]
    #[validate(length(max = 128, message = "提交版本过长"))]
    pub expected_subject_version: String,
    /// 完整领域决定；审核结论不得出现在命令顶层或 URL 路径中。
    #[validate(nested)]
    pub decision: PurchaseOrderReviewDecisionCommand,
    /// 幂等键（同一键重试不得重复形成审核事实）。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    #[validate(length(max = 128, message = "幂等键过长"))]
    pub idempotency_key: String,
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
    use super::{
        normalize_sort, PurchaseOrderListParams, PurchaseOrderReviewDecisionResult,
        ReviewPurchaseOrderCommand, SortDir,
    };
    use entities::purchase_order::PurchaseOrderStatus;
    use serde_json::json;
    use validator::Validate;

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
            "basis_id": "   ",
            "purchase_type": "PHYSICAL",
            "payment_term_code": "NET-30",
            "idempotency_key": "k-1",
        }))
        .unwrap();
        assert!(request.validate().is_err());
    }

    #[test]
    fn review_command_accepts_only_nested_decision_wire() {
        let command: ReviewPurchaseOrderCommand = serde_json::from_value(json!({
            "work_item_id": "wi-1",
            "expected_task_version": "2",
            "expected_subject_version": "submission-1",
            "decision": {
                "purchase_order_id": "po-1",
                "submission_id": "submission-1",
                "expected_purchase_order_lock_version": 3,
                "review_result": "REJECTED",
                "reason_code": "COST_TAX"
            },
            "idempotency_key": "idem-1"
        }))
        .unwrap();

        assert_eq!(
            command.decision.review_result,
            PurchaseOrderReviewDecisionResult::Rejected
        );
        assert!(command.validate().is_ok());
        assert!(command.decision.validate_branch().is_ok());

        let legacy = json!({
            "submission_id": "submission-1",
            "work_item_id": "wi-1",
            "expected_task_version": 2,
            "expected_subject_version": "submission-1",
            "expected_lock_version": 3,
            "reason_code": "COST_TAX",
            "idempotency_key": "idem-1"
        });
        assert!(serde_json::from_value::<ReviewPurchaseOrderCommand>(legacy).is_err());
    }

    #[test]
    fn review_decision_rejects_branch_field_drift() {
        let approved_with_reason: ReviewPurchaseOrderCommand = serde_json::from_value(json!({
            "work_item_id": "wi-1",
            "expected_task_version": "2",
            "expected_subject_version": "submission-1",
            "decision": {
                "purchase_order_id": "po-1",
                "submission_id": "submission-1",
                "expected_purchase_order_lock_version": 3,
                "review_result": "APPROVED",
                "reason_code": "OTHER"
            },
            "idempotency_key": "idem-1"
        }))
        .unwrap();
        assert!(approved_with_reason.decision.validate_branch().is_err());

        let rejected_without_reason: ReviewPurchaseOrderCommand = serde_json::from_value(json!({
            "work_item_id": "wi-1",
            "expected_task_version": "2",
            "expected_subject_version": "submission-1",
            "decision": {
                "purchase_order_id": "po-1",
                "submission_id": "submission-1",
                "expected_purchase_order_lock_version": 3,
                "review_result": "REJECTED"
            },
            "idempotency_key": "idem-2"
        }))
        .unwrap();
        assert!(rejected_without_reason.decision.validate_branch().is_err());
    }
}
