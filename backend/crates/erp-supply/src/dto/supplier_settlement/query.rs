//! 结算单/明细/差异查询参数、列表与详情视图。

use std::str::FromStr;

use application_core::{normalized_text, page_or_default, page_size_or_default};
use erp_core::common::time::BusinessDate;
use erp_core::ids::{SupplierAccountId, SupplierSettlementItemId, SupplierSettlementStatementId};
use erp_core::money::Amount;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::Result;
use crate::dto::supplier_fulfillment::{PageParams, normalize_sort};
use crate::entity::supplier_settlement::{
    SettlementDifferenceStatus, SettlementDifferenceType, SettlementReviewResult, SettlementStatus,
    SupplierSettlementStatement,
};

/// 结算单列表允许的排序字段白名单（Service 层校验，禁止任意字段透传）。
const STATEMENT_SORT_FIELDS: &[&str] = &["created_at", "period_start", "period_end", "confirmed_at"];
/// 结算明细列表允许的排序字段白名单。
const ITEM_SORT_FIELDS: &[&str] = &["created_at", "erp_calculated_amount", "supplier_billed_amount"];
/// 结算差异列表允许的排序字段白名单。
const DIFFERENCE_SORT_FIELDS: &[&str] = &["created_at", "difference_amount", "resolved_at"];

/// 供应商结算单列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct SupplierSettlementStatementListParams {
    /// 编号或供应商当前名称字面量关键词。
    #[validate(length(max = 200))]
    pub q: Option<String>,
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
    /// 编号或供应商当前名称字面量关键词。
    pub q: Option<String>,
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
            return Err(crate::Error::ValidationError("期间开始不得晚于期间结束".to_string()));
        }
        Ok(StatementListQuery {
            q: normalized_text(self.q.as_deref()),
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
                .map_err(|_| crate::Error::ValidationError(format!("{field}不是合法业务日期")))
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

/// 结算明细列表查询参数。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
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
    pub quantity: erp_core::money::Quantity,
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
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
pub type SettlementPageView<T> = crate::dto::supplier_fulfillment::PageView<T>;
