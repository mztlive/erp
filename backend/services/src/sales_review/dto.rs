//! 域 D14 `sales_review` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；金额/数量/单价/税率按 P0 约定字符串序列化；
//! 时间一律秒级时间戳；业务日期 `YYYY-MM-DD`。
//!
//! 契约来源：erp-client `features/procurement-confirmation`（W07）与
//! `features/sales-orders`（W05）；本域接口按后端实体字段形状提供，与前端 mock
//! 视图的差异见批次报告「契约变更」。

use entities::money::Quantity;
use entities::sales_order::{GoodsLineFields, LineType, VoucherLineDraft};
use entities::sales_review::{ProcurementConfirmationStatus, SalesChangeType};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::Result;
use crate::query::{page_or_default, page_size_or_default};

/// 审批记录列表允许的排序字段白名单。
pub(crate) const SALES_ORDER_REVIEW_SORT_FIELDS: &[&str] = &["created_at"];
/// 采购确认列表允许的排序字段白名单。
pub(crate) const PROCUREMENT_CONFIRMATION_SORT_FIELDS: &[&str] = &["created_at"];
/// 销售变更单列表允许的排序字段白名单。
pub(crate) const SALES_CHANGE_ORDER_SORT_FIELDS: &[&str] = &["created_at"];

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

// ---------------------------------------------------------------------------
// sales_order_review（销售单审批记录，W05 卡券审批轨）
// ---------------------------------------------------------------------------

/// 审批记录列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SalesOrderReviewListParams {
    /// 被审批的提交快照筛选。
    pub submission_id: Option<entities::ids::SalesOrderSubmissionId>,
    /// 销售单筛选。
    pub sales_order_id: Option<entities::ids::SalesOrderId>,
    /// 审批阶段筛选。
    pub review_stage: Option<entities::sales_review::SalesReviewStage>,
    /// 审批状态筛选。
    pub status: Option<entities::sales_review::SalesReviewStatus>,
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

