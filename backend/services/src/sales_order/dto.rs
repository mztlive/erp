//! 域 D13 `sales_order` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；金额/数量/单价/税率按 P0 约定字符串序列化
//! （`entities::money` 的 human-readable 形态）；时间一律秒级时间戳；业务日期
//! `YYYY-MM-DD`。
//!
//! 契约来源：erp-client `features/sales-orders`（W05）；本域接口按后端实体字段
//! 形状提供，与前端 mock 视图的差异见批次报告「契约变更」。

use entities::common::time::BusinessDate;
use entities::ids::{ContractId, CustomerAccountId, PartyId, SkuId, SourceSystemId};
use entities::money::{Amount, Quantity, Rate, UnitPrice};
use entities::sales_order::{
    BusinessType, CardForm, CommercialStatus, FulfillmentMode, GoodsLineFields, LineStatus, LineType,
    OriginSystem, VoucherLineDraft, WelfareScenario,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::work_item::{ProcessingBlockerView, ProcessingState, WorkItemPartyView};

use crate::errors::Result;
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 销售单列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const SALES_ORDER_SORT_FIELDS: &[&str] = &["created_at", "order_no"];

/// 排序方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    /// 升序。
    Asc,
    /// 降序。
    Desc,
}

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
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
            .ok_or_else(|| crate::errors::Error::ValidationError(format!("不支持的排序字段: {field}")))?,
        None => "created_at",
    };
    let sort_dir = match sort_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some("asc") => SortDir::Asc,
        Some("desc") => SortDir::Desc,
        Some(other) => {
            return Err(crate::errors::Error::ValidationError(format!(
                "非法排序方向: {other}"
            )))
        }
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

/// 校验文本去除首尾空白后非空。
fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

/// 建单意图（W05 M5：保存草稿或直接提交）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SalesOrderCreateIntent {
    /// 保存草稿。
    SaveDraft,
    /// 提交进入审核。
    Submit,
}

/// 草稿行请求（字段组按 `line_type` 二选一；金额与跨行断言由实体 `new` 校验）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SalesOrderDraftLineRequest {
    /// 行号（从 1 递增，变更不复用历史行号）。
    #[validate(range(min = 1, message = "行号必须为正整数"))]
    pub line_no: u32,
    /// 行类型。
    pub line_type: LineType,
    /// 销项税率（字符串，6 位小数语义）。
    pub sales_tax_rate: Rate,
    /// 销售项名称快照。
    #[validate(custom(function = "non_blank", message = "销售项名称不能为空"))]
    pub item_name_snapshot: String,
    /// 规格快照。
    pub spec_snapshot: Option<String>,
    /// 单位快照。
    pub unit_snapshot: Option<String>,
    /// 实物及服务字段组（`GOODS_SERVICE` 行必填）。
    pub goods: Option<GoodsLineFields>,
    /// 卡券字段组（`VOUCHER` 行必填）。
    pub voucher: Option<VoucherLineDraft>,
}

