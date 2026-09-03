//! Finance WorkItem 的类型化 factory、identity 与 lifecycle contract（FIN-E06）。
//!
//! 应收（W11 销项开票执行、W13 卡券票款复核）与应付（W12 供应商付款执行）共用
//! 同一 contract，禁止两套规则。Service 只解析责任人/组织、调用 factory 并持久化，
//! 不得继续拼接关键身份字段（object/type/role/reason/key/summary/due）。
//!
//! ID 与时钟由 Service 显式注入：任务主键 `WorkItemId` 与活动时间 `Instant` 均为
//! 参数；本模块不生成 ID、不读取全局时钟、不访问 I/O。

use chrono::{FixedOffset, TimeZone};
use std::str::FromStr;

use crate::common::time::{BusinessDate, Instant};
use crate::errors::{Error, Result};
use crate::ids::WorkItemId;
use crate::money::Amount;
use crate::payable::{EntryDirection, PayableAccount, PayableEntry, PayableSourceType};
use crate::receivable::AccountReviewStatus;

use super::entity::{AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType};

/// 应收/应付财务任务的业务对象类型。
pub const RECEIVABLE_OBJECT_TYPE: &str = "receivable_account";
/// 应付财务任务的业务对象类型。
pub const PAYABLE_OBJECT_TYPE: &str = "payable_account";
/// 财务任务的责任角色。
pub const FINANCE_OWNER_ROLE: &str = "role-finance";

/// 销项开票任务产生原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SalesInvoiceTaskReason {
    /// 新形成且存在可开票额度。
    Initial,
    /// 红票恢复可开票额度后重开。
    ReopenedByRedInvoice,
    /// 销售变更调整可开票总额后重开。
    ReopenedBySalesChange,
}

impl SalesInvoiceTaskReason {
    /// 返回稳定的原因代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Initial => "RECEIVABLE_INVOICE_REQUIRED",
            Self::ReopenedByRedInvoice => "INVOICEABLE_REOPENED_BY_RED_INVOICE",
            Self::ReopenedBySalesChange => "INVOICEABLE_REOPENED_BY_SALES_CHANGE",
        }
    }
}

/// 卡券票款复核任务种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardFundsTaskKind {
    /// 首笔票款复核。
    Opening,
    /// 差额票款复核。
    Delta,
}

impl CardFundsTaskKind {
    /// 返回任务类型与稳定原因代码。
    pub fn spec(self) -> (WorkItemType, &'static str) {
        match self {
            Self::Opening => (WorkItemType::CardFundsReview, "CARD_FUNDS_OPENING_REVIEW"),
            Self::Delta => (WorkItemType::CardFundsDeltaReview, "CARD_FUNDS_DELTA_REVIEW"),
        }
    }
}

/// 供应商付款任务产生原因（W12，应付域复用同一 contract）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupplierPaymentTaskReason {
    /// 采购最终通过形成应付。
    Initial,
    /// 冲正重新产生余额后重开。
    ReopenedByReversal,
}

impl SupplierPaymentTaskReason {
    /// 返回稳定的原因代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Initial => "PAYABLE_PAYMENT_REQUIRED",
            Self::ReopenedByReversal => "PAYABLE_REOPENED_BY_REVERSAL",
        }
    }
}

/// 由账户复核状态解析票款任务种类（原 `review_task_spec` 唯一规则源）。
///
/// # 参数
/// * `status` - 应收子账票款复核状态缓存
///
/// # 返回
/// 待复核状态返回任务类型与原因；无需复核时返回 `None`，调用方不得建任务。
pub fn card_funds_task_kind(status: AccountReviewStatus) -> Option<(WorkItemType, &'static str)> {
    match status {
        AccountReviewStatus::OpeningPending => Some(CardFundsTaskKind::Opening.spec()),
        AccountReviewStatus::SyncDeltaPending => Some(CardFundsTaskKind::Delta.spec()),
        AccountReviewStatus::NotApplicable | AccountReviewStatus::Reviewed => None,
    }
}

