//! 域 D33 `supplier_settlement` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；金额使用 `entities::money`
//! 定点类型（serde_json 下自动字符串化）；业务日期使用 `BusinessDate`（`YYYY-MM-DD`）。

use entities::common::time::BusinessDate;
use entities::ids::{
    SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId, SupplierSettlementItemId,
    SupplierSettlementStatementId,
};
use entities::money::Amount;
use entities::supplier_settlement::{
    SettlementDifferenceStatus, SettlementDifferenceType, SettlementReviewRejectReason,
    SettlementReviewResult, SettlementStatus, SupplierSettlementSourceEvidence, SupplierSettlementStatement,
};
use entities::work_item::{WorkItemStatus, WorkItemType};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use validator::Validate;

use crate::errors::Result;
use crate::query::{normalized_text, page_or_default, page_size_or_default};
use crate::supplier_fulfillment::dto::{normalize_sort, PageParams};

/// 结算单列表允许的排序字段白名单（Service 层校验，禁止任意字段透传）。
const STATEMENT_SORT_FIELDS: &[&str] = &["created_at", "period_start", "period_end", "confirmed_at"];
/// 结算明细列表允许的排序字段白名单。
const ITEM_SORT_FIELDS: &[&str] = &["created_at", "erp_calculated_amount", "supplier_billed_amount"];
/// 结算差异列表允许的排序字段白名单。
const DIFFERENCE_SORT_FIELDS: &[&str] = &["created_at", "difference_amount", "resolved_at"];

/// 校验文本去除首尾空白后非空。
use crate::query::non_blank;

/// 校验需写入幂等收据的操作 ID 不含协议分隔符。
fn safe_command_id(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Ok(());
    }
    Err(validator::ValidationError::new("操作ID包含非法字符"))
}