/// 草稿表头请求（工作副本头字段，`HeaderSnapshots::build` 在实体层统一规范化）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SalesOrderDraftRequest {
    /// 当前草稿责任人。
    #[validate(custom(function = "non_blank", message = "编辑人不能为空"))]
    pub editor_user_id: String,
    /// 客户名称快照。
    #[validate(custom(function = "non_blank", message = "客户名称不能为空"))]
    pub customer_name: String,
    /// 合同编号快照；无合同时省略。
    pub contract_no: Option<String>,
    /// 用户明确选择的合同不可变版本；有合同时必填。
    pub requested_contract_revision_id: Option<entities::ids::ContractRevisionId>,
    /// 结算主体名称快照；与 `settlement_party_id` 同时提供。
    pub settlement_party_name: Option<String>,
    /// 付款条件代码。
    #[validate(custom(function = "non_blank", message = "付款条件代码不能为空"))]
    pub payment_term_code: String,
    /// 付款条件名称。
    #[validate(custom(function = "non_blank", message = "付款条件名称不能为空"))]
    pub payment_term_name: String,
    /// 开票类型。
    #[validate(custom(function = "non_blank", message = "开票类型不能为空"))]
    pub invoice_type: String,
    /// 税点。
    #[validate(custom(function = "non_blank", message = "税点不能为空"))]
    pub tax_point: String,
    /// 客户项目名称。
    pub project_name: Option<String>,
    /// 业务备注。
    pub business_remark: Option<String>,
    /// 卡券类目 SKU（卡券单必填）。
    pub voucher_category_sku_id: Option<SkuId>,
    /// 卡券履约期限（秒级时间戳，卡券单必填）。
    pub voucher_expiry_at: Option<u64>,
    /// 卡券执行投影的目标商城；非卡券单为空。
    pub target_mall_id: Option<SourceSystemId>,
    /// 卡券最终通过时形成应收所使用的到期日；非卡券单为空。
    pub receivable_due_date: Option<BusinessDate>,
    /// 草稿行清单（非空，上限 200 行）。
    #[validate(length(min = 1, max = 200, message = "明细行数必须在1-200之间"))]
    pub lines: Vec<SalesOrderDraftLineRequest>,
}

/// 创建销售单请求（W05 M5：合同 + 表头 + 明细 + 意图 + 幂等键）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSalesOrderRequest {
    /// 销售单号（唯一，创建后不可修改）。
    #[validate(custom(function = "non_blank", message = "销售单号不能为空"))]
    pub order_no: String,
    /// 业务性质（创建后永久不变）。
    pub business_type: BusinessType,
    /// 客户稳定身份。
    pub customer_id: CustomerAccountId,
    /// 合同稳定身份（无合同时省略）。
    pub contract_id: Option<ContractId>,
    /// 结算主体。
    pub settlement_party_id: PartyId,
    /// 幂等键；同一操作人、同一键和同一完整载荷返回原销售单，异载荷返回冲突。
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
    /// 建单意图。
    pub intent: SalesOrderCreateIntent,
    /// 草稿表头与明细。
    #[validate(nested)]
    pub draft: SalesOrderDraftRequest,
}

/// 保存草稿请求（乐观锁：携带期望版本；整表头覆盖 + 明细整批替换）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SaveWorkingCopyRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 草稿表头与明细。
    #[validate(nested)]
    pub draft: SalesOrderDraftRequest,
}

/// 提交销售单请求（幂等键 + 期望版本）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SubmitSalesOrderRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝提交（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 幂等键（重复提交按「同一草稿已提交」去重，返回既有提交）。
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 作废销售单请求（乐观锁：携带期望版本）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct VoidSalesOrderRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝作废（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
}

