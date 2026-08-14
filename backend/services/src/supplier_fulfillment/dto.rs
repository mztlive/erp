//! 域 D32 `supplier_fulfillment` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；金额/数量使用
//! `entities::money` 定点类型（serde_json 下自动字符串化）。

use entities::common::source::SourceType;
use entities::ids::{
    CostAllocationId, CostEntryId, MallAfterSalesRequestId, MallAfterSalesRequestLineId, MallOrderId,
    MallOrderItemId, PayableEntryId, PaymentAllocationId, SupplierAccountId, SupplierApiConnectionId,
    SupplierFulfillmentItemId, SupplierFulfillmentOrderId, SupplierOfferingRevisionId, SupplierOrderActionId,
    WorkItemId,
};
use entities::money::{Amount, Quantity, Rate, UnitPrice};
use entities::supplier_fulfillment::{
    AllocationAction, CancelStatus, FulfillmentStatus, RefundStatus, SupplierFulfillmentOrder,
    SupplierOrderAction, SupplierOrderActionStatus, SupplierOrderActionType, SupplierOrderStatusHistory,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};
use crate::work_item::WorkItemView;

/// 供应商履约订单列表允许的排序字段白名单（Service 层校验，禁止任意字段透传）。
pub(crate) const FULFILLMENT_ORDER_SORT_FIELDS: &[&str] =
    &["created_at", "submitted_at", "accepted_at", "completed_at"];

/// 排序方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SortDir {
    /// 升序。
    Asc,
    /// 降序。
    Desc,
}

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验）。
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
///
/// # 说明
/// 跨域复用入口：D33 的列表参数同样使用本函数（后续若出现第三处使用，
/// 应走地基修订把该逻辑下沉到 `services::query`）。
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