/// 供应商结算单列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierSettlementStatementListParams {
    /// 结算单号模糊筛选（字面量、忽略大小写）。
    pub statement_no: Option<String>,
    /// 结算供应商筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 结算状态筛选。
    pub status: Option<SettlementStatus>,
    /// 结算期间开始下界（含）。
    pub period_from: Option<String>,
    /// 结算期间结束上界（含）。
    pub period_to: Option<String>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`period_start`/`period_end`/`confirmed_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的结算单列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StatementListQuery {
    /// 结算单号模糊筛选。
    pub statement_no: Option<String>,
    /// 结算供应商筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 结算状态筛选。
    pub status: Option<SettlementStatus>,
    /// 已校验的结算期间开始下界。
    pub period_from: Option<BusinessDate>,
    /// 已校验的结算期间结束上界。
    pub period_to: Option<BusinessDate>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SupplierSettlementStatementListParams {
    /// 归一化结算单列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<StatementListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, STATEMENT_SORT_FIELDS)?;
        let period_from = optional_business_date(self.period_from.as_deref(), "期间开始")?;
        let period_to = optional_business_date(self.period_to.as_deref(), "期间结束")?;
        if period_from.zip(period_to).is_some_and(|(from, to)| from > to) {
            return Err(crate::errors::Error::ValidationError(
                "期间开始不得晚于期间结束".to_string(),
            ));
        }
        Ok(StatementListQuery {
            statement_no: normalized_text(self.statement_no.as_deref()),
            supplier_id: self.supplier_id.clone(),
            status: self.status,
            period_from,
            period_to,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

fn optional_business_date(value: Option<&str>, field: &str) -> Result<Option<BusinessDate>> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            BusinessDate::from_str(value)
                .map_err(|_| crate::errors::Error::ValidationError(format!("{field}不是合法业务日期")))
        })
        .transpose()
}

/// 供应商结算单响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierSettlementStatementView {
    /// 实体主键。
    pub id: String,
    /// ERP 结算单号（创建幂等键）。
    pub statement_no: String,
    /// 结算供应商。
    pub supplier_id: String,
    /// 结算期间开始（含）。
    pub period_start: String,
    /// 结算期间结束（含）。
    pub period_end: String,
    /// 冻结的供应商结算期间策略。
    pub period_policy_id: String,
    /// 冻结的供应商结算期间策略版本。
    pub period_policy_version: String,
    /// 冻结的供应商结算期间策略时区。
    pub period_timezone: String,
    /// 供应商账单号。
    pub external_bill_no: Option<String>,
    /// 供应商账单版本。
    pub external_bill_version: Option<String>,
    /// ERP 金额。
    pub erp_amount: Amount,
    /// 供应商金额。
    pub supplier_amount: Amount,
    /// 双方金额差异（= 供应商金额 − ERP 金额）。
    pub difference_amount: Amount,
    /// 结算状态。
    pub status: SettlementStatus,
    /// 覆盖冻结来源、明细与差异结论的正式主题摘要。
    pub subject_hash: String,
    /// 正式来源事实水位（秒级时间戳）。
    pub source_as_of: i64,
    /// 来源快照冻结时间（秒级时间戳）。
    pub source_snapshot_at: i64,
    /// 不可变来源快照摘要。
    pub source_snapshot_hash: String,
    /// 提交复核采用的刷新截止策略。
    pub refresh_cutoff_policy_id: String,
    /// 刷新截止策略冻结版本。
    pub refresh_cutoff_policy_version: String,
    /// 经办人。
    pub prepared_by: String,
    /// 复核人。
    pub reviewed_by: Option<String>,
    /// 最近一次正式复核决定。
    pub review_result: Option<SettlementReviewResult>,
    /// 最近一次驳回原因代码。
    pub review_reason_code: Option<String>,
    /// 最近一次复核说明。
    pub review_comment: Option<String>,
    /// 最近一次正式复核决定时间（秒级时间戳）。
    pub reviewed_at: Option<i64>,
    /// 确认时间（秒级时间戳）。
    pub confirmed_at: Option<i64>,
    /// 确认后形成的应付账户。
    pub payable_account_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<SupplierSettlementStatement> for SupplierSettlementStatementView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `statement` - 结算单实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(statement: SupplierSettlementStatement) -> Self {
        Self {
            id: statement.base.id,
            statement_no: statement.statement_no,
            supplier_id: statement.supplier_id.to_string(),
            period_start: statement.period_start.to_string(),
            period_end: statement.period_end.to_string(),
            period_policy_id: statement.period_policy_id,
            period_policy_version: statement.period_policy_version,
            period_timezone: statement.period_timezone,
            external_bill_no: statement.external_bill_no,
            external_bill_version: statement.external_bill_version,
            erp_amount: statement.erp_amount,
            supplier_amount: statement.supplier_amount,
            difference_amount: statement.difference_amount,
            status: statement.status,
            subject_hash: statement.subject_hash,
            source_as_of: statement.source_as_of.unix_secs(),
            source_snapshot_at: statement.source_snapshot_at.unix_secs(),
            source_snapshot_hash: statement.source_snapshot_hash,
            refresh_cutoff_policy_id: statement.refresh_cutoff_policy_id,
            refresh_cutoff_policy_version: statement.refresh_cutoff_policy_version,
            prepared_by: statement.prepared_by,
            reviewed_by: statement.reviewed_by,
            review_result: statement.review_result,
            review_reason_code: statement.review_reason_code,
            review_comment: statement.review_comment,
            reviewed_at: statement.reviewed_at.map(|time| time.unix_secs()),
            confirmed_at: statement.confirmed_at.map(|t| t.unix_secs()),
            payable_account_id: statement.payable_account_id.map(|id| id.to_string()),
            version: statement.base.version,
            created_at: statement.base.created_at,
        }
    }
}

/// 录入来源证据时由客户端提供、并由服务端逐行校验与补全的行。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RecordSettlementSourceEvidenceLineRequest {
    /// 供应商子订单。
    pub supplier_fulfillment_order_id: SupplierFulfillmentOrderId,
    /// 供应商履约明细。
    pub supplier_fulfillment_item_id: SupplierFulfillmentItemId,
    /// 取消发生时间；仅取消事实尚无可关联正式记录时携带。
    pub cancel_occurred_at: Option<i64>,
    /// 取消结果正式证据引用；与取消发生时间成对。
    pub cancel_evidence_reference_id: Option<String>,
    /// 费用及账单行补证引用；服务端会并入履约/退款正式引用。
    #[validate(length(min = 1, max = 20, message = "费用及账单行证据引用必须在1-20项之间"))]
    pub evidence_reference_ids: Vec<String>,
    /// 运费含税金额。
    pub freight_gross: Amount,
    /// 运费不含税金额。
    pub freight_net: Amount,
    /// 运费税额。
    pub freight_tax: Amount,
    /// 服务费含税金额。
    pub service_fee_gross: Amount,
    /// 服务费不含税金额。
    pub service_fee_net: Amount,
    /// 服务费税额。
    pub service_fee_tax: Amount,
    /// 供应商账单行含税金额。
    pub supplier_billed_gross: Amount,
    /// 供应商账单行不含税金额。
    pub supplier_billed_net: Amount,
    /// 供应商账单行税额。
    pub supplier_billed_tax: Amount,
}

/// 录入不可变结算来源证据批次的强命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RecordSettlementSourceEvidenceRequest {
    /// 稳定请求 ID；重复请求只返回原批次。
    #[validate(length(min = 1, max = 128, message = "请求ID长度必须在1-128之间"))]
    #[validate(custom(function = "safe_command_id", message = "请求ID格式非法"))]
    pub request_id: String,
    /// 幂等键；与请求 ID 一起纳入命令摘要。
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
    /// 结算供应商。
    pub supplier_id: SupplierAccountId,
    /// 结算期间开始（含）。
    pub period_start: String,
    /// 结算期间结束（含）。
    pub period_end: String,
    /// 供应商结算期间策略。
    #[validate(custom(function = "non_blank", message = "期间策略不能为空"))]
    pub period_policy_id: String,
    /// 供应商结算期间策略版本。
    #[validate(custom(function = "non_blank", message = "期间策略版本不能为空"))]
    pub period_policy_version: String,
    /// 期间策略时区；当前只接受 `Asia/Shanghai`。
    #[validate(custom(function = "non_blank", message = "期间策略时区不能为空"))]
    pub timezone: String,
    /// 同范围单调递增来源版本。
    #[validate(range(min = 1, message = "来源版本必须大于0"))]
    pub source_version: u64,
    /// 外部账单号。
    #[validate(custom(function = "non_blank", message = "外部账单号不能为空"))]
    pub external_bill_no: String,
    /// 外部账单版本。
    #[validate(custom(function = "non_blank", message = "外部账单版本不能为空"))]
    pub external_bill_version: String,
    /// 外部账单头正式证据引用。
    #[validate(custom(function = "non_blank", message = "外部账单证据引用不能为空"))]
    pub external_bill_evidence_reference_id: String,
    /// 逐行补证输入；订单和退款金额由服务端派生。
    #[validate(length(min = 1, max = 1000, message = "来源证据行数必须在1-1000之间"))]
    #[validate(nested)]
    pub lines: Vec<RecordSettlementSourceEvidenceLineRequest>,
}

/// 来源证据批次响应。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierSettlementSourceEvidenceView {
    pub id: String,
    pub request_id: String,
    pub supplier_id: String,
    pub period_start: String,
    pub period_end: String,
    pub period_policy_id: String,
    pub period_policy_version: String,
    pub timezone: String,
    pub source_version: u64,
    pub external_bill_no: String,
    pub external_bill_version: String,
    pub source_as_of: i64,
    pub source_hash: String,
    pub line_count: usize,
}

/// 创建结算草稿前查询最新来源证据的服务端预检参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierSettlementSourceEvidenceQuery {
    pub supplier_id: SupplierAccountId,
    #[validate(custom(function = "non_blank", message = "期间开始不能为空"))]
    pub period_start: String,
    #[validate(custom(function = "non_blank", message = "期间结束不能为空"))]
    pub period_end: String,
}

impl From<SupplierSettlementSourceEvidence> for SupplierSettlementSourceEvidenceView {
    fn from(value: SupplierSettlementSourceEvidence) -> Self {
        Self {
            id: value.base.id,
            request_id: value.request_id,
            supplier_id: value.supplier_id.to_string(),
            period_start: value.period_start.to_string(),
            period_end: value.period_end.to_string(),
            period_policy_id: value.period_policy_id,
            period_policy_version: value.period_policy_version,
            timezone: value.timezone,
            source_version: value.source_version,
            external_bill_no: value.external_bill_no,
            external_bill_version: value.external_bill_version,
            source_as_of: value.source_as_of.unix_secs(),
            source_hash: value.source_hash,
            line_count: value.lines.len(),
        }
    }
}

/// 草稿来源动作。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementDraftAction {
    Create,
    Refresh,
}

/// 供应商结算单创建请求；金额明细只能由服务端来源构建器生成。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSettlementStatementRequest {
    pub action: SettlementDraftAction,
    pub supplier_id: SupplierAccountId,
    pub period_start: String,
    pub period_end: String,
    #[validate(length(min = 1, max = 128, message = "请求ID长度必须在1-128之间"))]
    #[validate(custom(function = "safe_command_id", message = "请求ID格式非法"))]
    pub request_id: String,
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 刷新可变草稿试算请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RefreshSettlementStatementRequest {
    pub action: SettlementDraftAction,
    #[validate(length(min = 1, max = 128, message = "结算单ID长度必须在1-128之间"))]
    pub statement_id: String,
    #[validate(range(min = 1, message = "乐观锁版本必须大于0"))]
    pub expected_lock_version: u64,
    #[validate(length(equal = 64, message = "来源快照摘要必须为64位"))]
    pub expected_source_snapshot_hash: String,
    #[validate(length(min = 1, max = 128, message = "请求ID长度必须在1-128之间"))]
    #[validate(custom(function = "safe_command_id", message = "请求ID格式非法"))]
    pub request_id: String,
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 草稿创建或刷新结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettlementDraftCommandResult {
    pub result_status: String,
    pub message: String,
    pub request_id: String,
    pub statement: SupplierSettlementStatementView,
    pub item_count: usize,
    pub difference_count: usize,
}

/// 结算单对象级提交动作。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementObjectAction {
    /// 提交财务复核。
    SubmitReview,
}

/// 提交结算复核强类型请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SubmitSettlementReviewRequest {
    /// 固定对象级动作。
    pub action: SettlementObjectAction,
    /// 结算单 ID；必须与路径一致。
    #[validate(length(min = 1, max = 128, message = "结算单ID长度必须在1-128之间"))]
    pub statement_id: String,
    /// 查询所得结算单乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 查询所得服务端主题摘要。
    #[validate(length(equal = 64, message = "主题摘要必须为64位"))]
    pub subject_hash: String,
    /// 查询所得刷新截止策略 ID。
    #[validate(custom(function = "non_blank", message = "刷新截止策略不能为空"))]
    pub refresh_cutoff_policy_id: String,
    /// 查询所得刷新截止策略版本。
    #[validate(custom(function = "non_blank", message = "刷新截止策略版本不能为空"))]
    pub expected_refresh_cutoff_policy_version: String,
    /// 本次复核任务的明确责任人。
    #[validate(
        custom(function = "non_blank", message = "复核人不能为空"),
        length(max = 128, message = "复核人ID不能超过128个字符")
    )]
    pub reviewer_user_id: String,
    /// 客户端稳定操作 ID，用于结果查询关联。
    #[validate(length(min = 1, max = 64, message = "操作ID长度必须在1-64之间"))]
    #[validate(custom(function = "safe_command_id", message = "操作ID格式非法"))]
    pub operation_id: String,
    /// 正式命令幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
    /// 提交说明。
    #[validate(length(max = 512, message = "提交说明长度不能超过512"))]
    pub comment: Option<String>,
}

/// 提交复核结果状态。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementReviewSubmissionStatus {
    /// 结算主题与唯一复核任务已原子提交。
    Submitted,
}

/// 提交结算复核结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SubmitSettlementReviewResult {
    /// 固定结果状态。
    pub result_status: SettlementReviewSubmissionStatus,
    /// 面向用户的稳定结果说明。
    pub message: String,
    /// 原请求操作 ID。
    pub operation_id: String,
    /// 提交后的结算单投影。
    pub statement: SupplierSettlementStatementView,
    /// 同事务创建的正式复核任务。
    pub work_item_id: String,
}

/// 供应商结算复核动作。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementReviewAction {
    /// 驳回给经办人。
    Reject,
    /// 确认并形成应付与成本差额。
    Confirm,
}

/// 供应商结算复核决定载荷。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SettlementReviewDecisionData {
    /// 结算单 ID；必须与路径一致。
    #[validate(length(min = 1, max = 128, message = "结算单ID长度必须在1-128之间"))]
    pub statement_id: String,
    /// 查询所得结算单乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 固定强类型决定。
    pub action: SettlementReviewAction,
    /// 客户端稳定操作 ID。
    #[validate(length(min = 1, max = 64, message = "操作ID长度必须在1-64之间"))]
    #[validate(custom(function = "safe_command_id", message = "操作ID格式非法"))]
    pub operation_id: String,
    /// 驳回原因代码；驳回必填，确认必须为空。
    pub reason_code: Option<String>,
    /// 决定说明。
    #[validate(length(max = 512, message = "决定说明长度不能超过512"))]
    pub comment: Option<String>,
}

impl SettlementReviewDecisionData {
    /// 解析复核决定附带的原因代码并执行命令协议校验。
    ///
    /// 驳回必须携带可解析为 [`SettlementReviewRejectReason`] 的原因代码；
    /// 确认不得携带任何原因代码。线上传输字符串的规范化与 allowlist 由
    /// 领域值对象独占，本方法只做协议分支。
    ///
    /// # 参数
    /// 无显式参数（方法接收者为已反序列化的决定载荷）。
    ///
    /// # 返回
    /// 驳回时返回解析后的强类型原因；确认时返回 `None`。
    ///
    /// # 错误
    /// 驳回缺失、非法或未知原因，以及确认携带原因时返回错误。
    ///
    /// # 约束
    /// 不改变线上传输形态；原因语义以领域值对象三元集合为准。
    pub fn parsed_reject_reason(&self) -> Result<Option<SettlementReviewRejectReason>> {
        match self.action {
            SettlementReviewAction::Reject => {
                let raw = self.reason_code.as_deref().ok_or_else(|| {
                    crate::errors::Error::ValidationError("驳回必须携带原因代码".to_string())
                })?;
                Ok(Some(SettlementReviewRejectReason::parse(raw)?))
            }
            SettlementReviewAction::Confirm if self.reason_code.is_some() => Err(
                crate::errors::Error::ValidationError("确认结算不得携带驳回原因代码".to_string()),
            ),
            SettlementReviewAction::Confirm => Ok(None),
        }
    }
}

/// 供应商结算复核唯一强类型命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SettlementReviewCommand {
    /// 当前正式复核任务。
    #[validate(length(min = 1, max = 128, message = "待办ID长度必须在1-128之间"))]
    pub work_item_id: String,
    /// 查询所得任务版本（字符串用于跨端稳定表示）。
    #[validate(custom(function = "non_blank", message = "待办版本不能为空"))]
    pub expected_task_version: String,
    /// 任务冻结的结算主题摘要。
    #[validate(length(equal = 64, message = "主题版本必须为64位"))]
    pub expected_subject_version: String,
    /// 强类型业务决定。
    #[validate(nested)]
    pub decision: SettlementReviewDecisionData,
    /// 正式命令幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 复核决定结果状态。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementReviewDecisionStatus {
    /// 已确认结算。
    Confirmed,
    /// 已驳回给经办人。
    Rejected,
}

/// 供应商结算正式复核结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettlementReviewDecisionResult {
    /// 固定结果状态。
    pub result_status: SettlementReviewDecisionStatus,
    /// 面向用户的稳定说明。
    pub message: String,
    /// 原请求操作 ID。
    pub operation_id: String,
    /// 决定后的结算单投影。
    pub statement: SupplierSettlementStatementView,
    /// 已完成的正式任务。
    pub work_item_id: String,
    /// 固定终态。
    pub work_item_status: WorkItemStatus,
    /// 任务完成后的版本。
    pub task_version: u64,
    /// 确认形成的应付编号；驳回为空。
    pub payable_no: Option<String>,
    /// 确认形成的应付账户；驳回为空。
    pub payable_account_id: Option<String>,
    /// 结算确认追加的含税成本差额；驳回为空。
    pub cost_delta_gross: Option<Amount>,
}

/// 结算单作废请求（乐观锁 + 原因）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct VoidSettlementRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 作废原因。
    #[validate(custom(function = "non_blank", message = "作废原因不能为空"))]
    pub reason: String,
}

/// 结算明细列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierSettlementItemListParams {
    /// 所属结算单筛选。
    pub statement_id: Option<SupplierSettlementStatementId>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`erp_calculated_amount`/`supplier_billed_amount`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的结算明细列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SettlementItemListQuery {
    /// 所属结算单筛选。
    pub statement_id: Option<SupplierSettlementStatementId>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SupplierSettlementItemListParams {
    /// 归一化结算明细列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SettlementItemListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, ITEM_SORT_FIELDS)?;
        Ok(SettlementItemListQuery {
            statement_id: self.statement_id.clone(),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 供应商结算明细响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierSettlementItemView {
    /// 实体主键。
    pub id: String,
    /// 所属结算单。
    pub statement_id: String,
    /// 供应商子订单。
    pub supplier_fulfillment_order_id: String,
    /// 供应商履约明细。
    pub supplier_fulfillment_item_id: String,
    /// 来源快照冻结数量。
    pub quantity: entities::money::Quantity,
    /// 订单结算金额。
    pub order_amount: Amount,
    /// 运费金额。
    pub freight_amount: Amount,
    /// 服务费金额。
    pub service_fee_amount: Amount,
    /// 供应商退款金额。
    pub refund_amount: Amount,
    /// ERP 计算含税金额（= 订单 + 运费 + 服务费 − 退款）。
    pub erp_calculated_amount: Amount,
    /// ERP 计算不含税金额。
    pub erp_calculated_net_amount: Amount,
    /// ERP 计算税额。
    pub erp_calculated_tax_amount: Amount,
    /// 供应商账单含税金额。
    pub supplier_billed_amount: Amount,
    /// 供应商账单不含税金额。
    pub supplier_billed_net_amount: Amount,
    /// 供应商账单税额。
    pub supplier_billed_tax_amount: Amount,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 结算差异列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierSettlementDifferenceListParams {
    /// 所属结算明细筛选。
    pub statement_item_id: Option<SupplierSettlementItemId>,
    /// 差异状态筛选。
    pub status: Option<SettlementDifferenceStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`difference_amount`/`resolved_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的结算差异列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SettlementDifferenceListQuery {
    /// 所属结算明细筛选。
    pub statement_item_id: Option<SupplierSettlementItemId>,
    /// 差异状态筛选。
    pub status: Option<SettlementDifferenceStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SupplierSettlementDifferenceListParams {
    /// 归一化结算差异列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SettlementDifferenceListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, DIFFERENCE_SORT_FIELDS)?;
        Ok(SettlementDifferenceListQuery {
            statement_item_id: self.statement_item_id.clone(),
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

/// 供应商结算差异响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierSettlementDifferenceView {
    /// 实体主键。
    pub id: String,
    /// 所属结算明细。
    pub statement_item_id: String,
    /// 差异类型。
    pub difference_type: SettlementDifferenceType,
    /// 差异金额（有符号）。
    pub difference_amount: Amount,
    /// 差异状态。
    pub status: SettlementDifferenceStatus,
    /// 处理结果文本。
    pub resolution: Option<String>,
    /// 处理人。
    pub resolved_by: Option<String>,
    /// 处理时间（秒级时间戳）。
    pub resolved_at: Option<i64>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 当前差异已追加的正式补证记录。
    pub evidence: Vec<SettlementDifferenceEvidenceView>,
}

/// 差异补证强命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SettlementDifferenceEvidenceRequest {
    #[validate(length(min = 1, max = 128, message = "结算单ID长度必须在1-128之间"))]
    pub statement_id: String,
    #[validate(length(min = 1, max = 128, message = "差异ID长度必须在1-128之间"))]
    pub difference_id: String,
    #[validate(range(min = 1, message = "差异版本必须大于0"))]
    pub expected_difference_version: u64,
    #[validate(length(min = 1, max = 20, message = "证据引用必须在1-20项之间"))]
    pub evidence_reference_ids: Vec<String>,
    pub opinion_code: Option<String>,
    #[validate(length(max = 1024, message = "补证说明不能超过1024字"))]
    pub comment: Option<String>,
    #[validate(length(min = 1, max = 128, message = "请求ID长度必须在1-128之间"))]
    #[validate(custom(function = "safe_command_id", message = "请求ID格式非法"))]
    pub request_id: String,
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 差异补证视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettlementDifferenceEvidenceView {
    pub evidence_id: String,
    pub evidence_reference_ids: Vec<String>,
    pub opinion_code: Option<String>,
    pub comment: Option<String>,
    pub provided_by: String,
    pub provided_at: i64,
}

/// 差异补证命令结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettlementDifferenceEvidenceResult {
    pub result_status: String,
    pub message: String,
    pub request_id: String,
    pub statement_id: String,
    pub difference_id: String,
    pub evidence: SettlementDifferenceEvidenceView,
}

/// 结算差异正式处理结论。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementDifferenceResolution {
    /// 供应商接受 ERP 口径。
    SupplierAccepted,
    /// ERP 接受供应商口径。
    ErpAccepted,
    /// 已通过独立补偿事实处理。
    Compensated,
    /// 有证据证明无需金额调整并关闭。
    ClosedNoAdjustment,
}

/// 结算差异强类型决定请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SettlementDifferenceDecisionRequest {
    /// 所属结算单；必须与差异归属一致。
    #[validate(length(min = 1, max = 128, message = "结算单ID长度必须在1-128之间"))]
    pub statement_id: String,
    /// 差异 ID；必须与路径一致。
    #[validate(length(min = 1, max = 128, message = "差异ID长度必须在1-128之间"))]
    pub difference_id: String,
    /// 查询所得结算单版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_lock_version: u64,
    /// 查询所得差异版本。
    #[validate(range(min = 1, message = "差异版本必须大于 0"))]
    pub expected_difference_version: u64,
    /// 固定正式结论。
    pub resolution: SettlementDifferenceResolution,
    /// 受控原因代码。
    #[validate(custom(function = "non_blank", message = "原因代码不能为空"))]
    pub reason_code: String,
    /// 正式证据引用；补偿或无调整关闭至少一项。
    pub evidence_reference_ids: Vec<String>,
    /// 客户端稳定操作 ID。
    #[validate(length(min = 1, max = 64, message = "操作ID长度必须在1-64之间"))]
    #[validate(custom(function = "safe_command_id", message = "操作ID格式非法"))]
    pub operation_id: String,
    /// 正式命令幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 差异决定结果状态。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementDifferenceDecisionStatus {
    /// 差异正式结论已登记。
    Resolved,
}