/// 撤回销售单审批请求。原因必填。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CancelSalesOrderApprovalRequest {
    /// 期望的单据乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 非空撤回原因。
    #[validate(length(min = 1, max = 512, message = "撤回原因不能为空"))]
    pub reason: String,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 销售单列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SalesOrderListParams {
    /// 销售单号（字面量模糊筛选）。
    pub order_no: Option<String>,
    /// 客户筛选。
    pub customer_id: Option<CustomerAccountId>,
    /// 合同筛选。
    pub contract_id: Option<ContractId>,
    /// 最初创建入口筛选。
    pub origin_system: Option<OriginSystem>,
    /// 商业主状态筛选。
    pub commercial_status: Option<CommercialStatus>,
    /// 审核轨状态筛选。
    pub review_status: Option<entities::sales_order::ReviewStatus>,
    /// 业务性质筛选。
    pub business_type: Option<BusinessType>,
    /// 履约进度筛选。
    pub fulfillment_progress: Option<entities::sales_order::FulfillmentProgress>,
    /// 回款进度筛选。
    pub collection_progress: Option<entities::sales_order::CollectionProgress>,
    /// 开票进度筛选。
    pub invoice_progress: Option<entities::sales_order::InvoiceProgress>,
    /// 关闭状态筛选。
    pub close_status: Option<entities::sales_order::CloseStatus>,
    /// 创建时间下界（含，秒级时间戳）。
    pub created_from: Option<u64>,
    /// 创建时间上界（含，秒级时间戳）。
    pub created_to: Option<u64>,
    /// 创建人账号筛选（"我创建的"/"待我处理"视图用）。
    pub created_by: Option<String>,
    /// "待我处理"视图：仅草稿或被驳回/低毛利待处理回销售的单；与
    /// `commercial_status`/`review_status` 互斥，调用方不应同时传两者。
    #[serde(default)]
    pub my_todo: bool,
    /// "异常"视图：审核轨被驳回；与 `review_status` 互斥。
    #[serde(default)]
    pub exception_only: bool,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`order_no`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的销售单列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SalesOrderListQuery {
    /// 销售单号筛选。
    pub order_no: Option<String>,
    /// 客户筛选。
    pub customer_id: Option<String>,
    /// 合同筛选。
    pub contract_id: Option<String>,
    /// 最初创建入口筛选。
    pub origin_system: Option<OriginSystem>,
    /// 商业主状态筛选。
    pub commercial_status: Option<CommercialStatus>,
    /// 审核轨状态筛选。
    pub review_status: Option<entities::sales_order::ReviewStatus>,
    /// 业务性质筛选。
    pub business_type: Option<BusinessType>,
    /// 履约进度筛选。
    pub fulfillment_progress: Option<entities::sales_order::FulfillmentProgress>,
    /// 回款进度筛选。
    pub collection_progress: Option<entities::sales_order::CollectionProgress>,
    /// 开票进度筛选。
    pub invoice_progress: Option<entities::sales_order::InvoiceProgress>,
    /// 关闭状态筛选。
    pub close_status: Option<entities::sales_order::CloseStatus>,
    /// 创建时间下界（含）。
    pub created_from: Option<u64>,
    /// 创建时间上界（含）。
    pub created_to: Option<u64>,
    /// 创建人账号筛选。
    pub created_by: Option<String>,
    /// "待我处理"视图。
    pub my_todo: bool,
    /// "异常"视图。
    pub exception_only: bool,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SalesOrderListParams {
    /// 归一化销售单列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SalesOrderListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, SALES_ORDER_SORT_FIELDS)?;
        if matches!((self.created_from, self.created_to), (Some(from), Some(to)) if from > to) {
            return Err(crate::errors::Error::ValidationError(
                "创建时间下界不能晚于上界".to_string(),
            ));
        }
        Ok(SalesOrderListQuery {
            order_no: normalized_text(self.order_no.as_deref()),
            customer_id: self.customer_id.as_ref().map(ToString::to_string),
            contract_id: self.contract_id.as_ref().map(ToString::to_string),
            origin_system: self.origin_system,
            commercial_status: self.commercial_status,
            review_status: self.review_status,
            business_type: self.business_type,
            fulfillment_progress: self.fulfillment_progress,
            collection_progress: self.collection_progress,
            invoice_progress: self.invoice_progress,
            close_status: self.close_status,
            created_from: self.created_from,
            created_to: self.created_to,
            created_by: normalized_text(self.created_by.as_deref()),
            my_todo: self.my_todo,
            exception_only: self.exception_only,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 销售单列表行视图（契约形状；金额以字符串序列化）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesOrderView {
    /// 实体主键。
    pub id: String,
    /// 销售单号。
    pub order_no: String,
    /// 业务性质。
    pub business_type: BusinessType,
    /// 最初创建入口。
    pub origin_system: OriginSystem,
    /// 客户稳定身份。
    pub customer_id: String,
    /// 合同稳定身份。
    pub contract_id: Option<String>,
    /// 商业主状态。
    pub commercial_status: CommercialStatus,
    /// 审核轨状态。
    pub review_status: entities::sales_order::ReviewStatus,
    /// 履约进度。
    pub fulfillment_progress: entities::sales_order::FulfillmentProgress,
    /// 回款进度。
    pub collection_progress: entities::sales_order::CollectionProgress,
    /// 开票进度。
    pub invoice_progress: entities::sales_order::InvoiceProgress,
    /// 关闭状态。
    pub close_status: entities::sales_order::CloseStatus,
    /// 生效时间（秒级时间戳）。
    pub effective_at: Option<u64>,
    /// ERP 关闭时间（秒级时间戳）。
    pub closed_at: Option<u64>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 更新时间（秒级时间戳）。
    pub updated_at: u64,
    /// 负责销售账号。
    pub owner_user_id: String,
    /// 负责销售姓名；账号已删除时为空。
    pub owner_user_name: Option<String>,
    /// 当前阶段摘要（服务端权威计算，替代前端字符串拼接）。
    pub stage: SalesOrderStageSummary,
}

/// 销售单当前阶段摘要（列表行与详情共用）。
///
/// `code` 与 erp-client `filter-orders.ts::SALES_ORDER_STATUS_OPTIONS` 的 9 个
/// 筛选码一一对应，前端筛选逻辑改为直接比较该码，不再维护一份中文 label 匹配。
/// 责任人/时限来自批量查询命中的待办（详情单条查、列表整页批量查，见
/// `SalesOrderService::sales_order_list`），审核轨未在途或无命中待办时为 `None`。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesOrderStageSummary {
    /// 阶段码：`draft`/`awaiting_confirm`/`awaiting_sales`/`awaiting_sales_lead`/
    /// `awaiting_ops`/`fulfilling`/`effective`/`closed`/`voided`。
    pub code: &'static str,
    /// 中文展示文案。
    pub label: &'static str,
    /// 展示语气：`success`/`warning`/`info`/`void`/`neutral`。
    pub tone: &'static str,
    /// 当前责任岗位（来自命中的待办 `owner_role`）；无待办时为 `None`。
    pub owner_role: Option<String>,
    /// 当前责任人账号（来自命中的待办 `owner_user_id`）；团队待处理时为 `None`。
    pub owner_user_id: Option<String>,
    /// 当前责任人姓名。
    pub owner_user_name: Option<String>,
    /// 预计完成时限（秒级时间戳）；当前待办派发时尚未赋值时为 `None`。
    pub due_at: Option<u64>,
}

/// 结案资格判定（服务端权威；移植自 erp-client `close-eligibility.ts`）。
///
/// 规则（W05 §5.3/§12）：非卡券以客户验收完成判定交付；卡券以履约期限到期判定
/// （不因已消费完提前算完成）；结案门槛为交付完成且回款收齐，开票进度不参与。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CloseEligibilityView {
    /// 交付是否已完成。
    pub fulfillment_complete: bool,
    /// 应收是否已结清。
    pub receivable_settled: bool,
    /// 开票是否已完成（不影响 `eligible_to_close`）。
    pub invoice_complete: bool,
    /// 是否具备结案资格。
    pub eligible_to_close: bool,
    /// 阻塞原因（`eligible_to_close=false` 时非空）。
    pub blockers: Vec<String>,
    /// 面向用户的说明文案。
    pub note: String,
}

/// 明细行视图（金额/数量/单价字符串序列化）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesOrderLineView {
    /// 实体主键。
    pub id: String,
    /// 单内稳定行号。
    pub line_no: u32,
    /// 行状态。
    pub line_status: LineStatus,
}

/// 工作副本视图（草稿）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkingCopyView {
    /// 实体主键。
    pub id: String,
    /// 乐观锁版本（`save_working_copy` 的 `SaveWorkingCopyRequest.version` 按此比对）。
    pub version: u64,
    /// 编辑目的。
    pub working_purpose: entities::sales_order::WorkingPurpose,
    /// 草稿状态。
    pub status: entities::sales_order::WorkingCopyStatus,
    /// 草稿版本。
    pub draft_version: u32,
    /// 内容指纹。
    pub content_hash: String,
    /// 当前草稿责任人。
    pub editor_user_id: String,
    /// 业务性质。
    pub business_type: BusinessType,
    /// 客户名称快照。
    pub customer_name: String,
    /// 合同编号快照。
    pub contract_no: Option<String>,
    /// 草稿冻结的合同不可变版本；有合同时必须回显该值。
    pub contract_revision_id: Option<String>,
    /// 结算主体名称快照。
    pub settlement_party_name: Option<String>,
    /// 付款条件代码。
    pub payment_term_code: String,
    /// 付款条件名称。
    pub payment_term_name: String,
    /// 开票类型。
    pub invoice_type: String,
    /// 税点。
    pub tax_point: String,
    /// 客户项目名称（前端福利场景）。
    pub project_name: Option<String>,
    /// 业务备注。
    pub business_remark: Option<String>,
    /// 卡券类目 SKU。
    pub voucher_category_sku_id: Option<String>,
    /// 卡券履约期限（秒级时间戳）。
    pub voucher_expiry_at: Option<u64>,
    /// 卡券执行投影的目标商城。
    pub target_mall_id: Option<String>,
    /// 卡券最终通过时形成应收所使用的到期日。
    pub receivable_due_date: Option<BusinessDate>,
    /// 草稿行汇总（含税）。
    pub gross_amount: Amount,
    /// 草稿行汇总（不含税）。
    pub net_amount: Amount,
    /// 草稿行汇总（税额）。
    pub tax_amount: Amount,
    /// 草稿行清单。
    pub lines: Vec<SalesOrderWorkingCopyLineView>,
}

/// 工作副本行视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesOrderWorkingCopyLineView {
    /// 实体主键。
    pub id: String,
    /// 稳定明细身份。
    pub sales_order_line_id: String,
    /// 行号。
    pub line_no: u32,
    /// 行类型。
    pub line_type: LineType,
    /// 行含税金额。
    pub gross_amount: Amount,
    /// 行不含税金额。
    pub net_amount: Amount,
    /// 行税额。
    pub tax_amount: Amount,
    /// 销项税率。
    pub sales_tax_rate: Rate,
    /// 销售项名称快照。
    pub item_name_snapshot: String,
    /// 规格快照。
    pub spec_snapshot: Option<String>,
    /// 单位快照。
    pub unit_snapshot: Option<String>,
    /// 正式销售项 SKU。
    pub sku_id: Option<SkuId>,
    /// 下单锁定的精确 SKU 修订。
    pub sku_revision_id: Option<entities::ids::SkuRevisionId>,
    /// 福利场景。
    pub welfare_scenario: Option<WelfareScenario>,
    /// 销售提交承诺的履约方式。
    pub fulfillment_mode: Option<FulfillmentMode>,
    /// 销售提交承诺的履约期限（秒级时间戳）。
    pub fulfillment_due_at: Option<u64>,
    /// 基础单位数量。
    pub quantity: Option<Quantity>,
    /// 基础单位代码。
    pub base_unit_code: Option<String>,
    /// 含税成交单价快照。
    pub unit_price_gross: Option<UnitPrice>,
    /// 单卡面额。
    pub face_value: Option<Amount>,
    /// 卡张数。
    pub card_count: Option<u32>,
    /// 最终成交金额。
    pub transaction_amount: Option<Amount>,
    /// 卡形态。
    pub card_form: Option<CardForm>,
}

