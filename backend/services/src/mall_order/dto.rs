//! 域 D29 `mall_order` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；金额一律十进制字符串。
//! 视图字段名与 `erp-client/features/mall-consumption-orders/types.ts` 对齐
//! （snake_case 化，差异见 PR「契约变更」）。

use entities::ids::{MallAfterSalesRequestId, MallOrderFactId};
use entities::mall_order::{AttributionStatus, DataSource, FactType, FulfillmentChain, ProcessingStatus};
use entities::money::Amount;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use validator::Validate;

use crate::errors::Result;
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 商城订单列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const MALL_ORDER_SORT_FIELDS: &[&str] = &["paid_at", "ordered_at", "created_at"];
/// 关键事实列表允许的排序字段白名单。
pub(crate) const MALL_ORDER_FACT_SORT_FIELDS: &[&str] = &["occurred_at", "received_at", "created_at"];

/// 排序方向。
pub use crate::query::SortDir;

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
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
pub(crate) use crate::query::normalize_sort;

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use crate::query::PageView;

/// 校验文本去除首尾空白后非空。
use crate::query::non_blank;

/// 校验金额字符串为合法非负定点数值（小数位 ≤ 2）。
fn valid_amount(value: &str) -> std::result::Result<(), validator::ValidationError> {
    let amount = Amount::from_str(value).map_err(|_| validator::ValidationError::new("不是合法定点数值"))?;
    if amount.to_decimal().is_sign_negative() {
        return Err(validator::ValidationError::new("金额不能为负"));
    }
    Ok(())
}