/// 归一化后的审批记录列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SalesOrderReviewListQuery {
    /// 提交快照筛选。
    pub submission_id: Option<String>,
    /// 销售单筛选。
    pub sales_order_id: Option<String>,
    /// 审批阶段筛选。
    pub review_stage: Option<entities::sales_review::SalesReviewStage>,
    /// 审批状态筛选。
    pub status: Option<entities::sales_review::SalesReviewStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SalesOrderReviewListParams {
    /// 归一化审批记录列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SalesOrderReviewListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, SALES_ORDER_REVIEW_SORT_FIELDS)?;
        Ok(SalesOrderReviewListQuery {
            submission_id: self.submission_id.as_ref().map(ToString::to_string),
            sales_order_id: self.sales_order_id.as_ref().map(ToString::to_string),
            review_stage: self.review_stage,
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

/// 审批记录视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesOrderReviewView {
    /// 实体主键。
    pub id: String,
    /// 销售单。
    pub sales_order_id: String,
    /// 被审批的提交快照。
    pub submission_id: String,
    /// 审批阶段。
    pub review_stage: entities::sales_review::SalesReviewStage,
    /// 审批状态。
    pub status: entities::sales_review::SalesReviewStatus,
    /// 审批人。
    pub reviewer_id: Option<String>,
    /// 审批时间（秒级时间戳）。
    pub reviewed_at: Option<u64>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 审批决策请求（W05 卡券审批轨）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReviewDecisionRequest {
    /// 审批意见（通过时可空；驳回必填且非空白）。
    pub decision_reason: Option<String>,
}

// ---------------------------------------------------------------------------
// procurement_confirmation（采购二次确认，W07）
// ---------------------------------------------------------------------------

/// 采购确认列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProcurementConfirmationListParams {
    /// 被确认的销售提交筛选。
    pub submission_id: Option<entities::ids::SalesOrderSubmissionId>,
    /// 确认状态筛选。
    pub status: Option<ProcurementConfirmationStatus>,
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

/// 归一化后的采购确认列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcurementConfirmationListQuery {
    /// 提交快照筛选。
    pub submission_id: Option<String>,
    /// 确认状态筛选。
    pub status: Option<ProcurementConfirmationStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl ProcurementConfirmationListParams {
    /// 归一化采购确认列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<ProcurementConfirmationListQuery> {
        let (sort_by, sort_dir) = normalize_sort(
            &self.sort_by,
            &self.sort_dir,
            PROCUREMENT_CONFIRMATION_SORT_FIELDS,
        )?;
        Ok(ProcurementConfirmationListQuery {
            submission_id: self.submission_id.as_ref().map(ToString::to_string),
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

/// 采购确认分行请求（W07 保存分行草稿）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProcurementConfirmationLineRequest {
    /// 行号。
    #[validate(range(min = 1, message = "行号必须为正整数"))]
    pub line_no: u32,
    /// 被确认的提交快照明细 ID。
    pub sales_order_submission_line_id: entities::ids::SalesOrderSubmissionLineId,
    /// 确认供应商。
    pub supplier_id: entities::ids::SupplierAccountId,
    /// 采用的不可变供给修订。
    pub supplier_offering_revision_id: entities::ids::SupplierOfferingRevisionId,
    /// 确认可供数量。
    pub confirmed_quantity: Quantity,
    /// 最新含税成本。
    pub latest_cost_gross: entities::money::UnitPrice,
    /// 进项税率。
    pub input_tax_rate: entities::money::Rate,
    /// 预计交期（`YYYY-MM-DD`）。
    pub expected_delivery_date: entities::common::time::BusinessDate,
    /// 确认履约方式。
    pub fulfillment_mode: entities::sales_review::types::FulfillmentMode,
    /// 使用的能力版本。
    pub supplier_capability_revision_id: entities::ids::SupplierCapabilityRevisionId,
}

/// 保存采购确认分行请求（乐观锁：携带期望版本）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SaveProcurementConfirmationLinesRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 确认分行清单（非空，上限 200 行）。
    #[validate(length(min = 1, max = 200, message = "确认分行数必须在1-200之间"))]
    pub lines: Vec<ProcurementConfirmationLineRequest>,
}

/// 采购确认通过请求（幂等键）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ApproveProcurementConfirmationRequest {
    /// 幂等键（重复通过按「已通过」去重，返回既有结果）。
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 采购确认驳回请求（驳回原因代码必填）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RejectProcurementConfirmationRequest {
    /// 驳回原因代码。
    pub reject_reason_code: entities::sales_review::ProcurementRejectReasonCode,
    /// 补充说明。
    pub comment: Option<String>,
    /// 幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 采购确认列表行视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProcurementConfirmationView {
    /// 实体主键。
    pub id: String,
    /// 被确认的销售单。
    pub sales_order_id: String,
    /// 被确认的销售提交。
    pub submission_id: String,
    /// 确认状态。
    pub status: ProcurementConfirmationStatus,
    /// 采购处理人。
    pub handled_by: Option<String>,
    /// 处理时间（秒级时间戳）。
    pub handled_at: Option<u64>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 采购确认分行视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProcurementConfirmationLineView {
    /// 实体主键。
    pub id: String,
    /// 行号。
    pub line_no: u32,
    /// 被确认的提交快照明细 ID。
    pub sales_order_submission_line_id: String,
    /// 确认供应商。
    pub supplier_id: String,
    /// 采用的不可变供给修订；旧数据缺失时返回空值并阻断审批。
    pub supplier_offering_revision_id: Option<String>,
    /// 确认可供数量。
    pub confirmed_quantity: Quantity,
    /// 最新含税成本。
    pub latest_cost_gross: entities::money::UnitPrice,
    /// 进项税率。
    pub input_tax_rate: entities::money::Rate,
    /// 预计交期。
    pub expected_delivery_date: entities::common::time::BusinessDate,
    /// 确认履约方式。
    pub fulfillment_mode: entities::sales_review::types::FulfillmentMode,
    /// 使用的能力版本。
    pub supplier_capability_revision_id: String,
}

/// 采购确认详情视图（批次 + 分行）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProcurementConfirmationDetailView {
    /// 实体主键。
    pub id: String,
    /// 被确认的销售单。
    pub sales_order_id: String,
    /// 被确认的销售提交。
    pub submission_id: String,
    /// 确认状态。
    pub status: ProcurementConfirmationStatus,
    /// 处理人。
    pub handled_by: Option<String>,
    /// 处理时间（秒级时间戳）。
    pub handled_at: Option<u64>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 确认分行。
    pub lines: Vec<ProcurementConfirmationLineView>,
}

/// 采购确认决策结果视图（通过/驳回统一形状）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProcurementConfirmationDecisionView {
    /// 确认批次 ID。
    pub confirmation_id: String,
    /// 被确认的销售单。
    pub sales_order_id: String,
    /// 确认状态。
    pub status: ProcurementConfirmationStatus,
    /// 生效版本（通过时产生）。
    pub revision_id: Option<String>,
    /// 应收往来子账（通过时产生）。
    pub receivable_account_id: Option<String>,
    /// 处理时间（秒级时间戳）。
    pub handled_at: u64,
    /// 操作号（业务参考）。
    pub reference: String,
}

// ---------------------------------------------------------------------------
// sales_change_order（销售变更单，W05 变更轨）
// ---------------------------------------------------------------------------

/// 销售变更单列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SalesChangeOrderListParams {
    /// 原销售单筛选。
    pub sales_order_id: Option<entities::ids::SalesOrderId>,
    /// 变更状态筛选。
    pub status: Option<entities::sales_review::SalesChangeOrderStatus>,
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

/// 归一化后的销售变更单列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SalesChangeOrderListQuery {
    /// 原销售单筛选。
    pub sales_order_id: Option<String>,
    /// 变更状态筛选。
    pub status: Option<entities::sales_review::SalesChangeOrderStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SalesChangeOrderListParams {
    /// 归一化销售变更单列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SalesChangeOrderListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, SALES_CHANGE_ORDER_SORT_FIELDS)?;
        Ok(SalesChangeOrderListQuery {
            sales_order_id: self.sales_order_id.as_ref().map(ToString::to_string),
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

/// 变更目标草稿行请求（与销售草稿行同形）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SalesChangeLineRequest {
    /// 行号。
    #[validate(range(min = 1, message = "行号必须为正整数"))]
    pub line_no: u32,
    /// 行类型。
    pub line_type: LineType,
    /// 销项税率。
    pub sales_tax_rate: entities::money::Rate,
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

/// 变更目标草稿表头请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SalesChangeDraftRequest {
    /// 当前草稿责任人。
    #[validate(custom(function = "non_blank", message = "编辑人不能为空"))]
    pub editor_user_id: String,
    /// 客户名称快照。
    #[validate(custom(function = "non_blank", message = "客户名称不能为空"))]
    pub customer_name: String,
    /// 合同编号快照。
    pub contract_no: Option<String>,
    /// 结算主体名称快照。
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
    pub voucher_category_sku_id: Option<entities::ids::SkuId>,
    /// 卡券履约期限（秒级时间戳，卡券单必填）。
    pub voucher_expiry_at: Option<u64>,
    /// 目标行清单（非空，上限 200 行）。
    #[validate(length(min = 1, max = 200, message = "明细行数必须在1-200之间"))]
    pub lines: Vec<SalesChangeLineRequest>,
}

/// 创建销售变更单请求（草稿 + 变更工作副本原子形成）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSalesChangeOrderRequest {
    /// 原销售单。
    pub sales_order_id: entities::ids::SalesOrderId,
    /// 变更类型。
    pub change_type: SalesChangeType,
    /// 变更原因。
    #[validate(custom(function = "non_blank", message = "变更原因不能为空"))]
    pub reason: String,
    /// 幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
    /// 变更目标草稿表头与明细。
    #[validate(nested)]
    pub draft: SalesChangeDraftRequest,
}

/// 发起销售变更影响确认请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SubmitSalesChangeRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝提交（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 变更复核决策请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ChangeReviewDecisionRequest {
    /// 复核意见（通过时可空；驳回必填且非空白）。
    pub decision_reason: Option<String>,
    /// 幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 作废销售变更单请求（乐观锁：携带期望版本）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct VoidSalesChangeOrderRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
}

/// 销售变更单列表行视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesChangeOrderView {
    /// 实体主键。
    pub id: String,
    /// 原销售单。
    pub sales_order_id: String,
    /// 发起时当前版本。
    pub base_revision_id: String,
    /// 变更类型。
    pub change_type: SalesChangeType,
    /// 变更状态。
    pub status: entities::sales_review::SalesChangeOrderStatus,
    /// 当前不可变目标提交。
    pub current_submission_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 销售变更单详情视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesChangeOrderDetailView {
    /// 实体主键。
    pub id: String,
    /// 原销售单。
    pub sales_order_id: String,
    /// 发起时当前版本。
    pub base_revision_id: String,
    /// 变更类型。
    pub change_type: SalesChangeType,
    /// 变更原因。
    pub reason: String,
    /// 变更状态。
    pub status: entities::sales_review::SalesChangeOrderStatus,
    /// 当前不可变目标提交。
    pub current_submission_id: Option<String>,
    /// 目标完整内容指纹。
    pub target_content_hash: Option<String>,
    /// 生效后生成的新销售版本。
    pub effective_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

#[cfg(test)]
mod tests {
    use super::normalize_sort;
    use entities::sales_review::SalesChangeType;
    use serde_json::json;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("sideways".to_string()), &["created_at"]).is_err());
        let (field, direction) = normalize_sort(&None, &None, &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, super::SortDir::Desc);
    }

    #[test]
    fn list_params_normalize_filters_and_paging() {
        use super::ProcurementConfirmationListParams;
        use entities::sales_review::ProcurementConfirmationStatus;

        let params: ProcurementConfirmationListParams = serde_json::from_value(json!({
            "status": "PENDING",
            "page_size": 50,
        }))
        .unwrap();
        let query = params.normalized().unwrap();
        assert_eq!(query.status, Some(ProcurementConfirmationStatus::Pending));
        assert_eq!(query.paging.page_size, 50);
    }

    #[test]
    fn change_type_serializes_with_stable_code() {
        assert_eq!(
            serde_json::to_string(&SalesChangeType::Quantity).unwrap(),
            "\"QUANTITY\""
        );
    }
}