/// 结算差异决定结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettlementDifferenceDecisionResult {
    /// 固定结果状态。
    pub result_status: SettlementDifferenceDecisionStatus,
    /// 面向用户的稳定说明。
    pub message: String,
    /// 原请求操作 ID。
    pub operation_id: String,
    /// 所属结算单。
    pub statement_id: String,
    /// 差异决定后推进的结算单版本。
    pub statement_lock_version: u64,
    /// 正式差异投影。
    pub difference: SupplierSettlementDifferenceView,
}

/// 结算复核任务处理状态。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementReviewProcessingState {
    /// 正式任务责任事实可用；具体动作仍按当前 actor 责任与资格裁剪。
    Ready,
    /// 正式任务缺失、重复或与冻结主题不一致，所有决定均阻断。
    ApprovalBlocked,
}

/// W27 领域动作阻断摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettlementReviewActionBlockerView {
    /// 被阻断的动作。
    pub action: String,
    /// 稳定阻断代码。
    pub code: String,
    /// 面向用户的说明。
    pub message: String,
}

/// 结算详情嵌入的当前正式复核任务。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettlementReviewWorkItemView {
    /// 正式任务 ID。
    pub work_item_id: String,
    /// 固定任务类型。
    pub work_item_type: WorkItemType,
    /// 任务 CAS 版本。
    pub task_version: u64,
    /// 冻结的结算主题摘要。
    pub subject_version: String,
    /// 任务状态。
    pub status: WorkItemStatus,
    /// 当前正式处理状态。
    pub processing_state: SettlementReviewProcessingState,
    /// 责任角色。
    pub owner_role: String,
    /// 责任组织。
    pub owner_organization_id: String,
    /// 当前个人责任人。
    pub owner_user_id: Option<String>,
    /// 当前 actor 的领域动作阻断。
    pub action_blockers: Vec<SettlementReviewActionBlockerView>,
}