/// 商城订单列表查询参数（W25 §8.1 扁平筛选）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MallOrderListParams {
    /// 商城订单号/商城/客户引用模糊搜索。
    pub q: Option<String>,
    /// 来源商城精确筛选。
    pub mall_id: Option<String>,
    /// 商城订单号精确筛选。
    pub external_order_no: Option<String>,
    /// 映射后的企业客户筛选。
    pub customer_id: Option<String>,
    /// 履约链筛选。
    pub fulfillment_chain: Option<FulfillmentChain>,
    /// 归集进度状态筛选。
    pub attribution_status: Option<AttributionStatus>,
    /// 支付时间下界（含，秒级时间戳）。
    pub paid_at_from: Option<u64>,
    /// 支付时间上界（含，秒级时间戳）。
    pub paid_at_to: Option<u64>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`paid_at`/`ordered_at`/`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的商城订单列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MallOrderListQuery {
    /// 模糊搜索词。
    pub q: Option<String>,
    /// 来源商城精确筛选。
    pub mall_id: Option<String>,
    /// 商城订单号精确筛选。
    pub external_order_no: Option<String>,
    /// 映射后的企业客户筛选。
    pub customer_id: Option<String>,
    /// 履约链筛选。
    pub fulfillment_chain: Option<FulfillmentChain>,
    /// 归集进度状态筛选。
    pub attribution_status: Option<AttributionStatus>,
    /// 支付时间下界（含）。
    pub paid_at_from: Option<u64>,
    /// 支付时间上界（含）。
    pub paid_at_to: Option<u64>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl MallOrderListParams {
    /// 归一化商城订单列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<MallOrderListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, MALL_ORDER_SORT_FIELDS)?;
        Ok(MallOrderListQuery {
            q: normalized_text(self.q.as_deref()),
            mall_id: normalized_text(self.mall_id.as_deref()),
            external_order_no: normalized_text(self.external_order_no.as_deref()),
            customer_id: normalized_text(self.customer_id.as_deref()),
            fulfillment_chain: self.fulfillment_chain,
            attribution_status: self.attribution_status,
            paid_at_from: self.paid_at_from,
            paid_at_to: self.paid_at_to,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 事实摘要（W25 列表行：按事实类型聚合）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FactSummaryItemView {
    /// 事实类型。
    pub fact_type: FactType,
    /// 该类型最近发生时间（秒级时间戳）。
    pub latest_occurred_at: u64,
    /// 该类型事实条数。
    pub count: u64,
}

/// 支付构成（W25 列表行）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PaymentCompositionView {
    /// 卡券支付合计（字符串）。
    pub card_amount: String,
    /// 微信支付合计（字符串）。
    pub wechat_amount: String,
    /// 支付来源条数。
    pub source_count: u32,
}

/// 成本口径分项摘要（W25 列表行）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CostBasisBreakdownItemView {
    /// 成本口径。
    pub basis: entities::mall_order::CostBasis,
    /// 消费行数。
    pub line_count: u64,
    /// 成本金额合计（字符串）；`NONE` 时省略。
    pub cost_amount: Option<String>,
}

/// 供应商履约摘要（W25 列表行；D32 未闭环时恒为 0 摘要）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierOrderSummaryView {
    /// 供应商子订单总数。
    pub total: u32,
    /// 履约状态集合。
    pub statuses: Vec<String>,
    /// 是否存在异常。
    pub has_exception: bool,
}

/// 商城订单列表行视图（W25 §8.1）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MallOrderListRow {
    /// 商城订单 ID。
    pub mall_order_id: String,
    /// 来源商城。
    pub mall_id: String,
    /// 商城名称（P3 与 `mall_id` 同值，见契约变更）。
    pub mall_name: String,
    /// 商城订单号。
    pub external_order_no: String,
    /// 映射后的企业客户。
    pub customer_id: Option<String>,
    /// 客户展示名（未归集时为空，见契约变更）。
    pub customer_label: Option<String>,
    /// 支付成功时间（秒级时间戳）。
    pub paid_at: u64,
    /// 实付金额（字符串）。
    pub paid_amount: String,
    /// 支付构成。
    pub payment_composition: PaymentCompositionView,
    /// 事实摘要。
    pub fact_summary: Vec<FactSummaryItemView>,
    /// 履约链归属。
    pub fulfillment_chain: FulfillmentChain,
    /// 供应商履约摘要。
    pub supplier_order_summary: SupplierOrderSummaryView,
    /// 归集进度状态。
    pub attribution_status: AttributionStatus,
    /// 成本口径分项。
    pub cost_basis_breakdown: Vec<CostBasisBreakdownItemView>,
    /// 数据来源（按事实派生）。
    pub data_source: DataSource,
    /// 允许动作（W25 只读，恒为空）。
    pub allowed_actions: Vec<String>,
    /// 动作阻断说明（恒为空）。
    pub action_blockers: Vec<String>,
    /// 成本口径策略状态。
    pub cost_basis_policy_state: String,
    /// 归一化成本口径（`ACTUAL`/`STANDARD`/`NONE`/`MIXED`）。
    pub normalized_cost_basis: Option<String>,
}

/// 关键事实视图（W25 §8.2）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MallOrderFactView {
    /// 事实 ID。
    pub fact_id: String,
    /// 事实类型。
    pub fact_type: FactType,
    /// 业务事实键。
    pub business_fact_key: String,
    /// 商城结果版本。
    pub external_order_version: String,
    /// 商城售后请求 ID（适用时）。
    pub after_sales_request_id: Option<String>,
    /// 原支付事实 ID（适用时）。
    pub original_payment_fact_id: Option<String>,
    /// 事实发生时间（秒级时间戳）。
    pub occurred_at: u64,
    /// ERP 接收时间（秒级时间戳）。
    pub received_at: u64,
    /// 数据来源。
    pub data_source: DataSource,
    /// 处理状态。
    pub processing_status: ProcessingStatus,
}

/// 商品明细视图（W25 §8.2）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MallOrderItemView {
    /// 商品明细 ID。
    pub mall_order_item_id: String,
    /// 来源明细 ID。
    pub external_item_id: String,
    /// ERP SKU。
    pub sku_id: Option<String>,
    /// 下单时发布版本。
    pub product_publication_revision_id: Option<String>,
    /// 下单时固定供给。
    pub supplier_offering_revision_id: Option<String>,
    /// 商品名称快照。
    pub name_snapshot: String,
    /// 规格快照。
    pub spec_snapshot: Option<String>,
    /// 数量（字符串）。
    pub quantity: String,
    /// 含税售价（字符串）。
    pub unit_price_gross: String,
    /// 明细原价（字符串）。
    pub line_gross_amount: String,
    /// 分摊优惠（字符串）。
    pub allocated_discount_amount: String,
    /// 分摊运费（字符串）。
    pub allocated_freight_amount: String,
    /// 明细实付（字符串）。
    pub paid_amount: String,
    /// 销项税率（字符串）。
    pub sales_tax_rate: String,
    /// 商城记录的单位供货成本（字符串）。
    pub unit_cost_snapshot: Option<String>,
    /// 商城记录的明细供货成本合计（字符串）。
    pub cost_snapshot_total: Option<String>,
    /// 成本含税标识。
    pub cost_tax_inclusion: Option<bool>,
    /// 成本进项税率（字符串）。
    pub cost_input_tax_rate: Option<String>,
    /// 归集进度状态。
    pub attribution_status: AttributionStatus,
}

/// 支付来源视图（W25 §8.2）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PaymentSourceView {
    /// 支付来源 ID。
    pub payment_source_id: String,
    /// 单内来源序号。
    pub source_no: u32,
    /// 来源类型。
    pub source_type: entities::mall_order::PaymentSourceType,
    /// 支付金额（字符串）。
    pub amount: String,
    /// 来源引用（卡券稳定引用或微信引用，脱敏形态）。
    pub source_reference: String,
    /// 映射后的卡实例。
    pub mall_card_instance_id: Option<String>,
    /// 归集进度状态。
    pub attribution_status: AttributionStatus,
    /// 来源归属（卡券归集后沿卡实例追溯原销售单）。
    pub origin: Option<PaymentSourceOriginView>,
}

/// 支付来源归属视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PaymentSourceOriginView {
    /// 企业客户。
    pub customer_id: Option<String>,
    /// 原销售单。
    pub sales_order_id: String,
}

/// 商品 × 支付来源分摊视图（W25 §8.2）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FundingAllocationView {
    /// 商品明细 ID。
    pub mall_order_item_id: String,
    /// 支付来源 ID。
    pub payment_source_id: String,
    /// 分摊实付（字符串）。
    pub allocated_payment_amount: String,
}

/// 成本评估视图（W25 §8.2 消费明细的当前成本）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CostAssessmentView {
    /// 评估 ID。
    pub assessment_id: String,
    /// 同消费递增评估号。
    pub assessment_no: u32,
    /// 成本口径。
    pub cost_basis: entities::mall_order::CostBasis,
    /// 依据来源展示名。
    pub basis_source_label: String,
    /// 含税成本金额（字符串）；`NONE` 时省略。
    pub gross_amount: Option<String>,
    /// 不含税成本金额（字符串）；`NONE` 时省略。
    pub net_amount: Option<String>,
    /// 税额（字符串）；`NONE` 时省略。
    pub tax_amount: Option<String>,
    /// 含税标识。
    pub tax_inclusion: Option<bool>,
    /// 进项税率（字符串）。
    pub input_tax_rate: Option<String>,
    /// 评估时间（秒级时间戳）。
    pub assessed_at: u64,
}

/// 消费事实视图（W25 §8.2）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConsumptionEntryView {
    /// 消费事实 ID。
    pub consumption_entry_id: String,
    /// 所引事实 ID。
    pub fact_id: String,
    /// 商品明细 ID。
    pub item_id: String,
    /// 支付来源 ID。
    pub payment_source_id: String,
    /// 消费或消费冲减。
    pub direction: entities::mall_order::ConsumptionDirection,
    /// 金额（字符串）。
    pub amount: String,
    /// 业务发生时间（秒级时间戳）。
    pub occurred_at: u64,
    /// 归集进度状态。
    pub attribution_status: AttributionStatus,
    /// 卡券经营归属原销售单。
    pub origin_sales_order_id: Option<String>,
    /// 冲减的原消费。
    pub reverses_consumption_entry_id: Option<String>,
    /// 当前成本评估。
    pub current_cost_assessment: Option<CostAssessmentView>,
}

/// 守恒校验行结果（W25 §8.2）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConservationResultRow {
    /// 校验对象 ID。
    pub id: String,
    /// 期望值（字符串）。
    pub expected: String,
    /// 实际值（字符串）。
    pub actual: String,
    /// 是否有效。
    pub valid: bool,
}

/// 守恒校验结果（W25 §8.2：分摊矩阵行列守恒）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConservationView {
    /// 商品明细行校验。
    pub item_row_results: Vec<ConservationResultRow>,
    /// 支付来源列校验。
    pub source_column_results: Vec<ConservationResultRow>,
    /// 订单总额校验。
    pub order_total: ConservationResultRow,
}

/// 供应商履约视图（D32 未闭环时为空数组，见契约变更）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierOrderView {
    /// 供应商履约订单 ID。
    pub supplier_fulfillment_order_id: String,
    /// 履约单号。
    pub fulfillment_order_no: String,
    /// 供应商展示名。
    pub supplier_label: String,
    /// 商品明细 ID 集合。
    pub item_ids: Vec<String>,
    /// 履约状态。
    pub fulfillment_status: String,
}

/// 商城订单详情视图（W25 §8.2）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MallOrderDetailView {
    /// 订单身份。
    pub identity: MallOrderIdentityView,
    /// 客户与归属。
    pub customer: MallOrderCustomerView,
    /// 下单时间（秒级时间戳）。
    pub ordered_at: u64,
    /// 支付时间（秒级时间戳）。
    pub paid_at: u64,
    /// 金额快照与守恒状态。
    pub amounts: MallOrderAmountsView,
    /// 履约链归属。
    pub fulfillment: MallOrderFulfillmentView,
    /// 关键事实。
    pub facts: Vec<MallOrderFactView>,
    /// 商品明细。
    pub items: Vec<MallOrderItemView>,
    /// 支付来源。
    pub payment_sources: Vec<PaymentSourceView>,
    /// 分摊矩阵。
    pub funding_allocations: Vec<FundingAllocationView>,
    /// 守恒校验。
    pub conservation: ConservationView,
    /// 消费事实。
    pub consumption_entries: Vec<ConsumptionEntryView>,
    /// 供应商履约（D32 未闭环，恒为空）。
    pub supplier_orders: Vec<SupplierOrderView>,
    /// 地址快照摘要（敏感，仅提示存储形态）。
    pub address: MallOrderAddressView,
    /// 允许动作（只读，恒为空）。
    pub allowed_actions: Vec<String>,
    /// 动作阻断说明（恒为空）。
    pub action_blockers: Vec<String>,
}

/// 订单身份视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MallOrderIdentityView {
    /// 商城订单 ID。
    pub mall_order_id: String,
    /// 来源商城。
    pub mall_id: String,
    /// 商城名称。
    pub mall_name: String,
    /// 商城订单号。
    pub external_order_no: String,
    /// 原支付成功事实 ID。
    pub payment_fact_id: String,
}

/// 客户与归属视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MallOrderCustomerView {
    /// 来源客户标识。
    pub source_customer_ref: Option<String>,
    /// 映射后的企业客户。
    pub customer_id: Option<String>,
    /// 客户展示名。
    pub customer_label: Option<String>,
    /// 归集进度状态。
    pub attribution_status: AttributionStatus,
}

/// 金额快照视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MallOrderAmountsView {
    /// 原价（字符串）。
    pub gross: String,
    /// 优惠（字符串）。
    pub discount: String,
    /// 运费（字符串）。
    pub freight: String,
    /// 实付（字符串）。
    pub paid: String,
    /// 守恒状态。
    pub conservation_status: String,
}

/// 履约链视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MallOrderFulfillmentView {
    /// 履约链归属。
    pub chain: FulfillmentChain,
    /// 依据的切换记录。
    pub cutover_id: Option<String>,
    /// 切换启用时间 `T`。
    pub cutover_at: Option<u64>,
    /// 判定依据的支付发生时间。
    pub decided_by_occurred_at: u64,
}

/// 地址快照摘要视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MallOrderAddressView {
    /// 脱敏摘要。
    pub masked_summary: String,
    /// 是否允许揭示（受控动作，P3 恒为 `false`）。
    pub reveal_allowed: bool,
}

/// 关键事实列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MallOrderFactListParams {
    /// 来源商城模糊筛选。
    pub mall_id: Option<String>,
    /// 事实类型筛选。
    pub fact_type: Option<FactType>,
    /// 处理状态筛选。
    pub processing_status: Option<ProcessingStatus>,
    /// 商城售后请求 ID 筛选。
    pub after_sales_request_id: Option<MallAfterSalesRequestId>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`occurred_at`/`received_at`/`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的关键事实列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MallOrderFactListQuery {
    /// 来源商城模糊筛选。
    pub mall_id: Option<String>,
    /// 事实类型筛选。
    pub fact_type: Option<FactType>,
    /// 处理状态筛选。
    pub processing_status: Option<ProcessingStatus>,
    /// 商城售后请求 ID 筛选。
    pub after_sales_request_id: Option<MallAfterSalesRequestId>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl MallOrderFactListParams {
    /// 归一化关键事实列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<MallOrderFactListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, MALL_ORDER_FACT_SORT_FIELDS)?;
        Ok(MallOrderFactListQuery {
            mall_id: normalized_text(self.mall_id.as_deref()),
            fact_type: self.fact_type,
            processing_status: self.processing_status,
            after_sales_request_id: self.after_sales_request_id.clone(),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 支付明细行（付款载荷）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PaymentItemData {
    /// 来源明细 ID。
    #[validate(custom(function = "non_blank", message = "来源明细ID不能为空"))]
    pub external_item_id: String,
    /// ERP SKU（可空，暂未映射时标记待归集）。
    pub sku_id: Option<String>,
    /// 下单时发布版本。
    pub product_publication_revision_id: Option<String>,
    /// 下单时固定供给。
    pub supplier_offering_revision_id: Option<String>,
    /// 商品名称快照。
    #[validate(custom(function = "non_blank", message = "商品名称不能为空"))]
    pub name_snapshot: String,
    /// 规格快照。
    pub spec_snapshot: Option<String>,
    /// 数量（字符串，6 位小数）。
    pub quantity: String,
    /// 含税售价（字符串，4 位小数）。
    pub unit_price_gross: String,
    /// 分摊优惠（字符串）。
    #[validate(custom(function = "valid_amount", message = "优惠金额非法"))]
    pub allocated_discount_amount: String,
    /// 分摊运费（字符串）。
    #[validate(custom(function = "valid_amount", message = "运费金额非法"))]
    pub allocated_freight_amount: String,
    /// 销项税率（字符串，6 位小数）。
    pub sales_tax_rate: String,
    /// 商城记录的单位供货成本（可空）。
    pub unit_cost_snapshot: Option<String>,
    /// 商城记录的明细供货成本合计（可空）。
    pub cost_snapshot_total: Option<String>,
    /// 成本含税标识（有成本字段时必填）。
    pub cost_tax_inclusion: Option<bool>,
    /// 成本进项税率（含税成本时必填）。
    pub cost_input_tax_rate: Option<String>,
}

/// 支付来源载荷。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PaymentSourceData {
    /// 单内支付来源序号（从 1 起）。
    #[validate(range(min = 1, message = "支付来源序号必须从 1 开始"))]
    pub source_no: u32,
    /// 来源类型（仅 `CARD` 或 `WECHAT`）。
    pub source_type: entities::mall_order::PaymentSourceType,
    /// 实际支付金额（字符串）。
    #[validate(custom(function = "valid_amount", message = "支付金额非法"))]
    pub amount: String,
    /// 卡券来源稳定引用（`CARD` 必填）。
    pub source_card_instance_ref: Option<String>,
    /// 微信支付引用（`WECHAT` 必填）。
    pub wechat_payment_ref: Option<String>,
}

/// 分摊矩阵载荷。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct FundingAllocationData {
    /// 商品明细（按来源明细 ID 引用）。
    #[validate(custom(function = "non_blank", message = "分摊明细ID不能为空"))]
    pub external_item_id: String,
    /// 支付来源（按单内序号引用）。
    #[validate(range(min = 1, message = "分摊来源序号必须从 1 开始"))]
    pub source_no: u32,
    /// 分摊实付（字符串）。
    #[validate(custom(function = "valid_amount", message = "分摊金额非法"))]
    pub allocated_payment_amount: String,
}

/// 付款载荷（`PAYMENT_SUCCEEDED` 必填）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PaymentFactData {
    /// 商城用户稳定标识。
    #[validate(custom(function = "non_blank", message = "商城用户标识不能为空"))]
    pub mall_user_ref: String,
    /// 来源客户标识。
    pub source_customer_ref: Option<String>,
    /// 映射后的企业客户（可空）。
    pub customer_id: Option<String>,
    /// 下单时间（秒级时间戳）。
    #[validate(range(min = 1, message = "下单时间必须大于 0"))]
    pub ordered_at: u64,
    /// 原价（字符串）。
    #[validate(custom(function = "valid_amount", message = "原价非法"))]
    pub gross_amount: String,
    /// 优惠（字符串）。
    #[validate(custom(function = "valid_amount", message = "优惠金额非法"))]
    pub discount_amount: String,
    /// 运费（字符串）。
    #[validate(custom(function = "valid_amount", message = "运费金额非法"))]
    pub freight_amount: String,
    /// 实付（字符串）。
    #[validate(custom(function = "valid_amount", message = "实付金额非法"))]
    pub paid_amount: String,
    /// 供应商履约所需地址快照（加密形态，可空）。
    pub address_snapshot_encrypted: Option<String>,
    /// 商品明细。
    #[validate(length(min = 1, message = "商品明细不能为空"))]
    pub items: Vec<PaymentItemData>,
    /// 支付来源。
    #[validate(length(min = 1, message = "支付来源不能为空"))]
    pub payment_sources: Vec<PaymentSourceData>,
    /// 分摊矩阵。
    #[validate(length(min = 1, message = "分摊矩阵不能为空"))]
    pub funding_allocations: Vec<FundingAllocationData>,
}

/// 取消载荷（`ORDER_CANCELED` 必填）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CancelFactData {
    /// 来源取消版本。
    #[validate(custom(function = "non_blank", message = "取消版本不能为空"))]
    pub cancel_version: String,
    /// 整单或明细。
    pub cancel_scope: entities::mall_order::CancelScope,
    /// 实际取消数量（字符串）。
    pub actual_canceled_quantity: String,
    /// 实际取消金额（字符串）。
    #[validate(custom(function = "valid_amount", message = "取消金额非法"))]
    pub actual_canceled_amount: String,
    /// 取消原因。
    #[validate(custom(function = "non_blank", message = "取消原因不能为空"))]
    pub reason: String,
}