/// 销项开票任务创建规格（Service 已解析责任人/组织后传入）。
pub struct SalesInvoiceTaskSpec {
    /// 应收子账主键。
    pub account_id: String,
    /// 子账乐观锁版本（冻结为任务 subject_version）。
    pub subject_version: String,
    /// 责任组织（往来主体）。
    pub owner_organization_id: String,
    /// 当前个人责任人。
    pub owner_user_id: String,
    /// 产生原因。
    pub reason: SalesInvoiceTaskReason,
    /// 剩余可开票含税额度（冻结为影响摘要）。
    pub open_invoiceable_total: Amount,
}

/// 卡券票款复核任务创建规格（Service 已解析责任人/组织后传入）。
pub struct CardFundsTaskSpec {
    /// 应收子账主键。
    pub account_id: String,
    /// 待复核销售版本。
    pub subject_version: String,
    /// 责任组织（往来主体）。
    pub owner_organization_id: String,
    /// 当前个人责任人。
    pub owner_user_id: String,
    /// 复核种类。
    pub kind: CardFundsTaskKind,
    /// 含税应收总额。
    pub gross_total: Amount,
    /// 已核销含税总额。
    pub settled_total: Amount,
    /// 净已开含税总额。
    pub invoiced_total: Amount,
}

/// 供应商付款任务创建规格（Service 已解析责任人/组织后传入，应付域复用）。
pub struct SupplierPaymentTaskSpec {
    /// 应付子账主键。
    pub account_id: String,
    /// 子账乐观锁版本（冻结为任务 subject_version）。
    pub subject_version: String,
    /// 责任组织（供应商往来主体）。
    pub owner_organization_id: String,
    /// 当前个人责任人。
    pub owner_user_id: String,
    /// 产生原因。
    pub reason: SupplierPaymentTaskReason,
    /// 计划付款时限（业务自然日对应的上海当日 23:59:59）。
    pub due_at: Instant,
    /// 剩余未付含税余额（冻结为影响摘要）。
    pub open_total: Amount,
}

/// 创建销项开票执行任务（W11 factory）。
///
/// # 参数
/// * `id` - 任务主键（Service 注入）
/// * `spec` - 已解析的任务规格
/// * `responsibility_key` - 服务端财务责任键（参与开放唯一性）
///
/// # 返回
/// 返回已冻结身份的开放任务。
///
/// # 错误
/// 责任键为空或字段超长时返回错误。
pub fn new_sales_invoice_task(
    id: WorkItemId,
    spec: SalesInvoiceTaskSpec,
    responsibility_key: String,
) -> Result<WorkItem> {
    WorkItem::new_with_responsibility_key(
        id,
        WorkItemData {
            work_item_type: WorkItemType::SalesInvoiceExecution,
            business_object_type: RECEIVABLE_OBJECT_TYPE.to_string(),
            business_object_id: spec.account_id,
            subject_version: spec.subject_version,
            owner_role: FINANCE_OWNER_ROLE.to_string(),
            owner_organization_id: spec.owner_organization_id,
            owner_user_id: spec.owner_user_id,
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: None,
            reason_code: Some(spec.reason.as_str().to_string()),
            impact_summary: Some(sales_invoice_impact_summary(spec.open_invoiceable_total)),
        },
        responsibility_key,
    )
}

/// 创建卡券票款复核任务（W13 factory）。
///
/// # 参数
/// * `id` - 任务主键（Service 注入）
/// * `spec` - 已解析的任务规格
/// * `responsibility_key` - 服务端财务责任键（参与开放唯一性）
///
/// # 返回
/// 返回已冻结身份的开放任务。
///
/// # 错误
/// 责任键为空或字段超长时返回错误。
pub fn new_card_funds_task(
    id: WorkItemId,
    spec: CardFundsTaskSpec,
    responsibility_key: String,
) -> Result<WorkItem> {
    let (work_item_type, reason_code) = spec.kind.spec();
    let priority = match spec.kind {
        CardFundsTaskKind::Opening | CardFundsTaskKind::Delta => WorkItemPriority::High,
    };
    WorkItem::new_with_responsibility_key(
        id,
        WorkItemData {
            work_item_type,
            business_object_type: RECEIVABLE_OBJECT_TYPE.to_string(),
            business_object_id: spec.account_id,
            subject_version: spec.subject_version,
            owner_role: FINANCE_OWNER_ROLE.to_string(),
            owner_organization_id: spec.owner_organization_id,
            owner_user_id: spec.owner_user_id,
            assignment_source: AssignmentSource::SystemRule,
            priority,
            due_at: None,
            reason_code: Some(reason_code.to_string()),
            impact_summary: Some(card_funds_impact_summary(
                spec.gross_total,
                spec.settled_total,
                spec.invoiced_total,
            )),
        },
        responsibility_key,
    )
}