/// 结算单详情视图（结算单 + 全部明细 + 全部差异 + actor-specific 复核责任）。
#[derive(Debug, Clone, Serialize)]
pub struct SupplierSettlementStatementDetailView {
    /// 结算单头。
    pub statement: SupplierSettlementStatementView,
    /// 结算明细。
    pub items: Vec<SupplierSettlementItemView>,
    /// 结算差异。
    pub differences: Vec<SupplierSettlementDifferenceView>,
    /// 服务端汇总统计，客户端不得自行猜测处理状态。
    pub stats: SettlementStatementStatsView,
    /// 结算对象当前处理态。
    pub processing_state: String,
    /// 当前正式复核任务；不存在时为空且动作保持关闭。
    pub review_work_item: Option<SettlementReviewWorkItemView>,
    /// 正式复核任务投影是否因缺失、重复或主题不一致而阻断。
    pub review_processing_state: SettlementReviewProcessingState,
    /// 任务缺失或责任事实异常时的 fail-closed 阻断。
    pub review_action_blockers: Vec<SettlementReviewActionBlockerView>,
    /// 当前 actor 可执行的结算对象动作。
    pub allowed_actions: Vec<String>,
    /// 当前 actor 的结算对象动作阻断。
    pub action_blockers: Vec<SettlementReviewActionBlockerView>,
}

