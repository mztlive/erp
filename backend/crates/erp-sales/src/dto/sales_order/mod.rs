//! 域 D13 `sales_order` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；金额/数量/单价/税率按 P0 约定字符串序列化
//! （`entities::money` 的 human-readable 形态）；时间一律秒级时间戳；业务日期
//! `YYYY-MM-DD`。
//!
//! 契约来源：erp-client `features/sales-orders`（W05）；本域接口按后端实体字段
//! 形状提供，与前端 mock 视图的差异见批次报告「契约变更」。

use crate::entity::sales_order::{
    BusinessType, CardForm, CommercialStatus, GoodsLineFields, LineStatus, LineType, OriginSystem,
    VoucherLineDraft, WelfareScenario,
};
use erp_core::common::time::BusinessDate;
use erp_core::ids::{ContractId, CustomerAccountId, SkuId};
use erp_core::money::{Amount, Quantity, Rate, UnitPrice};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::Result;
use application_core::{normalized_text, page_or_default, page_size_or_default};

/// 销售单列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const SALES_ORDER_SORT_FIELDS: &[&str] = &["created_at", "order_no"];

/// 排序方向。
pub use application_core::SortDir;

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
pub(crate) use application_core::normalize_sort;

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use application_core::PageView;

/// 校验文本去除首尾空白后非空。
use application_core::non_blank;

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
    pub requested_contract_revision_id: Option<erp_core::ids::ContractRevisionId>,
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
    /// 卡券最终通过时形成应收所使用的到期日；非卡券单为空。
    pub receivable_due_date: Option<BusinessDate>,
    /// 草稿行清单（非空，上限 200 行）。
    #[validate(length(min = 1, max = 200, message = "明细行数必须在1-200之间"))]
    pub lines: Vec<SalesOrderDraftLineRequest>,
}

/// 前端可编辑的销售草稿命令。
///
/// 客户、合同号、结算主体、付款条件与开票要求均由服务端按所选合同修订冻结，
/// 客户端不得复制后再回传这些权威快照。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SalesOrderEditableDraftRequest {
    /// 当前草稿责任人。
    #[validate(custom(function = "non_blank", message = "编辑人不能为空"))]
    pub editor_user_id: String,
    /// 用户明确选择的合同不可变版本。
    pub requested_contract_revision_id: erp_core::ids::ContractRevisionId,
    /// 客户项目名称。
    pub project_name: Option<String>,
    /// 业务备注。
    pub business_remark: Option<String>,
    /// 卡券类目 SKU（卡券单必填）。
    pub voucher_category_sku_id: Option<SkuId>,
    /// 卡券履约期限（秒级时间戳，卡券单必填）。
    pub voucher_expiry_at: Option<u64>,
    /// 卡券最终通过时形成应收所使用的到期日；非卡券单为空。
    pub receivable_due_date: Option<BusinessDate>,
    /// 草稿行清单（非空，上限 200 行）。
    #[validate(length(min = 1, max = 200, message = "明细行数必须在1-200之间"))]
    #[validate(nested)]
    pub lines: Vec<SalesOrderDraftLineRequest>,
}

/// 创建销售单请求（W05 M5：合同 + 可编辑草稿 + 意图 + 幂等键）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateSalesOrderRequest {
    /// 销售单号（唯一，创建后不可修改）。
    #[validate(custom(function = "non_blank", message = "销售单号不能为空"))]
    pub order_no: String,
    /// 业务性质（创建后永久不变）。
    pub business_type: BusinessType,
    /// 合同稳定身份；客户与结算主体由服务端从当前合同修订解析。
    pub contract_id: ContractId,
    /// 幂等键；同一操作人、同一键和同一完整载荷返回原销售单，异载荷返回冲突。
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
    /// 建单意图。
    pub intent: SalesOrderCreateIntent,
    /// 客户端可编辑草稿；合同权威快照由服务端补齐。
    #[validate(nested)]
    pub draft: SalesOrderEditableDraftRequest,
}

/// 保存草稿请求（乐观锁：携带期望版本；服务端解析合同快照后整批替换）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SaveWorkingCopyRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 所选合同稳定身份。
    pub contract_id: ContractId,
    /// 客户端可编辑草稿；合同权威快照由服务端补齐。
    #[validate(nested)]
    pub draft: SalesOrderEditableDraftRequest,
}

