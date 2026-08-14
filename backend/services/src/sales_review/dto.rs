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

use crate::work_item::WorkItemView;

use crate::errors::{Error, Result};
use crate::query::{page_or_default, page_size_or_default};

/// 审批决定记录列表允许的排序字段白名单。
pub(crate) const SALES_ORDER_REVIEW_SORT_FIELDS: &[&str] = &["reviewed_at", "created_at"];
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
    pub status: Option<entities::sales_review::SalesOrderReviewDecision>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`reviewed_at`/`created_at`）。
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
    pub status: Option<entities::sales_review::SalesOrderReviewDecision>,
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
    pub status: entities::sales_review::SalesOrderReviewDecision,
    /// 审批人。
    pub reviewer_id: String,
    /// 审批时间（秒级时间戳）。
    pub reviewed_at: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
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
#[serde(deny_unknown_fields)]
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
    /// 确认履约方式（W07 HTTP 稳定代码）。
    pub fulfillment_mode: ProcurementConfirmationFulfillmentMode,
    /// 使用的能力版本。
    pub supplier_capability_revision_id: entities::ids::SupplierCapabilityRevisionId,
}

/// W07 HTTP 使用的固定履约方式代码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProcurementConfirmationFulfillmentMode {
    /// 公司仓履约。
    Warehouse,
    /// 供应商直发。
    SupplierDirect,
    /// 电子交付。
    Electronic,
    /// 线下服务。
    Service,
}

impl From<ProcurementConfirmationFulfillmentMode> for entities::sales_review::types::FulfillmentMode {
    fn from(value: ProcurementConfirmationFulfillmentMode) -> Self {
        match value {
            ProcurementConfirmationFulfillmentMode::Warehouse => Self::CompanyWarehouse,
            ProcurementConfirmationFulfillmentMode::SupplierDirect => Self::SupplierDirect,
            ProcurementConfirmationFulfillmentMode::Electronic => Self::ElectronicDelivery,
            ProcurementConfirmationFulfillmentMode::Service => Self::OfflineService,
        }
    }
}

/// 保存采购确认工作数据的强类型动作。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SaveProcurementConfirmationAction {
    /// 路径必须指向的采购确认。
    #[validate(custom(function = "non_blank", message = "采购确认ID不能为空"))]
    #[validate(length(max = 128, message = "采购确认ID不能超过128个字符"))]
    pub confirmation_id: String,
    /// 当前采购确认冻结的销售提交。
    #[validate(custom(function = "non_blank", message = "销售提交ID不能为空"))]
    #[validate(length(max = 128, message = "销售提交ID不能超过128个字符"))]
    pub submission_id: String,
    /// 期望的采购确认编辑版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_edit_version: u64,
    /// 确认分行清单（非空，上限 200 行）。
    #[validate(length(min = 1, max = 200, message = "确认分行数必须在1-200之间"))]
    #[validate(nested)]
    pub lines: Vec<ProcurementConfirmationLineRequest>,
}

/// 保存采购确认分行的 W07 强类型命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SaveProcurementConfirmationLinesRequest {
    /// 当前采购确认待办 ID。
    #[validate(custom(function = "non_blank", message = "待办ID不能为空"))]
    #[validate(length(max = 128, message = "待办ID不能超过128个字符"))]
    pub work_item_id: String,
    /// 查询所得待办版本；Service 严格解析正整数字符串。
    #[validate(custom(function = "non_blank", message = "待办版本不能为空"))]
    #[validate(length(max = 20, message = "待办版本不能超过20个字符"))]
    pub expected_task_version: String,
    /// 待办冻结的不可变销售提交 ID。
    #[validate(custom(function = "non_blank", message = "提交版本不能为空"))]
    #[validate(length(max = 128, message = "提交版本不能超过128个字符"))]
    pub expected_subject_version: String,
    /// 本次非终结保存动作。
    #[validate(nested)]
    pub action: SaveProcurementConfirmationAction,
    /// 客户端稳定幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    #[validate(length(max = 128, message = "幂等键不能超过128个字符"))]
    pub idempotency_key: String,
}

/// 保存采购确认后的两个服务端版本。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SaveProcurementConfirmationResult {
    /// 新采购确认编辑版本。
    pub edit_version: u64,
    /// 新待办活动版本。
    pub task_version: String,
}