/// 供应商履约订单列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierFulfillmentOrderListParams {
    /// 固定供应商筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 履约主线状态筛选。
    pub fulfillment_status: Option<FulfillmentStatus>,
    /// 取消进度状态筛选。
    pub cancel_status: Option<CancelStatus>,
    /// 退款进度状态筛选。
    pub refund_status: Option<RefundStatus>,
    /// 供应商订单号模糊筛选（字面量、忽略大小写）。
    pub external_order_no: Option<String>,
    /// 来源商城订单筛选。
    pub mall_order_id: Option<MallOrderId>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`submitted_at`/`accepted_at`/`completed_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的供应商履约订单列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FulfillmentOrderListQuery {
    /// 固定供应商筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 履约主线状态筛选。
    pub fulfillment_status: Option<FulfillmentStatus>,
    /// 取消进度状态筛选。
    pub cancel_status: Option<CancelStatus>,
    /// 退款进度状态筛选。
    pub refund_status: Option<RefundStatus>,
    /// 供应商订单号模糊筛选。
    pub external_order_no: Option<String>,
    /// 来源商城订单筛选。
    pub mall_order_id: Option<MallOrderId>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SupplierFulfillmentOrderListParams {
    /// 归一化供应商履约订单列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<FulfillmentOrderListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, FULFILLMENT_ORDER_SORT_FIELDS)?;
        Ok(FulfillmentOrderListQuery {
            supplier_id: self.supplier_id.clone(),
            fulfillment_status: self.fulfillment_status,
            cancel_status: self.cancel_status,
            refund_status: self.refund_status,
            external_order_no: normalized_text(self.external_order_no.as_deref()),
            mall_order_id: self.mall_order_id.clone(),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 供应商履约订单响应视图（契约形状：`id`/`fulfillment_order_no`/`mall_order_id`/
/// `supplier_id`/`connection_id`/`split_no`/三条状态/`external_order_no`/关键时间/
/// `version`/`created_at`）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierFulfillmentOrderView {
    /// 实体主键。
    pub id: String,
    /// ERP 供应商子订单号（下单幂等键）。
    pub fulfillment_order_no: String,
    /// 来源商城订单。
    pub mall_order_id: String,
    /// 固定供应商。
    pub supplier_id: String,
    /// 供应商 API 连接。
    pub connection_id: String,
    /// 确定性拆单序号。
    pub split_no: u32,
    /// 履约主线状态。
    pub fulfillment_status: FulfillmentStatus,
    /// 取消进度状态。
    pub cancel_status: CancelStatus,
    /// 退款进度状态。
    pub refund_status: RefundStatus,
    /// 供应商订单号。
    pub external_order_no: Option<String>,
    /// 提交给供应商的时间（秒级时间戳）。
    pub submitted_at: Option<i64>,
    /// 供应商接单时间（秒级时间戳）。
    pub accepted_at: Option<i64>,
    /// 履约完成时间（秒级时间戳）。
    pub completed_at: Option<i64>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<SupplierFulfillmentOrder> for SupplierFulfillmentOrderView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `order` - 供应商履约订单实体
    ///
    /// # 返回
    /// 返回响应视图（不暴露地址快照敏感值）。
    fn from(order: SupplierFulfillmentOrder) -> Self {
        Self {
            id: order.base.id,
            fulfillment_order_no: order.fulfillment_order_no,
            mall_order_id: order.mall_order_id.to_string(),
            supplier_id: order.supplier_id.to_string(),
            connection_id: order.connection_id.to_string(),
            split_no: order.split_no,
            fulfillment_status: order.fulfillment_status,
            cancel_status: order.cancel_status,
            refund_status: order.refund_status,
            external_order_no: order.external_order_no,
            submitted_at: order.submitted_at.map(|t| t.unix_secs()),
            accepted_at: order.accepted_at.map(|t| t.unix_secs()),
            completed_at: order.completed_at.map(|t| t.unix_secs()),
            version: order.base.version,
            created_at: order.base.created_at,
        }
    }
}

/// 供应商履约明细创建请求行。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PlaceFulfillmentItemRequest {
    /// 来源商城商品明细。
    pub mall_order_item_id: MallOrderItemId,
    /// 下单时固定的供给修订。
    pub supplier_offering_revision_id: SupplierOfferingRevisionId,
    /// 整条明细数量（SKU 基础单位，最多 6 位小数）。
    pub quantity: Quantity,
    /// 下单含税单位成本快照（最多 4 位小数）。
    pub unit_cost_snapshot_gross: UnitPrice,
    /// 下单成本进项税率（最多 6 位小数）。
    pub input_tax_rate: Rate,
}

/// 供应商下单请求（`fulfillment_order_no` 同时是下单幂等键，§6.19）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PlaceFulfillmentOrderRequest {
    /// ERP 供应商子订单号（唯一，也是下单幂等键；重复提交返回原订单不重复下单）。
    #[validate(custom(function = "non_blank", message = "供应商子订单号不能为空"))]
    pub fulfillment_order_no: String,
    /// 来源商城订单。
    pub mall_order_id: MallOrderId,
    /// 固定供应商。
    pub supplier_id: SupplierAccountId,
    /// 供应商 API 连接。
    pub connection_id: SupplierApiConnectionId,
    /// 同一商城订单、同一供应商下的确定性拆单序号。
    #[validate(range(min = 1, message = "拆单序号必须大于 0"))]
    pub split_no: u32,
    /// 履约地址快照加密值（调用方已加密，本接口按不透明值保存）。
    #[validate(custom(function = "non_blank", message = "履约地址快照不能为空"))]
    pub address_snapshot_encrypted: String,
    /// 履约地址快照 HMAC 查询指纹。
    #[validate(custom(function = "non_blank", message = "履约地址查询指纹不能为空"))]
    pub address_snapshot_fingerprint: String,
    /// 履约明细（至少一行）。
    #[validate(length(min = 1, message = "履约明细至少一行"))]
    pub items: Vec<PlaceFulfillmentItemRequest>,
}

/// 校验文本去除首尾空白后非空。
fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

/// 供应商履约明细响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierFulfillmentItemView {
    /// 实体主键。
    pub id: String,
    /// 所属供应商子订单。
    pub supplier_fulfillment_order_id: String,
    /// 来源商城商品明细。
    pub mall_order_item_id: String,
    /// 下单时固定的供给修订。
    pub supplier_offering_revision_id: String,
    /// 下单时固定的供应商侧订货 SKU 编码。
    pub supplier_sku_code_snapshot: String,
    /// 下单时固定的供应商侧商品编码。
    pub supplier_product_code_snapshot: Option<String>,
    /// 整条明细数量。
    pub quantity: Quantity,
    /// 下单含税单位成本快照。
    pub unit_cost_snapshot_gross: UnitPrice,
    /// 明细含税成本快照。
    pub cost_snapshot_total_gross: Amount,
    /// 下单成本进项税率。
    pub input_tax_rate: Rate,
}

/// 供应商订单状态历史响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierOrderStatusHistoryView {
    /// 实体主键。
    pub id: String,
    /// 原状态。
    pub previous_status: FulfillmentStatus,
    /// 新状态。
    pub new_status: FulfillmentStatus,
    /// 供应商状态版本。
    pub supplier_status_version: String,
    /// 业务发生时间（秒级时间戳）。
    pub occurred_at: i64,
    /// ERP 接收时间（秒级时间戳）。
    pub received_at: i64,
    /// 外部事件 ID。
    pub external_event_id: String,
    /// 来源。
    pub source_type: SourceType,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<SupplierOrderStatusHistory> for SupplierOrderStatusHistoryView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `history` - 状态历史实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(history: SupplierOrderStatusHistory) -> Self {
        Self {
            id: history.base.id,
            previous_status: history.previous_status,
            new_status: history.new_status,
            supplier_status_version: history.supplier_status_version,
            occurred_at: history.occurred_at.unix_secs(),
            received_at: history.received_at.unix_secs(),
            external_event_id: history.external_event_id,
            source_type: history.source_type,
            created_at: history.base.created_at,
        }
    }
}

/// 供应商动作响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierOrderActionView {
    /// 实体主键。
    pub id: String,
    /// 供应商子订单。
    pub supplier_fulfillment_order_id: String,
    /// 动作类型。
    pub action_type: SupplierOrderActionType,
    /// 商城售后申请。
    pub after_sales_request_id: Option<String>,
    /// 动作状态。
    pub status: SupplierOrderActionStatus,
    /// 供应商请求号。
    pub external_request_id: Option<String>,
    /// 脱敏请求摘要。
    pub request_summary: Option<String>,
    /// 脱敏响应摘要。
    pub response_summary: Option<String>,
    /// 重试次数。
    pub attempt_count: u32,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<SupplierOrderAction> for SupplierOrderActionView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `action` - 供应商动作实体
    ///
    /// # 返回
    /// 返回响应视图（不暴露完整幂等键，只透出动作身份与状态）。
    fn from(action: SupplierOrderAction) -> Self {
        Self {
            id: action.base.id,
            supplier_fulfillment_order_id: action.supplier_fulfillment_order_id.to_string(),
            action_type: action.action_type,
            after_sales_request_id: action.after_sales_request_id.map(|id| id.to_string()),
            status: action.status,
            external_request_id: action.external_request_id,
            request_summary: action.request_summary,
            response_summary: action.response_summary,
            attempt_count: action.attempt_count,
            created_at: action.base.created_at,
        }
    }
}

/// 供应商动作行响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierOrderActionLineView {
    /// 实体主键。
    pub id: String,
    /// 动作内行号。
    pub line_no: u32,
    /// 原商城售后申请行。
    pub after_sales_request_line_id: String,
    /// 本供应商履约明细。
    pub supplier_fulfillment_item_id: String,
    /// 本动作提交数量。
    pub quantity: Quantity,
    /// 本动作提交金额。
    pub amount: Amount,
}

/// 供应商退款事实响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierRefundFactView {
    /// 实体主键。
    pub id: String,
    /// 供应商子订单。
    pub supplier_fulfillment_order_id: String,
    /// 外部退款号。
    pub external_refund_no: String,
    /// 外部退款版本。
    pub external_refund_version: String,
    /// 实际退款金额。
    pub refund_amount: Amount,
    /// 实际退款时间（秒级时间戳）。
    pub refunded_at: i64,
    /// 来源事件 ID。
    pub source_event_id: String,
    /// 分配行。
    pub allocations: Vec<SupplierRefundAllocationView>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 供应商退款分配行响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierRefundAllocationView {
    /// 实体主键。
    pub id: String,
    /// 退款头内稳定分配序号。
    pub allocation_no: u32,
    /// 原供应商履约明细。
    pub supplier_fulfillment_item_id: String,
    /// 实际供应商退款数量。
    pub refund_quantity: Quantity,
    /// 含税成本冲减金额。
    pub gross_amount: Amount,
    /// 不含税成本冲减金额。
    pub net_amount: Amount,
    /// 税额冲减金额。
    pub tax_amount: Amount,
    /// 未付应付冲减金额。
    pub payable_reduction_amount: Amount,
    /// 已付现金退回拆分金额。
    pub cash_refund_amount: Amount,
    /// 分配动作。
    pub allocation_action: AllocationAction,
}

/// 供应商履约订单详情视图（订单 + 明细 + 动作 + 状态历史 + 退款事实）。
#[derive(Debug, Clone, Serialize)]
pub struct SupplierFulfillmentOrderDetailView {
    /// 订单头。
    pub order: SupplierFulfillmentOrderView,
    /// 履约明细。
    pub items: Vec<SupplierFulfillmentItemView>,
    /// 状态历史（按发生时间升序）。
    pub status_history: Vec<SupplierOrderStatusHistoryView>,
    /// 对供应商动作（按创建时间降序）。
    pub actions: Vec<SupplierOrderActionView>,
    /// 退款事实（含分配行）。
    pub refund_facts: Vec<SupplierRefundFactView>,
    /// 权威供应商名称；基础资料缺失时为空，禁止回退显示 ID。
    pub supplier_name: Option<String>,
    /// 权威商城订单号；商城订单缺失时为空。
    pub mall_order_no: Option<String>,
    /// 地址的服务端安全投影。
    pub address: SupplierOrderAddressView,
    /// 当前操作人可见的 W26 正式任务。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_item: Option<WorkItemView>,
    /// 当前任务/对象入口对应的权威原供应商动作。
    pub target_supplier_action_id: Option<String>,
    /// 该原动作的最新结构化调查证据。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_investigation: Option<SupplierOrderInvestigationEvidenceView>,
    /// W26 领域动作，不得从通用任务动作推导。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_actions: Vec<SupplierOrderAllowedAction>,
    /// W26 领域动作及展示事实阻断。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub action_blockers: Vec<SupplierOrderActionBlockerView>,
}

/// W26 详情查询参数。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SupplierFulfillmentOrderDetailParams {
    /// 从正式待办进入时必须携带的任务 ID。
    pub work_item_id: Option<String>,
}

/// W26 详情的地址安全投影。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierOrderAddressView {
    /// 权限安全的脱敏地址；当前无权威脱敏器时为空。
    pub masked: Option<String>,
    /// 当前详情是否已注册可审计的短时揭示入口。
    pub can_reveal: bool,
    /// 不可揭示或无法投影时的稳定阻断码。
    pub blocker_code: Option<String>,
    /// 面向当前处理人的安全说明。
    pub blocker_message: Option<String>,
}

/// W26 强类型领域动作。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierOrderAllowedAction {
    /// 查询权威原动作的供应商结果。
    QueryResult,
    /// 只在最新证据明确无结果时按原幂等语义重放。
    Replay,
    /// 以已验证终态证据完成正式任务。
    ConfirmVerifiedTerminalResult,
}

impl SupplierOrderAllowedAction {
    /// 返回稳定动作代码。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::QueryResult => "QUERY_RESULT",
            Self::Replay => "REPLAY",
            Self::ConfirmVerifiedTerminalResult => "CONFIRM_VERIFIED_TERMINAL_RESULT",
        }
    }
}

/// W26 对象入口的供应商结果调查命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SupplierOrderObjectInvestigationCommand {
    /// 供应商履约订单。
    pub order_id: SupplierFulfillmentOrderId,
    /// 查询所得订单乐观锁版本。
    #[validate(range(min = 1, message = "订单版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 固定调查动作。
    pub action: SupplierOrderInvestigationAction,
    /// 客户端生成的本次操作身份。
    #[validate(custom(function = "non_blank", message = "操作ID不能为空"))]
    #[validate(length(max = 64, message = "操作ID不能超过 64 个字符"))]
    pub operation_id: String,
    /// 被调查或按原幂等键重放的供应商原动作。
    pub target_supplier_action_id: SupplierOrderActionId,
    /// 客户端稳定请求幂等键；不得作为供应商重放幂等键。
    #[validate(custom(function = "non_blank", message = "请求标识不能为空"))]
    #[validate(length(max = 128, message = "请求标识不能超过 128 个字符"))]
    pub idempotency_key: String,
}

/// W26 正式任务入口的供应商结果调查命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SupplierOrderTaskInvestigationCommand {
    /// 当前正式任务。
    pub work_item_id: WorkItemId,
    /// 查询所得任务版本；服务端按正整数字符串严格解析。
    #[validate(custom(function = "non_blank", message = "任务版本不能为空"))]
    pub expected_task_version: String,
    /// 查询所得任务主体版本。
    #[validate(custom(function = "non_blank", message = "任务主体版本不能为空"))]
    #[validate(length(max = 128, message = "任务主体版本不能超过 128 个字符"))]
    pub expected_subject_version: String,
    /// 带订单身份与版本的固定调查动作。
    #[validate(nested)]
    pub action: SupplierOrderTaskInvestigationAction,
    /// 客户端稳定请求幂等键；不得作为供应商重放幂等键。
    #[validate(custom(function = "non_blank", message = "请求标识不能为空"))]
    #[validate(length(max = 128, message = "请求标识不能超过 128 个字符"))]
    pub idempotency_key: String,
}

/// 任务调查命令内的固定动作载荷。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SupplierOrderTaskInvestigationAction {
    /// 固定调查动作。
    #[serde(rename = "type")]
    pub action_type: SupplierOrderInvestigationAction,
    /// 供应商履约订单。
    pub order_id: SupplierFulfillmentOrderId,
    /// 查询所得订单乐观锁版本。
    #[validate(range(min = 1, message = "订单版本必须大于 0"))]
    pub expected_order_lock_version: u64,
    /// 被调查或按原幂等键重放的供应商原动作。
    pub target_supplier_action_id: SupplierOrderActionId,
    /// 客户端生成的本次操作身份。
    #[validate(custom(function = "non_blank", message = "操作ID不能为空"))]
    #[validate(length(max = 64, message = "操作ID不能超过 64 个字符"))]
    pub operation_id: String,
}

/// W26 允许的供应商结果调查动作。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierOrderInvestigationAction {
    /// 查询原供应商动作结果。
    QueryResult,
    /// 已证明原请求无结果后，沿原供应商幂等键安全重放。
    Replay,
}

/// W26 唯一强类型任务完成命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SupplierOrderTaskCompletionCommand {
    /// 当前正式任务。
    pub work_item_id: WorkItemId,
    /// 查询所得任务版本；服务端按正整数字符串严格解析。
    #[validate(custom(function = "non_blank", message = "任务版本不能为空"))]
    pub expected_task_version: String,
    /// 查询所得任务主体版本。
    #[validate(custom(function = "non_blank", message = "任务主体版本不能为空"))]
    #[validate(length(max = 128, message = "任务主体版本不能超过 128 个字符"))]
    pub expected_subject_version: String,
    /// 固定终态确认决定。
    #[validate(nested)]
    pub decision: SupplierOrderTaskCompletionDecision,
    /// 客户端稳定请求幂等键。
    #[validate(custom(function = "non_blank", message = "请求标识不能为空"))]
    #[validate(length(max = 128, message = "请求标识不能超过 128 个字符"))]
    pub idempotency_key: String,
}

/// W26 任务完成命令内的终态确认决定。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SupplierOrderTaskCompletionDecision {
    /// 固定决定类型；其它值在反序列化边界即拒绝。
    #[serde(rename = "type")]
    pub decision_type: SupplierOrderTaskCompletionDecisionType,
    /// 供应商履约订单。
    pub order_id: SupplierFulfillmentOrderId,
    /// 查询所得订单乐观锁版本。
    #[validate(range(min = 1, message = "订单版本必须大于 0"))]
    pub expected_order_lock_version: u64,
    /// 查询或重放形成的服务端可验证终态证据。
    pub verified_supplier_action_result_id: SupplierOrderActionId,
    /// 待固定的业务终态。
    pub resolution: SupplierOrderResolution,
}

/// W26 唯一允许的任务完成决定类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierOrderTaskCompletionDecisionType {
    /// 以服务端证据确认终态并完成原任务。
    ConfirmVerifiedTerminalResult,
}

/// W26 可由已验证供应商证据确认的终态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierOrderResolution {
    /// 供应商已接单。
    OrderAccepted,
    /// 供应商明确拒单。
    OrderRejected,
    /// 供应商履约完成。
    OrderCompleted,
    /// 供应商取消完成。
    Canceled,
    /// 供应商退款完成。
    Refunded,
}

impl SupplierOrderResolution {
    /// 返回稳定终态代码。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::OrderAccepted => "ORDER_ACCEPTED",
            Self::OrderRejected => "ORDER_REJECTED",
            Self::OrderCompleted => "ORDER_COMPLETED",
            Self::Canceled => "CANCELED",
            Self::Refunded => "REFUNDED",
        }
    }

    /// 返回面向处理人的业务结果名称。
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::OrderAccepted => "供应商已接单",
            Self::OrderRejected => "供应商已拒单",
            Self::OrderCompleted => "供应商履约已完成",
            Self::Canceled => "供应商取消已完成",
            Self::Refunded => "供应商退款已完成",
        }
    }
}

/// 调查证据的服务端结论。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierOrderInvestigationOutcome {
    /// 已由持久化业务事实证明终态。
    VerifiedTerminal,
    /// 供应商查询明确证明原请求没有形成结果。
    VerifiedNoResult,
    /// 查询或重放后仍无法证明原结果。
    ResultUnknown,
}

/// 调查命令结果状态。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierOrderInvestigationResultStatus {
    /// 已取得终态或明确无结果证据。
    Succeeded,
    /// 结果仍未知。
    Unknown,
    /// 当前服务端事实禁止继续。
    Blocked,
}

/// W26 调查证据视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierOrderInvestigationEvidenceView {
    /// 服务端不可变证据动作 ID。
    pub evidence_id: String,
    /// 被调查的原供应商动作。
    pub target_supplier_action_id: String,
    /// 证据结论。
    pub outcome: SupplierOrderInvestigationOutcome,
    /// 证据记录时间（秒级时间戳）。
    pub recorded_at: i64,
    /// 服务端是否已证明可沿原供应商幂等键安全重放。
    pub can_safe_retry: bool,
    /// 已验证终态对应的供应商外部订单号。
    pub external_order_no: Option<String>,
    /// 权限安全的业务说明。
    pub summary: String,
    /// 已验证终态证据动作；仅 `VERIFIED_TERMINAL` 返回。
    pub verified_supplier_action_result_id: Option<String>,
    /// 已验证业务终态；仅 `VERIFIED_TERMINAL` 返回。
    pub verified_resolution: Option<SupplierOrderResolution>,
}

/// 调查动作关联的原开放任务投影。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierOrderInvestigationWorkItemView {
    /// 原任务 ID。
    pub id: String,
    /// 调查后仍固定为开放。
    pub status: entities::work_item::WorkItemStatus,
    /// 调查处理记录提交后的任务版本。
    pub task_version: u64,
}

/// 当前动作阻断说明。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierOrderActionBlockerView {
    /// 被阻断动作。
    pub action: String,
    /// 稳定阻断码。
    pub code: String,
    /// 权限安全的业务说明。
    pub message: String,
    /// 可选目标工作面。
    pub destination_workspace_id: Option<String>,
}

/// W26 对象/任务调查统一响应。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierOrderInvestigationResultView {
    /// 调查结果状态。
    pub result_status: SupplierOrderInvestigationResultStatus,
    /// 权限安全的业务说明。
    pub message: String,
    /// 客户端提交的操作身份。
    pub operation_id: String,
    /// 本次新增的不可变证据。
    pub evidence: SupplierOrderInvestigationEvidenceView,
    /// 返回时的订单事实。
    pub order: SupplierFulfillmentOrderView,
    /// 任务入口返回原开放任务；对象入口为空。
    pub work_item: Option<SupplierOrderInvestigationWorkItemView>,
    /// 本次证据后允许的固定下一动作。
    pub allowed_actions: Vec<String>,
    /// 本次证据后的动作阻断说明。
    pub action_blockers: Vec<SupplierOrderActionBlockerView>,
}

/// W26 强类型任务完成结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierOrderTaskCompletionResultView {
    /// 服务端稳定操作结果身份。
    pub operation_id: String,
    /// 已完成的正式任务。
    pub work_item_id: String,
    /// 固定为 `COMPLETED`。
    pub work_item_status: entities::work_item::WorkItemStatus,
    /// 完成后的任务版本。
    pub task_version: u64,
    /// 终态确认时的订单版本。
    pub order_lock_version: u64,
    /// 已固定的业务终态。
    pub resolution: SupplierOrderResolution,
}

/// 供应商取消/退款动作提交请求（动作行冻结实际提交给供应商的范围）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SubmitAfterSalesActionRequest {
    /// 商城售后申请（取消/退款动作必填，§6.19）。
    pub after_sales_request_id: MallAfterSalesRequestId,
    /// 提交给供应商的动作行。
    #[validate(length(min = 1, message = "动作行至少一行"))]
    pub lines: Vec<AfterSalesActionLineRequest>,
    /// 原因代码（可选）。
    pub reason_code: Option<String>,
    /// 备注（可选）。
    pub comment: Option<String>,
}

/// 供应商取消/退款动作行请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AfterSalesActionLineRequest {
    /// 原商城售后申请行。
    pub after_sales_request_line_id: MallAfterSalesRequestLineId,
    /// 本供应商履约明细。
    pub supplier_fulfillment_item_id: SupplierFulfillmentItemId,
    /// 本动作提交数量。
    pub quantity: Quantity,
    /// 本动作提交金额。
    pub amount: Amount,
}

/// 供应商动作提交结果视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SubmitActionResultView {
    /// 动作响应视图。
    pub action: SupplierOrderActionView,
    /// 动作行。
    pub lines: Vec<SupplierOrderActionLineView>,
    /// 动作后订单视图。
    pub order: SupplierFulfillmentOrderView,
}

/// 供应商拒单结果登记请求（回调幂等键 `(connection_id, external_event_id)`，§6.19）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RecordSupplierRejectRequest {
    /// 外部事件 ID（与连接组成回调幂等键）。
    #[validate(custom(function = "non_blank", message = "外部事件ID不能为空"))]
    pub external_event_id: String,
    /// 供应商状态版本。
    #[validate(custom(function = "non_blank", message = "供应商状态版本不能为空"))]
    pub supplier_status_version: String,
    /// 业务发生时间（秒级时间戳）。
    #[validate(range(min = 1, message = "发生时间必须大于 0"))]
    pub occurred_at: i64,
}

/// 供应商退款成功结果登记请求（幂等键 `(connection_id, external_refund_no,
/// external_refund_version)`，§6.19；分配行冻结冲减范围）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RecordRefundResultRequest {
    /// 外部退款号。
    #[validate(custom(function = "non_blank", message = "外部退款号不能为空"))]
    pub external_refund_no: String,
    /// 外部退款版本。
    #[validate(custom(function = "non_blank", message = "外部退款版本不能为空"))]
    pub external_refund_version: String,
    /// 实际退款金额。
    pub refund_amount: Amount,
    /// 实际退款时间（秒级时间戳）。
    #[validate(range(min = 1, message = "退款时间必须大于 0"))]
    pub refunded_at: i64,
    /// 来源事件 ID。
    #[validate(custom(function = "non_blank", message = "来源事件ID不能为空"))]
    pub source_event_id: String,
    /// 退款分配行（`APPLY`）。
    #[validate(length(min = 1, message = "退款分配至少一行"))]
    pub allocations: Vec<RefundAllocationRequest>,
}