/// 提交历史视图（含表头快照与明细，供提交后详情展示）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SubmissionView {
    /// 实体主键。
    pub id: String,
    /// 提交序号。
    pub submission_no: u32,
    /// 提交状态。
    pub status: entities::sales_order::SubmissionStatus,
    /// 业务性质。
    pub business_type: BusinessType,
    /// 客户名称快照。
    pub customer_name: String,
    /// 合同编号快照。
    pub contract_no: Option<String>,
    /// 提交时锁定的合同修订。
    pub contract_revision_id: Option<String>,
    /// 结算主体名称快照。
    pub settlement_party_name: Option<String>,
    /// 付款条件代码。
    pub payment_term_code: String,
    /// 付款条件名称。
    pub payment_term_name: String,
    /// 开票类型。
    pub invoice_type: String,
    /// 税点。
    pub tax_point: String,
    /// 客户项目名称（前端福利场景）。
    pub project_name: Option<String>,
    /// 业务备注。
    pub business_remark: Option<String>,
    /// 卡券类目 SKU。
    pub voucher_category_sku_id: Option<String>,
    /// 卡券履约期限（秒级时间戳）。
    pub voucher_expiry_at: Option<u64>,
    /// 卡券执行投影的目标商城。
    pub target_mall_id: Option<String>,
    /// 提交时服务端解析并冻结的商城客户身份。
    pub customer_external_identity: Option<String>,
    /// 提交时服务端解析并冻结的商城卡券类目身份。
    pub voucher_category_external_identity: Option<String>,
    /// 最终通过时形成应收所使用的到期日。
    pub receivable_due_date: Option<BusinessDate>,
    /// 提交行汇总（含税）。
    pub gross_amount: Amount,
    /// 提交行汇总（不含税）。
    pub net_amount: Amount,
    /// 提交行汇总（税额）。
    pub tax_amount: Amount,
    /// 提交审计人。
    pub submitted_by: String,
    /// 提交时间（秒级时间戳）。
    pub submitted_at: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 提交明细（与工作副本行视图同形，便于前端统一映射）。
    pub lines: Vec<SalesOrderWorkingCopyLineView>,
}