/// 创建供应商付款执行任务（W12 factory，应付域复用同一 contract）。
///
/// # 参数
/// * `id` - 任务主键（Service 注入）
/// * `spec` - 已解析的任务规格
/// * `responsibility_key` - 服务端财务责任键（参与开放唯一性）
///
/// # 返回
/// 返回已冻结身份的开放任务。
///
/// # 错误
/// 责任键为空或字段超长时返回错误。
pub fn new_supplier_payment_task(
    id: WorkItemId,
    spec: SupplierPaymentTaskSpec,
    responsibility_key: String,
) -> Result<WorkItem> {
    WorkItem::new_with_responsibility_key(
        id,
        WorkItemData {
            work_item_type: WorkItemType::SupplierPaymentExecution,
            business_object_type: PAYABLE_OBJECT_TYPE.to_string(),
            business_object_id: spec.account_id,
            subject_version: spec.subject_version,
            owner_role: FINANCE_OWNER_ROLE.to_string(),
            owner_organization_id: spec.owner_organization_id,
            owner_user_id: spec.owner_user_id,
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: Some(spec.due_at),
            reason_code: Some(spec.reason.as_str().to_string()),
            impact_summary: Some(supplier_payment_impact_summary(spec.open_total)),
        },
        responsibility_key,
    )
}

/// 校验任务与指定应收子账的销项开票身份（W11 identity）。
///
/// 覆盖 object、task type、role、reason 与 business key；调用方另行比对
/// subject version 与 open balance 归零终态。
///
/// # 参数
/// * `task` - 待校验任务
/// * `account_id` - 应收子账主键
///
/// # 返回
/// 身份一致返回 `true`，任一维度不符返回 `false`。
pub fn matches_sales_invoice_identity(task: &WorkItem, account_id: &str) -> bool {
    task.work_item_type == WorkItemType::SalesInvoiceExecution
        && task.business_object_type == RECEIVABLE_OBJECT_TYPE
        && task.business_object_id == account_id
        && task.owner_role == FINANCE_OWNER_ROLE
        && task
            .responsibility_key()
            .is_some_and(|key| key.starts_with("finance:SALES_INVOICE:"))
        && matches!(
            task.reason_code.as_deref(),
            Some(
                "RECEIVABLE_INVOICE_REQUIRED"
                    | "INVOICEABLE_REOPENED_BY_RED_INVOICE"
                    | "INVOICEABLE_REOPENED_BY_SALES_CHANGE"
            )
        )
}

/// 校验任务与指定应收子账的票款复核身份（W13 identity）。
///
/// # 参数
/// * `task` - 待校验任务
/// * `account_id` - 应收子账主键
///
/// # 返回
/// 身份一致返回 `true`，任一维度不符返回 `false`。
pub fn matches_card_funds_identity(task: &WorkItem, account_id: &str) -> bool {
    matches!(
        task.work_item_type,
        WorkItemType::CardFundsReview | WorkItemType::CardFundsDeltaReview
    ) && task.business_object_type == RECEIVABLE_OBJECT_TYPE
        && task.business_object_id == account_id
        && task.owner_role == FINANCE_OWNER_ROLE
        && task
            .responsibility_key()
            .is_some_and(|key| key.starts_with("finance:CARD_FUNDS_REVIEW:"))
}

/// 校验采购应付与原始分录属于同一正式事实（原 Service ensure_purchase_payable 唯一规则源）。
///
/// 覆盖来源类型、子账与分录归属、增加方向、来源单据一致与未结清；
/// 调用方（Service）负责把 `false` 映射为稳定业务错误并解析责任人/组织。
///
/// # 参数
/// * `account` - 已在当前事务形成的采购应付子账
/// * `entry` - 与子账一同形成的原始应付分录
///
/// # 返回
/// 同一正式采购应付事实返回 `true`，任一维度不符返回 `false`。
pub fn is_purchase_payable(account: &PayableAccount, entry: &PayableEntry) -> bool {
    account.source_type == PayableSourceType::PurchaseOrder
        && entry.payable_account_id.as_ref() == account.base.id
        && entry.direction == EntryDirection::Increase
        && entry.source_document_id == account.source_document_id
        && !account.is_settled()
}