/// 完成载荷（`ORDER_COMPLETED` 必填）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CompletionFactData {
    /// 来源完成版本。
    #[validate(custom(function = "non_blank", message = "完成版本不能为空"))]
    pub completion_version: String,
    /// 商城实际完成时间（秒级时间戳）。
    #[validate(range(min = 1, message = "完成时间必须大于 0"))]
    pub completed_at: u64,
}

/// 商城关键事实接收请求（共同事件信封，§6.17）。
///
/// `business_fact_key` 与 `inbox_message_id` 是幂等键：重复提交只返回既有
/// 正式事实，不产生第二份事实、订单、消费或成本（§9.4）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReceiveMallOrderFactRequest {
    /// 消息来源商城。
    #[validate(custom(function = "non_blank", message = "来源商城不能为空"))]
    pub mall_id: String,
    /// 来源事件 ID（消息层幂等，与 `inbox_message_id` 同源）。
    #[validate(custom(function = "non_blank", message = "来源事件ID不能为空"))]
    pub source_event_id: String,
    /// 共同信封（唯一）。
    #[validate(custom(function = "non_blank", message = "信封ID不能为空"))]
    pub inbox_message_id: String,
    /// 跨实时和回填的稳定事实键（业务幂等键）。
    #[validate(custom(function = "non_blank", message = "业务事实键不能为空"))]
    pub business_fact_key: String,
    /// 事实类型（`PAYMENT_SUCCEEDED`/`ORDER_CANCELED`/`ORDER_COMPLETED`；
    /// 退款与余额恢复走 D30 售后域接口）。
    pub fact_type: FactType,
    /// 商城订单号。
    #[validate(custom(function = "non_blank", message = "商城订单号不能为空"))]
    pub external_order_no: String,
    /// 对应结果版本。
    #[validate(custom(function = "non_blank", message = "结果版本不能为空"))]
    pub external_order_version: String,
    /// 商城售后请求 ID（取消必填；支付/完成不得携带）。
    pub after_sales_request_id: Option<MallAfterSalesRequestId>,
    /// 原支付成功事实（取消/完成必填；支付不得携带）。
    pub original_payment_fact_id: Option<MallOrderFactId>,
    /// 事实发生时间（秒级时间戳）。
    #[validate(range(min = 1, message = "事实发生时间必须大于 0"))]
    pub occurred_at: u64,
    /// ERP 接收时间（秒级时间戳）。
    #[validate(range(min = 1, message = "接收时间必须大于 0"))]
    pub received_at: u64,
    /// 实时或历史回填。
    pub data_source: DataSource,
    /// 可选的加密原文引用。
    pub raw_payload_reference: Option<String>,
    /// 付款载荷（`PAYMENT_SUCCEEDED` 必填）。
    pub payment: Option<PaymentFactData>,
    /// 取消载荷（`ORDER_CANCELED` 必填）。
    pub cancel: Option<CancelFactData>,
    /// 完成载荷（`ORDER_COMPLETED` 必填）。
    pub completion: Option<CompletionFactData>,
}