/// 提交销售单请求（完整草稿 + 幂等键 + 期望版本）。
///
/// 服务端在一个事务中完成草稿替换、提交快照冻结与审批启动；前端不得先保存。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SubmitSalesOrderRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝提交（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 幂等键（重复提交按「同一草稿已提交」去重，返回既有提交）。
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
    /// 所选合同稳定身份。
    pub contract_id: ContractId,
    /// 本次提交的完整可编辑草稿。
    #[validate(nested)]
    pub draft: SalesOrderEditableDraftRequest,
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
    pub review_status: Option<crate::entity::sales_order::ReviewStatus>,
    /// 业务性质筛选。
    pub business_type: Option<BusinessType>,
    /// 履约进度筛选。
    pub fulfillment_progress: Option<crate::entity::sales_order::FulfillmentProgress>,
    /// 回款进度筛选。
    pub collection_progress: Option<crate::entity::sales_order::CollectionProgress>,
    /// 开票进度筛选。
    pub invoice_progress: Option<crate::entity::sales_order::InvoiceProgress>,
    /// 关闭状态筛选。
    pub close_status: Option<crate::entity::sales_order::CloseStatus>,
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
    pub review_status: Option<crate::entity::sales_order::ReviewStatus>,
    /// 业务性质筛选。
    pub business_type: Option<BusinessType>,
    /// 履约进度筛选。
    pub fulfillment_progress: Option<crate::entity::sales_order::FulfillmentProgress>,
    /// 回款进度筛选。
    pub collection_progress: Option<crate::entity::sales_order::CollectionProgress>,
    /// 开票进度筛选。
    pub invoice_progress: Option<crate::entity::sales_order::InvoiceProgress>,
    /// 关闭状态筛选。
    pub close_status: Option<crate::entity::sales_order::CloseStatus>,
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
            return Err(crate::Error::ValidationError(
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
    pub working_purpose: crate::entity::sales_order::WorkingPurpose,
    /// 草稿状态。
    pub status: crate::entity::sales_order::WorkingCopyStatus,
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
    pub sku_revision_id: Option<erp_core::ids::SkuRevisionId>,
    /// 福利场景。
    pub welfare_scenario: Option<WelfareScenario>,
    /// 采购责任解析使用的服务区域。
    pub service_region: Option<String>,
    /// 销售对客户承诺完成明细交付或服务的最晚时间（秒级时间戳）。
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
    pub status: crate::entity::sales_order::SubmissionStatus,
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

/// 销售版本公共行摘要（详情「版本」分区展示当时明细，不含子类型字段）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RevisionLineView {
    /// 本版展示顺序。
    pub line_no: u32,
    /// 销售项名称快照。
    pub item_name: String,
    /// 规格快照。
    pub spec: Option<String>,
    /// 单位快照。
    pub unit: Option<String>,
    /// 行含税金额。
    pub gross_amount: Amount,
}

/// 销售版本视图。
///
/// 表头快照来自不可变 `sales_order_revision`，明细摘要来自同版公共行；后续基础资料
/// 或改单不得回写这些字段。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RevisionView {
    /// 实体主键。
    pub id: String,
    /// 版本号。
    pub revision_no: u32,
    /// 版本来源。
    pub revision_source: crate::entity::sales_order::RevisionSource,
    /// 内容指纹。
    pub content_hash: String,
    /// 当时客户名称快照。
    pub customer_name: String,
    /// 当时合同编号快照；无合同时为空。
    pub contract_no: Option<String>,
    /// 当时锁定的合同修订。
    pub contract_revision_id: Option<String>,
    /// 当时结算主体名称快照。
    pub settlement_party_name: Option<String>,
    /// 当时付款条件代码。
    pub payment_term_code: String,
    /// 当时付款条件名称。
    pub payment_term_name: String,
    /// 当时开票类型。
    pub invoice_type: String,
    /// 当时税点。
    pub tax_point: String,
    /// 当时客户项目名称。
    pub project_name: Option<String>,
    /// 当时业务备注。
    pub business_remark: Option<String>,
    /// 前一生效版本。
    pub previous_revision_id: Option<String>,
    /// 前一生效版本号；无法在本单版本列表中解析时为空。
    pub previous_revision_no: Option<u32>,
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
    /// 明细摘要，如「年货礼盒、企业福利卡 共 2 项」；无行时为空串。
    pub line_summary: String,
    /// 当时公共行，按行号升序。
    pub lines: Vec<RevisionLineView>,
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
            Some(crate::entity::sales_order::FulfillmentProgress::PartiallyFulfilled)
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