/// 校验任务与指定应付子账的付款执行身份（W12 identity，应付域复用）。
///
/// # 参数
/// * `task` - 待校验任务
/// * `account_id` - 应付子账主键
///
/// # 返回
/// 身份一致返回 `true`，任一维度不符返回 `false`。
pub fn matches_supplier_payment_identity(task: &WorkItem, account_id: &str) -> bool {
    task.work_item_type == WorkItemType::SupplierPaymentExecution
        && task.business_object_type == PAYABLE_OBJECT_TYPE
        && task.business_object_id == account_id
        && task.owner_role == FINANCE_OWNER_ROLE
        && task
            .responsibility_key()
            .is_some_and(|key| key.starts_with("finance:SUPPLIER_PAYMENT:"))
        && matches!(
            task.reason_code.as_deref(),
            Some("PAYABLE_PAYMENT_REQUIRED" | "PAYABLE_REOPENED_BY_REVERSAL")
        )
}

/// 返回随可开票额度变化的开票影响摘要（稳定编码，同一金额同一文本）。
pub fn sales_invoice_impact_summary(open_invoiceable_total: Amount) -> String {
    format!("待开票金额 ¥{open_invoiceable_total}，请登记销项发票并完成分配")
}

/// 返回票款复核影响摘要（稳定编码，同一三元组同一文本）。
pub fn card_funds_impact_summary(
    gross_total: Amount,
    settled_total: Amount,
    invoiced_total: Amount,
) -> String {
    format!("应收 ¥{gross_total}，已到账 ¥{settled_total}，已开票 ¥{invoiced_total}；请核对票款正式事实")
}

/// 返回随开放余额变化的付款影响摘要（稳定编码，应付域复用）。
pub fn supplier_payment_impact_summary(open_total: Amount) -> String {
    format!("未付金额 ¥{open_total}，请按付款条件登记付款")
}

/// 判断金额是否为零（open balance 归零即终态的唯一口径）。
pub fn is_zero_amount(amount: Amount) -> bool {
    amount == Amount::from_str("0.00").expect("静态零金额必须合法")
}