/// W07 唯一正式决定；分支只能由嵌套 `review_result` 表达。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "review_result",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum ProcurementConfirmationDecision {
    /// 通过并使销售生效。
    Approved {
        /// 路径必须指向的采购确认。
        confirmation_id: String,
        /// 当前采购确认冻结的销售提交。
        submission_id: String,
        /// 当前采购确认编辑版本。
        expected_confirmation_edit_version: u64,
    },
    /// 驳回并退回销售。
    Rejected {
        /// 路径必须指向的采购确认。
        confirmation_id: String,
        /// 当前采购确认冻结的销售提交。
        submission_id: String,
        /// 当前采购确认编辑版本。
        expected_confirmation_edit_version: u64,
        /// 驳回原因代码。
        reject_reason_code: entities::sales_review::ProcurementRejectReasonCode,
        /// 必填补充说明。
        comment: String,
    },
}

impl ProcurementConfirmationDecision {
    /// 返回采购确认 ID。
    pub(crate) fn confirmation_id(&self) -> &str {
        match self {
            Self::Approved { confirmation_id, .. } | Self::Rejected { confirmation_id, .. } => {
                confirmation_id
            }
        }
    }

    /// 返回冻结的销售提交 ID。
    pub(crate) fn submission_id(&self) -> &str {
        match self {
            Self::Approved { submission_id, .. } | Self::Rejected { submission_id, .. } => submission_id,
        }
    }

    /// 返回采购确认编辑版本。
    pub(crate) fn expected_confirmation_edit_version(&self) -> u64 {
        match self {
            Self::Approved {
                expected_confirmation_edit_version,
                ..
            }
            | Self::Rejected {
                expected_confirmation_edit_version,
                ..
            } => *expected_confirmation_edit_version,
        }
    }

    /// 校验两个正式决定分支的共享身份与分支专属字段。
    pub(crate) fn validate_branch(&self) -> Result<()> {
        if self.confirmation_id().trim().is_empty() || self.submission_id().trim().is_empty() {
            return Err(Error::ValidationError(
                "采购确认ID和销售提交ID不能为空".to_string(),
            ));
        }
        if self.expected_confirmation_edit_version() == 0 {
            return Err(Error::ValidationError("采购确认版本必须大于 0".to_string()));
        }
        if let Self::Rejected { comment, .. } = self {
            let count = comment.trim().chars().count();
            if count == 0 || count > 512 {
                return Err(Error::ValidationError(
                    "采购确认驳回说明不能为空且不能超过512个字符".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// 返回驳回分支字段；通过分支返回 `None`。
    pub(crate) fn rejection(&self) -> Option<(entities::sales_review::ProcurementRejectReasonCode, &str)> {
        match self {
            Self::Rejected {
                reject_reason_code,
                comment,
                ..
            } => Some((*reject_reason_code, comment)),
            Self::Approved { .. } => None,
        }
    }
}

/// W07 唯一采购确认完成命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CompleteProcurementConfirmationCommand {
    /// 当前采购确认待办 ID。
    #[validate(custom(function = "non_blank", message = "待办ID不能为空"))]
    pub work_item_id: String,
    /// 查询所得待办版本；Service 严格解析正整数字符串。
    #[validate(custom(function = "non_blank", message = "待办版本不能为空"))]
    #[validate(length(max = 20, message = "待办版本不能超过20个字符"))]
    pub expected_task_version: String,
    /// 待办冻结的不可变销售提交 ID。
    #[validate(custom(function = "non_blank", message = "提交版本不能为空"))]
    pub expected_subject_version: String,
    /// 唯一嵌套正式决定。
    pub decision: ProcurementConfirmationDecision,
    /// 客户端稳定幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    #[validate(length(max = 128, message = "幂等键不能超过128个字符"))]
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
    /// 当前操作人可见的正式任务；对象直接入口为空。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_item: Option<WorkItemView>,
    /// W07 领域动作，不得从通用任务动作推导。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_actions: Vec<ProcurementConfirmationAllowedAction>,
    /// W07 领域动作阻断事实。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub action_blockers: Vec<ProcurementConfirmationActionBlockerView>,
}

/// W07 详情查询参数。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProcurementConfirmationDetailParams {
    /// 从正式待办进入时必须携带的任务 ID。
    pub work_item_id: Option<String>,
}

/// W07 采购确认强类型领域动作。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProcurementConfirmationAllowedAction {
    /// 保存当前采购工作数据。
    Save,
    /// 通过采购确认。
    Approve,
    /// 驳回采购确认。
    Reject,
}

impl ProcurementConfirmationAllowedAction {
    /// 返回稳定动作代码。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Save => "SAVE",
            Self::Approve => "APPROVE",
            Self::Reject => "REJECT",
        }
    }
}