/// 销售版本视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RevisionView {
    /// 实体主键。
    pub id: String,
    /// 版本号。
    pub revision_no: u32,
    /// 版本来源。
    pub revision_source: entities::sales_order::RevisionSource,
    /// 内容指纹。
    pub content_hash: String,
    /// 含税合计。
    pub gross_amount: Amount,
    /// 不含税合计。
    pub net_amount: Amount,
    /// 税额合计。
    pub tax_amount: Amount,
    /// 生效时间（秒级时间戳）。
    pub effective_at: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 开放中的采购二次确认驳回摘要（销售单详情内嵌，供销售处理）。
///
/// 仅在「销售单仍为草稿、存在驳回确认、且无新的待处理确认」时出现；不依赖
/// 采购队列 list 权限，避免销售角色静默丢入口。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OpenProcurementRejectionView {
    /// 被驳回的采购确认 ID。
    pub procurement_confirmation_id: String,
    /// 被驳回的销售提交快照 ID。
    pub submission_id: String,
    /// 驳回原因代码（稳定码，如 `CANNOT_FULFILL`）。
    pub reject_reason_code: Option<String>,
    /// 驳回补充说明。
    pub comment: Option<String>,
    /// 采购处理人账号 ID。
    pub handled_by: Option<String>,
    /// 采购处理人姓名；账号已不存在时为 `None`，前端不得回退展示 `handled_by`。
    pub handled_by_name: Option<String>,
    /// 处理时间（秒级时间戳）。
    pub handled_at: Option<u64>,
    /// 当前操作人可执行的固定销售处置动作。
    pub allowed_actions: Vec<ProcurementRejectionAllowedAction>,
}