/// 把业务自然日转换为上海时区当日 23:59:59 的工作项时限。
///
/// # 参数
/// * `due_date` - 业务自然日
///
/// # 返回
/// 返回付款任务的稳定时限。
///
/// # 错误
/// 日期无法转换时返回错误。
pub fn payment_due_at(due_date: BusinessDate) -> Result<Instant> {
    let (year, month, day) = due_date.ymd();
    let timezone = FixedOffset::east_opt(8 * 3600).ok_or_else(|| Error::from("上海固定时差必须合法"))?;
    let due_at = timezone
        .with_ymd_and_hms(year, month, day, 23, 59, 59)
        .single()
        .ok_or_else(|| Error::from("计划付款日无法转换为工作项时限"))?;
    Ok(Instant::from_unix_secs(due_at.timestamp()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::WorkItemId;
    use crate::work_item::WorkItemStatus;
    use std::str::FromStr;

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn sales_spec() -> SalesInvoiceTaskSpec {
        SalesInvoiceTaskSpec {
            account_id: "ra-1".to_string(),
            subject_version: "7".to_string(),
            owner_organization_id: "party-1".to_string(),
            owner_user_id: "user-1".to_string(),
            reason: SalesInvoiceTaskReason::Initial,
            open_invoiceable_total: amount("100.00"),
        }
    }

    fn sales_task() -> WorkItem {
        new_sales_invoice_task(
            WorkItemId::new("wi-1"),
            sales_spec(),
            "finance:SALES_INVOICE:cust-1".to_string(),
        )
        .unwrap()
    }

    /// factory 冻结全部关键身份字段；同一业务事实生成稳定 key/summary/due。
    #[test]
    fn sales_invoice_factory_freezes_identity_and_stable_summary() {
        let first = sales_task();
        let second = sales_task();
        assert_eq!(first.work_item_type, WorkItemType::SalesInvoiceExecution);
        assert_eq!(first.business_object_type, RECEIVABLE_OBJECT_TYPE);
        assert_eq!(first.business_object_id, "ra-1");
        assert_eq!(first.owner_role, FINANCE_OWNER_ROLE);
        assert_eq!(first.reason_code.as_deref(), Some("RECEIVABLE_INVOICE_REQUIRED"));
        assert_eq!(
            first.responsibility_key(),
            second.responsibility_key(),
            "同一业务事实责任键稳定"
        );
        assert_eq!(first.impact_summary, second.impact_summary);
        assert_eq!(first.due_at, second.due_at);
        assert!(first.impact_summary.as_deref().unwrap().contains("100"));
    }

    /// 错误 object/task type/role/reason/business key 均不匹配。
    #[test]
    fn sales_invoice_identity_rejects_wrong_dimensions() {
        let task = sales_task();
        assert!(matches_sales_invoice_identity(&task, "ra-1"));
        assert!(
            !matches_sales_invoice_identity(&task, "ra-2"),
            "business key 不符"
        );
        let mut wrong_type = task.clone();
        wrong_type.work_item_type = WorkItemType::SupplierPaymentExecution;
        assert!(
            !matches_sales_invoice_identity(&wrong_type, "ra-1"),
            "task type 不符"
        );
        let mut wrong_object = task.clone();
        wrong_object.business_object_type = "payable_account".to_string();
        assert!(
            !matches_sales_invoice_identity(&wrong_object, "ra-1"),
            "object 不符"
        );
        let mut wrong_role = task.clone();
        wrong_role.owner_role = "role-other".to_string();
        assert!(!matches_sales_invoice_identity(&wrong_role, "ra-1"), "role 不符");
        let mut wrong_reason = task.clone();
        wrong_reason.reason_code = Some("UNKNOWN".to_string());
        assert!(
            !matches_sales_invoice_identity(&wrong_reason, "ra-1"),
            "reason 不符"
        );
        let wrong_key = WorkItem::new_with_responsibility_key(
            WorkItemId::new("wi-2"),
            WorkItemData {
                work_item_type: WorkItemType::SalesInvoiceExecution,
                business_object_type: RECEIVABLE_OBJECT_TYPE.to_string(),
                business_object_id: "ra-1".to_string(),
                subject_version: "7".to_string(),
                owner_role: FINANCE_OWNER_ROLE.to_string(),
                owner_organization_id: "party-1".to_string(),
                owner_user_id: "user-1".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: Some("RECEIVABLE_INVOICE_REQUIRED".to_string()),
                impact_summary: None,
            },
            "finance:OTHER:cust-1".to_string(),
        )
        .unwrap();
        assert!(
            !matches_sales_invoice_identity(&wrong_key, "ra-1"),
            "business key 前缀不符"
        );
    }

    /// 红票/销售变更重开原因同样通过身份校验。
    #[test]
    fn sales_invoice_reopened_reasons_match_identity() {
        for reason in [
            SalesInvoiceTaskReason::ReopenedByRedInvoice,
            SalesInvoiceTaskReason::ReopenedBySalesChange,
        ] {
            let task = new_sales_invoice_task(
                WorkItemId::new("wi-1"),
                SalesInvoiceTaskSpec {
                    reason,
                    ..sales_spec()
                },
                "finance:SALES_INVOICE:cust-1".to_string(),
            )
            .unwrap();
            assert!(matches_sales_invoice_identity(&task, "ra-1"));
        }
    }

    /// 票款任务种类映射：仅待复核状态建任务，稳定 key/summary。
    #[test]
    fn card_funds_kind_mapping_and_factory() {
        use crate::receivable::AccountReviewStatus;
        assert_eq!(
            card_funds_task_kind(AccountReviewStatus::OpeningPending),
            Some((WorkItemType::CardFundsReview, "CARD_FUNDS_OPENING_REVIEW"))
        );
        assert_eq!(
            card_funds_task_kind(AccountReviewStatus::SyncDeltaPending),
            Some((WorkItemType::CardFundsDeltaReview, "CARD_FUNDS_DELTA_REVIEW"))
        );
        assert!(card_funds_task_kind(AccountReviewStatus::NotApplicable).is_none());
        assert!(card_funds_task_kind(AccountReviewStatus::Reviewed).is_none());

        let task = new_card_funds_task(
            WorkItemId::new("wi-1"),
            CardFundsTaskSpec {
                account_id: "ra-1".to_string(),
                subject_version: "3".to_string(),
                owner_organization_id: "party-1".to_string(),
                owner_user_id: "user-1".to_string(),
                kind: CardFundsTaskKind::Opening,
                gross_total: amount("1000.00"),
                settled_total: amount("400.00"),
                invoiced_total: amount("100.00"),
            },
            "finance:CARD_FUNDS_REVIEW:cust-1".to_string(),
        )
        .unwrap();
        assert!(matches_card_funds_identity(&task, "ra-1"));
        assert!(!matches_card_funds_identity(&task, "ra-9"));
        assert_eq!(task.priority, WorkItemPriority::High);
        assert!(task.impact_summary.as_deref().unwrap().contains("1000"));
    }

    /// 付款 factory 冻结时限与摘要；错误组合不匹配。
    #[test]
    fn supplier_payment_factory_freezes_due_and_summary() {
        let due = payment_due_at(BusinessDate::from_ymd(2026, 8, 26).unwrap()).unwrap();
        assert_eq!(due.unix_secs(), 1_787_759_999);
        let task = new_supplier_payment_task(
            WorkItemId::new("wi-1"),
            SupplierPaymentTaskSpec {
                account_id: "pa-1".to_string(),
                subject_version: "5".to_string(),
                owner_organization_id: "party-9".to_string(),
                owner_user_id: "user-1".to_string(),
                reason: SupplierPaymentTaskReason::Initial,
                due_at: due,
                open_total: amount("200.00"),
            },
            "finance:SUPPLIER_PAYMENT:sup-1".to_string(),
        )
        .unwrap();
        assert!(matches_supplier_payment_identity(&task, "pa-1"));
        assert!(!matches_supplier_payment_identity(&task, "pa-2"));
        assert_eq!(task.due_at, Some(due));
        assert!(task.impact_summary.as_deref().unwrap().contains("200"));
    }

    /// 零金额即终态口径；非零不终结。
    #[test]
    fn zero_amount_is_terminal() {
        assert!(is_zero_amount(amount("0.00")));
        assert!(!is_zero_amount(amount("0.01")));
    }

    fn supplier_spec() -> SupplierPaymentTaskSpec {
        SupplierPaymentTaskSpec {
            account_id: "pa-1".to_string(),
            subject_version: "5".to_string(),
            owner_organization_id: "party-9".to_string(),
            owner_user_id: "user-1".to_string(),
            reason: SupplierPaymentTaskReason::Initial,
            due_at: payment_due_at(BusinessDate::from_ymd(2026, 8, 26).unwrap()).unwrap(),
            open_total: amount("200.00"),
        }
    }

    fn supplier_task() -> WorkItem {
        new_supplier_payment_task(
            WorkItemId::new("wi-1"),
            supplier_spec(),
            "finance:SUPPLIER_PAYMENT:sup-1".to_string(),
        )
        .unwrap()
    }

    /// 错误 object/task type/role/reason/business key 均不匹配；重开原因通过。
    #[test]
    fn supplier_payment_identity_rejects_wrong_dimensions() {
        let task = supplier_task();
        assert!(matches_supplier_payment_identity(&task, "pa-1"));
        assert!(
            !matches_supplier_payment_identity(&task, "pa-2"),
            "business key 不符"
        );
        let mut wrong_type = task.clone();
        wrong_type.work_item_type = WorkItemType::SalesInvoiceExecution;
        assert!(
            !matches_supplier_payment_identity(&wrong_type, "pa-1"),
            "task type 不符"
        );
        let mut wrong_object = task.clone();
        wrong_object.business_object_type = "receivable_account".to_string();
        assert!(
            !matches_supplier_payment_identity(&wrong_object, "pa-1"),
            "object 不符"
        );
        let mut wrong_role = task.clone();
        wrong_role.owner_role = "role-other".to_string();
        assert!(
            !matches_supplier_payment_identity(&wrong_role, "pa-1"),
            "role 不符"
        );
        let mut wrong_reason = task.clone();
        wrong_reason.reason_code = Some("UNKNOWN".to_string());
        assert!(
            !matches_supplier_payment_identity(&wrong_reason, "pa-1"),
            "reason 不符"
        );
        let reopened = new_supplier_payment_task(
            WorkItemId::new("wi-2"),
            SupplierPaymentTaskSpec {
                reason: SupplierPaymentTaskReason::ReopenedByReversal,
                ..supplier_spec()
            },
            "finance:SUPPLIER_PAYMENT:sup-1".to_string(),
        )
        .unwrap();
        assert_eq!(
            reopened.reason_code.as_deref(),
            Some("PAYABLE_REOPENED_BY_REVERSAL")
        );
        assert!(matches_supplier_payment_identity(&reopened, "pa-1"));
    }

    /// 同一业务事实生成稳定 key/summary/due；重复 ensure 幂等。
    #[test]
    fn supplier_payment_factory_is_stable_and_idempotent() {
        let first = supplier_task();
        let second = supplier_task();
        assert_eq!(first.responsibility_key(), second.responsibility_key());
        assert_eq!(first.impact_summary, second.impact_summary);
        assert_eq!(first.due_at, second.due_at);
        assert_eq!(first.subject_version, "5");
        assert_eq!(first.owner_role, FINANCE_OWNER_ROLE);
        assert_eq!(first.business_object_type, PAYABLE_OBJECT_TYPE);
    }

    /// 结清终态后不得重复完成或更新摘要；终态任务保持关闭。
    #[test]
    fn supplier_payment_terminal_state_is_closed() {
        let mut task = supplier_task();
        task.complete_when_payable_settled(Instant::from_unix_secs(100))
            .unwrap();
        assert_eq!(task.status, WorkItemStatus::Completed);
        assert!(task
            .complete_when_payable_settled(Instant::from_unix_secs(101))
            .is_err());
        assert!(task.update_impact_summary(Some("changed".to_string())).is_err());
    }

    /// 采购应付准入覆盖来源、分录归属、方向、单据一致与未结清。
    #[test]
    fn purchase_payable_admission_covers_all_dimensions() {
        use crate::ids::{PayableAccountId, SupplierAccountId};
        use crate::payable::{
            PayableAccount, PayableAccountData, PayableEntry, PayableEntryData, PayableEntryType,
        };
        fn account(source_type: crate::payable::PayableSourceType) -> PayableAccount {
            open_account(source_type, "0.00")
        }
        fn open_account(source_type: crate::payable::PayableSourceType, settled: &str) -> PayableAccount {
            PayableAccount::new(
                PayableAccountId::new("pa-1"),
                PayableAccountData {
                    source_document_id: "po-1".to_string(),
                    supplier_id: SupplierAccountId::new("sup-1"),
                    source_type,
                    gross_total: amount("100.00"),
                    settled_total: amount(settled),
                    invoiceable_total: amount("100.00"),
                    invoiced_total: amount("0.00"),
                },
                "tester",
            )
            .unwrap()
        }
        fn entry(direction: EntryDirection, document_id: &str) -> PayableEntry {
            use crate::ids::PayableEntryId;
            PayableEntry::new(
                PayableEntryId::new("entry-1"),
                PayableEntryData {
                    payable_account_id: PayableAccountId::new("pa-1"),
                    entry_type: PayableEntryType::Original,
                    direction,
                    amount: amount("100.00"),
                    due_date: BusinessDate::from_ymd(2026, 8, 26).unwrap(),
                    source_fact_type: "purchase_order".to_string(),
                    source_document_id: document_id.to_string(),
                    source_revision_id: "rev-1".to_string(),
                    source_sequence: 1,
                    posted_at: Instant::from_unix_secs(10),
                },
            )
            .unwrap()
        }
        let good = account(PayableSourceType::PurchaseOrder);
        assert!(is_purchase_payable(
            &good,
            &entry(EntryDirection::Increase, "po-1")
        ));
        assert!(
            !is_purchase_payable(
                &account(PayableSourceType::SupplierSettlement),
                &entry(EntryDirection::Increase, "po-1")
            ),
            "来源类型不符"
        );
        assert!(
            !is_purchase_payable(&good, &entry(EntryDirection::Increase, "po-2")),
            "单据不一致"
        );
        let settled = open_account(PayableSourceType::PurchaseOrder, "100.00");
        assert!(settled.is_settled());
        assert!(
            !is_purchase_payable(&settled, &entry(EntryDirection::Increase, "po-1")),
            "已结清不得建任务"
        );
    }
}