/// 结算详情的服务端汇总统计。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettlementStatementStatsView {
    pub item_count: usize,
    pub difference_count: usize,
    pub pending_difference_count: usize,
    pub evidenced_difference_count: usize,
    pub order_amount: Amount,
    pub freight_amount: Amount,
    pub service_fee_amount: Amount,
    pub refund_amount: Amount,
    pub erp_amount: Amount,
    pub supplier_amount: Amount,
    pub difference_amount: Amount,
}

/// 供应商结算列表的跨页服务端统计。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettlementStatementListStatsView {
    pub pending_reconciliation_count: i64,
    pub has_difference_count: i64,
    pub pending_review_count: i64,
    pub confirmed_amount: Amount,
}

/// 供应商结算单列表结果；统计与行数据使用同一过滤口径。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierSettlementStatementListView {
    pub items: Vec<SupplierSettlementStatementView>,
    pub total: i64,
    pub page: u64,
    pub page_size: u32,
    pub stats: SettlementStatementListStatsView,
    pub processing_state: String,
}

/// 供应商结算单分页视图（复用 D32 的契约形状）。
pub type SettlementPageView<T> = crate::supplier_fulfillment::dto::PageView<T>;

#[cfg(test)]
mod tests {
    use super::{
        normalize_sort, SettlementDifferenceDecisionRequest, SettlementDifferenceResolution,
        SettlementReviewAction, SettlementReviewCommand, SettlementReviewDecisionData,
        SubmitSettlementReviewRequest, SupplierSettlementDifferenceListParams,
        SupplierSettlementItemListParams, SupplierSettlementStatementListParams,
    };
    use crate::supplier_fulfillment::dto::SortDir;
    use entities::supplier_settlement::SettlementStatus;
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" period_start ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "period_start"],
        )
        .unwrap();
        assert_eq!(field, "period_start");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn statement_list_params_normalize_paging_filters_and_sort_defaults() {
        let params = SupplierSettlementStatementListParams {
            statement_no: Some(" ST-2026 ".to_string()),
            supplier_id: None,
            status: Some(SettlementStatus::PendingReview),
            period_from: None,
            period_to: None,
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.statement_no.as_deref(), Some("ST-2026"));
        assert_eq!(query.status, Some(SettlementStatus::PendingReview));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params = SupplierSettlementItemListParams {
            statement_id: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());

        let params = SupplierSettlementDifferenceListParams {
            statement_item_id: None,
            status: None,
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("difference_amount".to_string()),
            sort_dir: Some("asc".to_string()),
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.paging.sort_by, "difference_amount");
        assert_eq!(query.paging.sort_dir, SortDir::Asc);
    }

    #[test]
    fn strong_commands_match_frozen_wire_and_reject_receipt_delimiters() {
        let submit: SubmitSettlementReviewRequest = serde_json::from_value(serde_json::json!({
            "action": "SUBMIT_REVIEW",
            "statement_id": "statement-1",
            "expected_lock_version": 1,
            "subject_hash": "a".repeat(64),
            "refresh_cutoff_policy_id": "supplier-settlement-review-cutoff",
            "expected_refresh_cutoff_policy_version": "1",
            "reviewer_user_id": "reviewer-1",
            "operation_id": "submit:1",
            "idempotency_key": "submit-key-1",
            "comment": "提交复核"
        }))
        .unwrap();
        assert!(submit.validate().is_ok());

        let review: SettlementReviewCommand = serde_json::from_value(serde_json::json!({
            "work_item_id": "work-item-1",
            "expected_task_version": "2",
            "expected_subject_version": "a".repeat(64),
            "decision": {
                "statement_id": "statement-1",
                "expected_lock_version": 2,
                "action": "REJECT",
                "operation_id": "review-1",
                "reason_code": "NEEDS_MORE_EVIDENCE",
                "comment": "证据不足"
            },
            "idempotency_key": "review-key-1"
        }))
        .unwrap();
        assert!(review.validate().is_ok());

        let mut invalid = review;
        invalid.decision.operation_id = "review|1".to_string();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn difference_decision_uses_fixed_resolution_codes() {
        let request = SettlementDifferenceDecisionRequest {
            statement_id: "statement-1".to_string(),
            difference_id: "difference-1".to_string(),
            expected_lock_version: 1,
            expected_difference_version: 1,
            resolution: SettlementDifferenceResolution::ClosedNoAdjustment,
            reason_code: "NO_BUSINESS_IMPACT".to_string(),
            evidence_reference_ids: vec!["evidence-1".to_string()],
            operation_id: "difference-1".to_string(),
            idempotency_key: "difference-key-1".to_string(),
        };
        assert!(request.validate().is_ok());
        assert_eq!(
            serde_json::to_value(request).unwrap()["resolution"],
            "CLOSED_NO_ADJUSTMENT"
        );

        let command = SettlementReviewCommand {
            work_item_id: "work-item-1".to_string(),
            expected_task_version: "1".to_string(),
            expected_subject_version: "b".repeat(64),
            decision: SettlementReviewDecisionData {
                statement_id: "statement-1".to_string(),
                expected_lock_version: 1,
                action: SettlementReviewAction::Confirm,
                operation_id: "confirm-1".to_string(),
                reason_code: None,
                comment: None,
            },
            idempotency_key: "confirm-key-1".to_string(),
        };
        assert_eq!(
            serde_json::to_value(command).unwrap()["decision"]["action"],
            "CONFIRM"
        );
    }

    #[test]
    fn review_reject_reason_parses_and_enforces_command_protocol() {
        use entities::supplier_settlement::SettlementReviewRejectReason;

        let decision = SettlementReviewDecisionData {
            statement_id: "statement-1".to_string(),
            expected_lock_version: 1,
            action: SettlementReviewAction::Reject,
            operation_id: "review-1".to_string(),
            reason_code: Some("  amount_mismatch ".to_string()),
            comment: None,
        };
        assert_eq!(
            decision.parsed_reject_reason().unwrap(),
            Some(SettlementReviewRejectReason::AmountMismatch)
        );

        for code in ["NEEDS_MORE_EVIDENCE", "AMOUNT_MISMATCH", "OTHER"] {
            let decision = SettlementReviewDecisionData {
                statement_id: "statement-1".to_string(),
                expected_lock_version: 1,
                action: SettlementReviewAction::Reject,
                operation_id: "review-1".to_string(),
                reason_code: Some(code.to_string()),
                comment: None,
            };
            let reason = decision.parsed_reject_reason().unwrap().unwrap();
            assert_eq!(reason.as_str(), code);
        }

        for bad in [
            None,
            Some("   ".to_string()),
            Some("A".repeat(65)),
            Some("AMOUNT_UNRESOLVED".to_string()),
            Some("NEEDS MORE".to_string()),
        ] {
            let decision = SettlementReviewDecisionData {
                statement_id: "statement-1".to_string(),
                expected_lock_version: 1,
                action: SettlementReviewAction::Reject,
                operation_id: "review-1".to_string(),
                reason_code: bad,
                comment: None,
            };
            assert!(decision.parsed_reject_reason().is_err());
        }

        let confirm_with_reason = SettlementReviewDecisionData {
            statement_id: "statement-1".to_string(),
            expected_lock_version: 1,
            action: SettlementReviewAction::Confirm,
            operation_id: "confirm-1".to_string(),
            reason_code: Some("OTHER".to_string()),
            comment: None,
        };
        assert!(confirm_with_reason.parsed_reject_reason().is_err());

        let confirm = SettlementReviewDecisionData {
            statement_id: "statement-1".to_string(),
            expected_lock_version: 1,
            action: SettlementReviewAction::Confirm,
            operation_id: "confirm-1".to_string(),
            reason_code: None,
            comment: None,
        };
        assert_eq!(confirm.parsed_reject_reason().unwrap(), None);
    }
}