/// 采购驳回处理卡的服务端权威动作。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProcurementRejectionAllowedAction {
    /// 改品或改价后重提。
    ResubmitChangedTerms,
    /// 照原条件申请低毛利承接。
    RequestLowMarginAcceptance,
    /// 确认不做并作废。
    VoidAfterRejection,
}

/// 低毛利上级确认的服务端权威领域动作。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LowMarginManagerAllowedAction {
    /// 通过低毛利承接。
    Approve,
    /// 驳回低毛利承接。
    Reject,
}

/// 销售单详情内嵌的活动低毛利上级确认。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ActiveLowMarginManagerConfirmationView {
    /// 低毛利确认事实。
    pub confirmation_id: String,
    /// 当前待办。
    pub work_item_id: String,
    /// 当前待办版本。
    pub task_version: String,
    /// 冻结提交版本。
    pub subject_version: String,
    /// 被审批的新提交。
    pub low_margin_submission_id: String,
    /// 原驳回采购确认。
    pub rejected_procurement_confirmation_id: String,
    /// 销售承接理由。
    pub acceptance_reason: String,
    /// 受控证据引用。
    pub evidence_reference_ids: Vec<String>,
    /// 当前责任人安全摘要。
    pub owner_user: Option<WorkItemPartyView>,
    /// 当前操作人的领域动作。
    pub allowed_actions: Vec<LowMarginManagerAllowedAction>,
    /// 当前阻断原因。
    pub action_blockers: Vec<ProcessingBlockerView>,
}