/// 供应商退款分配行请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RefundAllocationRequest {
    /// 原供应商履约明细。
    pub supplier_fulfillment_item_id: SupplierFulfillmentItemId,
    /// 被冲减的原成本。
    pub original_cost_entry_id: CostEntryId,
    /// 被冲减的原成本归属。
    pub original_cost_allocation_id: CostAllocationId,
    /// 被冲减的原应付分录。
    pub original_payable_entry_id: PayableEntryId,
    /// 原应付已付款部分的付款分配，可空。
    pub original_payment_allocation_id: Option<PaymentAllocationId>,
    /// 实际供应商退款数量。
    pub refund_quantity: Quantity,
    /// 含税成本冲减金额。
    pub gross_amount: Amount,
    /// 不含税成本冲减金额。
    pub net_amount: Amount,
    /// 税额冲减金额。
    pub tax_amount: Amount,
    /// 未付应付冲减金额。
    pub payable_reduction_amount: Amount,
    /// 已付现金退回拆分金额。
    pub cash_refund_amount: Amount,
}

#[cfg(test)]
mod tests {
    use super::{normalize_sort, SortDir, SupplierFulfillmentOrderListParams};
    use entities::supplier_fulfillment::{CancelStatus, FulfillmentStatus, RefundStatus};
    use serde_json::json;
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" submitted_at ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "submitted_at"],
        )
        .unwrap();
        assert_eq!(field, "submitted_at");
        assert_eq!(direction, SortDir::Asc);

        let (field, direction) = normalize_sort(&None, &None, &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Desc);
    }

    #[test]
    fn list_params_normalize_paging_filters_and_sort_defaults() {
        let params = SupplierFulfillmentOrderListParams {
            supplier_id: None,
            fulfillment_status: Some(FulfillmentStatus::Accepted),
            cancel_status: Some(CancelStatus::None),
            refund_status: Some(RefundStatus::RefundPending),
            external_order_no: Some(" SUP-1 ".to_string()),
            mall_order_id: None,
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.fulfillment_status, Some(FulfillmentStatus::Accepted));
        assert_eq!(query.external_order_no.as_deref(), Some("SUP-1"));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params = SupplierFulfillmentOrderListParams {
            supplier_id: None,
            fulfillment_status: None,
            cancel_status: None,
            refund_status: None,
            external_order_no: None,
            mall_order_id: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn place_request_rejects_blank_order_no_and_empty_items() {
        let base = json!({
            "fulfillment_order_no": "FO-2026-001",
            "mall_order_id": "mall-order-1",
            "supplier_id": "supplier-1",
            "connection_id": "connection-1",
            "split_no": 1,
            "address_snapshot_encrypted": "encrypted",
            "address_snapshot_fingerprint": "fingerprint",
            "items": [{
                "mall_order_item_id": "mall-item-1",
                "supplier_offering_revision_id": "offering-rev-1",
                "quantity": "3.000000",
                "unit_cost_snapshot_gross": "9.9900",
                "input_tax_rate": "0.130000"
            }]
        });
        let request: super::PlaceFulfillmentOrderRequest = serde_json::from_value(base).unwrap();
        assert!(request.validate().is_ok());

        let blank_no = json!({
            "fulfillment_order_no": "  ",
            "mall_order_id": "mall-order-1",
            "supplier_id": "supplier-1",
            "connection_id": "connection-1",
            "split_no": 1,
            "address_snapshot_encrypted": "encrypted",
            "address_snapshot_fingerprint": "fingerprint",
            "items": []
        });
        let request: super::PlaceFulfillmentOrderRequest = serde_json::from_value(blank_no).unwrap();
        assert!(request.validate().is_err());
    }

    #[test]
    fn w26_task_investigation_accepts_only_the_registered_shape() {
        let command: super::SupplierOrderTaskInvestigationCommand = serde_json::from_value(json!({
            "work_item_id": "work-item-1",
            "expected_task_version": "4",
            "expected_subject_version": "12",
            "action": {
                "type": "QUERY_RESULT",
                "order_id": "supplier-order-1",
                "expected_order_lock_version": 12,
                "target_supplier_action_id": "supplier-action-1",
                "operation_id": "operation-1"
            },
            "idempotency_key": "request-1"
        }))
        .unwrap();
        assert!(command.validate().is_ok());

        let unknown_field = serde_json::from_value::<super::SupplierOrderTaskInvestigationCommand>(json!({
            "work_item_id": "work-item-1",
            "expected_task_version": "4",
            "expected_subject_version": "12",
            "action": {
                "type": "QUERY_RESULT",
                "order_id": "supplier-order-1",
                "expected_order_lock_version": 12,
                "target_supplier_action_id": "supplier-action-1",
                "operation_id": "operation-1"
            },
            "idempotency_key": "request-1",
            "unknown_field": "COMPLETE"
        }));
        assert!(unknown_field.is_err());
    }

    #[test]
    fn w26_task_completion_rejects_unregistered_decisions() {
        let result = serde_json::from_value::<super::SupplierOrderTaskCompletionCommand>(json!({
            "work_item_id": "work-item-1",
            "expected_task_version": "4",
            "expected_subject_version": "12",
            "decision": {
                "type": "MARK_SUCCESS_LOCALLY",
                "order_id": "supplier-order-1",
                "expected_order_lock_version": 12,
                "verified_supplier_action_result_id": "evidence-1",
                "resolution": "ORDER_ACCEPTED"
            },
            "idempotency_key": "request-1"
        }));
        assert!(result.is_err());
    }
}