/// 事实接收结果视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReceivedFactView {
    /// 事实视图。
    pub fact: MallOrderFactView,
    /// 新建（或既有）商城订单 ID（仅支付事实）。
    pub mall_order_id: Option<String>,
    /// 是否幂等命中既有事实。
    pub idempotent_hit: bool,
}

#[cfg(test)]
mod tests {
    use super::normalize_sort;
    use crate::mall_order::dto::{MallOrderFactListParams, MallOrderListParams, SortDir};
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["paid_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["paid_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" paid_at ".to_string()),
            &Some(" asc ".to_string()),
            &["paid_at", "ordered_at", "created_at"],
        )
        .unwrap();
        assert_eq!(field, "paid_at");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn list_params_normalize_paging_filters_and_sort_defaults() {
        let params = MallOrderListParams {
            q: Some(" SO-1 ".to_string()),
            mall_id: Some(" mall-a ".to_string()),
            external_order_no: None,
            customer_id: None,
            fulfillment_chain: None,
            attribution_status: None,
            paid_at_from: Some(1_700_000_000),
            paid_at_to: None,
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.q.as_deref(), Some("SO-1"));
        assert_eq!(query.mall_id.as_deref(), Some("mall-a"));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params = MallOrderFactListParams {
            mall_id: None,
            fact_type: None,
            processing_status: None,
            after_sales_request_id: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());
    }
}