/// 卡券销售审批工作面允许执行的固定动作。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CardSalesApprovalAllowedAction {
    /// 提交通过决定。
    Approve,
    /// 提交驳回决定。
    Reject,
    /// 提交终止决定；该动作不会形成驳回记录。
    Terminate,
    /// 由原提交人撤回尚未形成不可逆决定的审批。
    Cancel,
}

/// 销售单详情内嵌的唯一活动卡券审批投影。
///
/// 首步骤责任解析失败时实例和步骤仍会以 `BLOCKED` 落库，但可以没有待办；
/// 因此任务字段均允许为空，页面不得为此伪造任务身份或版本。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ActiveCardSalesApprovalView {
    /// 审批实例。
    pub approval_instance_id: String,
    /// 审批实例乐观锁版本。
    pub instance_version: u64,
    /// 当前审批步骤。
    pub approval_step_instance_id: String,
    /// 当前步骤乐观锁版本。
    pub step_version: u64,
    /// 当前开放待办；解析阻塞且尚未形成待办时为空。
    pub work_item_id: Option<String>,
    /// 当前待办乐观锁版本。
    pub task_version: Option<u64>,
    /// 当前固定任务类型。
    pub work_item_type: Option<entities::work_item::WorkItemType>,
    /// 当前任务状态。
    pub work_item_status: Option<entities::work_item::WorkItemStatus>,
    /// 当前处理状态。
    pub processing_state: ProcessingState,
    /// 权限安全的阻塞摘要。
    pub processing_blocker: Option<ProcessingBlockerView>,
    /// 当前个人责任人安全摘要。
    pub owner_user: Option<WorkItemPartyView>,
    /// 冻结业务版本。
    pub subject_version: String,
    /// 被审批的销售提交。
    pub sales_order_submission_id: String,
    /// 提交序号。
    pub submission_no: u32,
    /// 权限安全的冻结提交摘要。
    pub frozen_submission_summary: String,
    /// 当前步骤要求的销售审核轨状态。
    pub expected_review_status: String,
    /// 服务端当前允许动作。
    pub allowed_actions: Vec<CardSalesApprovalAllowedAction>,
    /// 当前不可执行原因。
    pub action_blockers: Vec<ProcessingBlockerView>,
}