/// W07 单个领域动作的服务端阻断事实。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProcurementConfirmationActionBlockerView {
    /// 被阻断的 W07 动作。
    pub action: String,
    /// 稳定阻断码。
    pub code: String,
    /// 面向当前处理人的安全说明。
    pub message: String,
}

/// 采购推荐中的阻断或提醒。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProcurementRecommendationIssueView {
    /// 稳定问题代码。
    pub code: String,
    /// 面向采购人员的说明。
    pub message: String,
    /// 对应销售提交行；全单问题为空。
    pub sales_order_submission_line_id: Option<String>,
}

/// 推荐方案中的采购分配行。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProcurementRecommendationLineView {
    /// 建议确认行号。
    pub line_no: u32,
    /// 被覆盖的销售提交行。
    pub sales_order_submission_line_id: String,
    /// 销售项名称快照。
    pub item_name: String,
    /// 公司 SKU。
    pub sku_id: String,
    /// 推荐供应商。
    pub supplier_id: String,
    /// 供应商名称。
    pub supplier_name: String,
    /// 精确供给修订。
    pub supplier_offering_revision_id: String,
    /// 分配数量。
    pub confirmed_quantity: String,
    /// 对应履约方式的含税供给单价。
    pub latest_cost_gross: String,
    /// 进项税率。
    pub input_tax_rate: String,
    /// 以销售承诺日期作为采购确认目标日期。
    pub expected_delivery_date: String,
    /// 推荐履约方式。
    pub fulfillment_mode: entities::sales_review::types::FulfillmentMode,
    /// 精确供应商能力修订。
    pub supplier_capability_revision_id: String,
    /// 本分配的商品价、运费与服务费合计。
    pub landed_gross: String,
    /// 计入本分配的运费。
    pub freight_amount: Option<String>,
    /// 计入本分配的服务费。
    pub service_fee_amount: Option<String>,
    /// 推荐原因。
    pub recommendation_reason: String,
}

/// 推荐方案预计形成的采购单草稿分组。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProcurementRecommendationOrderView {
    /// 推荐供应商。
    pub supplier_id: String,
    /// 供应商名称。
    pub supplier_name: String,
    /// 履约方式。
    pub fulfillment_mode: entities::sales_review::types::FulfillmentMode,
    /// 该采购单草稿预计包含的确认分行数。
    pub line_count: u32,
    /// 该采购单草稿预计含税落地成本。
    pub estimated_gross: String,
}

/// 采购二次确认的后端最低可执行成本推荐。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProcurementRecommendationView {
    /// 采购确认批次。
    pub confirmation_id: String,
    /// 推荐算法及规则版本。
    pub policy_version: String,
    /// 计算时间（秒级时间戳）。
    pub calculated_at: u64,
    /// 当前供给能否完整覆盖全部销售提交行。
    pub ready: bool,
    /// 推荐采购分配行。
    pub lines: Vec<ProcurementRecommendationLineView>,
    /// 审批通过后预计生成的采购单草稿。
    pub purchase_orders: Vec<ProcurementRecommendationOrderView>,
    /// 预计采购含税落地成本。
    pub estimated_purchase_gross: String,
    /// 销售提交含税金额。
    pub sales_gross: String,
    /// 销售含税金额减预计采购含税落地成本。
    pub estimated_gross_margin: String,
    /// 阻断审批的问题。
    pub blocking_issues: Vec<ProcurementRecommendationIssueView>,
    /// 需要采购确认的提醒。
    pub warnings: Vec<ProcurementRecommendationIssueView>,
}

/// 驳回后销售可选择的固定受控出路。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProcurementSalesResolution {
    /// 修改商品或交易条件后重新提交。
    ResubmitChangedTerms,
    /// 申请低毛利受控承接。
    RequestLowMarginAcceptance,
    /// 驳回后作废销售单。
    VoidAfterRejection,
}