/// 销售单详情视图（订单 + 稳定明细 + 草稿 + 提交历史 + 版本历史）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesOrderDetailView {
    /// 实体主键。
    pub id: String,
    /// 销售单号。
    pub order_no: String,
    /// 业务性质。
    pub business_type: BusinessType,
    /// 最初创建入口。
    pub origin_system: OriginSystem,
    /// 客户稳定身份。
    pub customer_id: String,
    /// 合同稳定身份。
    pub contract_id: Option<String>,
    /// 结算主体。
    pub settlement_party_id: String,
    /// 商业主状态。
    pub commercial_status: CommercialStatus,
    /// 审核轨状态。
    pub review_status: entities::sales_order::ReviewStatus,
    /// 履约/回款/开票/关闭进度。
    pub fulfillment_progress: entities::sales_order::FulfillmentProgress,
    /// 回款进度。
    pub collection_progress: entities::sales_order::CollectionProgress,
    /// 开票进度。
    pub invoice_progress: entities::sales_order::InvoiceProgress,
    /// 关闭状态。
    pub close_status: entities::sales_order::CloseStatus,
    /// 当前生效版本。
    pub current_revision_id: Option<String>,
    /// 生效时间（秒级时间戳）。
    pub effective_at: Option<u64>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 当前销售单负责人账号。
    pub owner_user_id: String,
    /// 当前销售单负责人姓名。
    pub owner_user_name: Option<String>,
    /// 当前关联的未作废采购单数量。
    pub purchase_order_count: u64,
    /// 应收子账已核销含税合计。
    pub settled_total: Amount,
    /// 应收子账净已开票含税合计。
    pub invoiced_total: Amount,
    /// 稳定明细行。
    pub lines: Vec<SalesOrderLineView>,
    /// 有效草稿（首次提交目的）。
    pub working_copy: Option<WorkingCopyView>,
    /// 提交历史（新提交在前）。
    pub submissions: Vec<SubmissionView>,
    /// 版本历史（新版本在前）。
    pub revisions: Vec<RevisionView>,
    /// 当前阶段详情（含责任人与时限）。
    pub stage: SalesOrderStageSummary,
    /// 结案资格判定。
    pub close_eligibility: CloseEligibilityView,
    /// 是否可以发起销售变更单。
    pub can_start_sales_change_order: bool,
    /// 不可发起销售变更单时的原因；可发起时为 `None`。
    pub change_order_blocker: Option<String>,
    /// 开放中的采购驳回（无则为 `None`）。
    pub open_procurement_rejection: Option<OpenProcurementRejectionView>,
    /// 唯一活动卡券审批；非卡券、无活动实例或数据不完整时为空。
    pub active_card_sales_approval: Option<ActiveCardSalesApprovalView>,
    /// 唯一活动低毛利上级确认。
    pub active_low_margin_manager_confirmation: Option<ActiveLowMarginManagerConfirmationView>,
    /// 统一只读审批结构（`SalesOrder` 与 `VoucherSalesOrder`）。
    pub approval: Option<DocumentApprovalView>,
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

#[cfg(test)]
mod tests {
    use super::{normalize_sort, SortDir};

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" order_no ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "order_no"],
        )
        .unwrap();
        assert_eq!(field, "order_no");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn list_params_normalize_filters_and_paging() {
        use super::SalesOrderListParams;
        use serde_json::json;

        let params: SalesOrderListParams = serde_json::from_value(json!({
            "order_no": " SO-2026 ",
            "customer_id": "cust-1",
            "contract_id": "contract-1",
            "origin_system": "ERP",
            "business_type": "GOODS_SERVICE",
            "fulfillment_progress": "PARTIALLY_FULFILLED",
            "collection_progress": "PARTIALLY_COLLECTED",
            "invoice_progress": "PARTIALLY_INVOICED",
            "close_status": "CLOSEABLE",
            "created_from": 1700000000,
            "created_to": 1800000000,
            "page_size": 50,
        }))
        .unwrap();
        let query = params.normalized().unwrap();
        assert_eq!(query.order_no.as_deref(), Some("SO-2026"));
        assert_eq!(query.customer_id.as_deref(), Some("cust-1"));
        assert_eq!(query.contract_id.as_deref(), Some("contract-1"));
        assert_eq!(query.origin_system, Some(super::OriginSystem::Erp));
        assert_eq!(query.business_type, Some(super::BusinessType::GoodsService));
        assert_eq!(
            query.fulfillment_progress,
            Some(entities::sales_order::FulfillmentProgress::PartiallyFulfilled)
        );
        assert_eq!(query.created_from, Some(1_700_000_000));
        assert_eq!(query.created_to, Some(1_800_000_000));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 50);
    }

    #[test]
    fn list_params_reject_reversed_created_range() {
        use super::SalesOrderListParams;
        use serde_json::json;

        let params: SalesOrderListParams = serde_json::from_value(json!({
            "created_from": 1800000000,
            "created_to": 1700000000,
        }))
        .unwrap();

        assert!(params.normalized().is_err());
    }
}