/// 采购确认的正式业务结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProcurementConfirmationBusinessResult {
    /// 采购确认通过且销售正式生效。
    ApprovedAndSalesEffective {
        /// 采购确认。
        procurement_confirmation_id: String,
        /// 销售单。
        sales_order_id: String,
        /// 不可变销售提交。
        submission_id: String,
        /// 新销售正式版本。
        sales_order_revision_id: String,
        /// 新应收往来账户。
        receivable_account_id: String,
        /// 可唯一消费的采购创建依据；当前稳定使用采购确认 ID。
        procurement_creation_basis_id: String,
    },
    /// 采购确认驳回并退回销售草稿。
    RejectedToSales {
        /// 采购确认。
        procurement_confirmation_id: String,
        /// 销售单。
        sales_order_id: String,
        /// 被驳回的不可变销售提交。
        rejected_submission_id: String,
        /// 本次追加式工作流动作。
        workflow_action_id: String,
        /// 固定销售后续出路；本事务不创建后继待办。
        next_sales_resolutions: [ProcurementSalesResolution; 3],
    },
}

/// W07 正式决定完成后的固定响应。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CompleteProcurementConfirmationResult {
    /// 已完成的原待办。
    pub work_item_id: String,
    /// 固定为 `COMPLETED`。
    pub work_item_status: entities::work_item::WorkItemStatus,
    /// 服务端重读或事务形成的正式业务结果。
    pub business_result: ProcurementConfirmationBusinessResult,
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
    /// 当前复核待办。
    #[validate(custom(function = "non_blank", message = "复核待办ID不能为空"))]
    pub work_item_id: String,
    /// 期望的待办乐观锁版本。
    #[validate(range(min = 1, message = "待办版本必须大于 0"))]
    pub expected_task_version: u64,
    /// 期望的不可变销售变更提交版本。
    #[validate(custom(function = "non_blank", message = "提交版本不能为空"))]
    pub expected_subject_version: String,
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

    #[test]
    fn w07_save_envelope_uses_nested_action_and_string_task_version() {
        let command: super::SaveProcurementConfirmationLinesRequest = serde_json::from_value(json!({
            "work_item_id": "wi-1",
            "expected_task_version": "7",
            "expected_subject_version": "submission-1",
            "action": {
                "confirmation_id": "pc-1",
                "submission_id": "submission-1",
                "expected_edit_version": 3,
                "lines": [{
                    "line_no": 1,
                    "sales_order_submission_line_id": "line-1",
                    "supplier_id": "supplier-1",
                    "supplier_offering_revision_id": "offering-revision-1",
                    "confirmed_quantity": "1.000000",
                    "latest_cost_gross": "10.0000",
                    "input_tax_rate": "0.130000",
                    "expected_delivery_date": "2026-08-31",
                    "fulfillment_mode": "WAREHOUSE",
                    "supplier_capability_revision_id": "capability-revision-1"
                }]
            },
            "idempotency_key": "save-1"
        }))
        .unwrap();

        assert_eq!(command.expected_task_version, "7");
        assert_eq!(command.action.confirmation_id, "pc-1");
        assert_eq!(
            entities::sales_review::types::FulfillmentMode::from(command.action.lines[0].fulfillment_mode),
            entities::sales_review::types::FulfillmentMode::CompanyWarehouse
        );
    }

    #[test]
    fn w07_decision_envelope_hard_cuts_rejection_reason_codes() {
        let current = json!({
            "work_item_id": "wi-1",
            "expected_task_version": "8",
            "expected_subject_version": "submission-1",
            "decision": {
                "review_result": "REJECTED",
                "confirmation_id": "pc-1",
                "submission_id": "submission-1",
                "expected_confirmation_edit_version": 4,
                "reject_reason_code": "DELIVERY_UNMET",
                "comment": "无法满足交期"
            },
            "idempotency_key": "decision-1"
        });
        let command: super::CompleteProcurementConfirmationCommand =
            serde_json::from_value(current.clone()).unwrap();
        assert_eq!(
            command.decision.rejection().map(|value| value.0),
            Some(entities::sales_review::ProcurementRejectReasonCode::DeliveryUnmet)
        );

        let mut legacy = current.clone();
        legacy["decision"]["reject_reason_code"] = json!("DELIVERY_NOT_MET");
        assert!(serde_json::from_value::<super::CompleteProcurementConfirmationCommand>(legacy).is_err());

        let mut redundant = current;
        redundant["decision"]["sales_order_id"] = json!("sales-1");
        assert!(serde_json::from_value::<super::CompleteProcurementConfirmationCommand>(redundant).is_err());
    }
}
