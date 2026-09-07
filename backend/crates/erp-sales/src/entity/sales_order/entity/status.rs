use serde::{Deserialize, Serialize};

use erp_core::common::state::DocumentState;
use erp_core::money::Amount;

/// 商业主状态（数据模型 §7.1：仅 4 值，审核环节走 `review_status` 审核轨）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CommercialStatus {
    /// 草稿。
    Draft,
    /// 审核中（进入 `review_status` 审核轨）。
    PendingReview,
    /// 已生效（不可直接编辑，变化通过销售变更单）。
    Effective,
    /// 已作废。
    Voided,
}

impl CommercialStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::PendingReview => "审核中",
            Self::Effective => "已生效",
            Self::Voided => "已作废",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::PendingReview => "PENDING_REVIEW",
            Self::Effective => "EFFECTIVE",
            Self::Voided => "VOIDED",
        }
    }
}

impl DocumentState for CommercialStatus {
    /// 数据模型 §7.1：`DRAFT → PENDING_REVIEW`、`DRAFT → VOIDED`、
    /// `PENDING_REVIEW → DRAFT`（驳回回到草稿）、`PENDING_REVIEW → EFFECTIVE`；
    /// `EFFECTIVE`/`VOIDED` 为终态（生效后变化走销售变更单）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::PendingReview, Self::Voided],
            Self::PendingReview => &[Self::Draft, Self::Effective],
            Self::Effective | Self::Voided => &[],
        }
    }
}

/// 审核轨状态（数据模型 §6.4：未提交、待采购确认、待低毛利上级确认、待销售领导、
/// 待运营、已通过、已驳回；§7.1/§7.3 审核环节值禁止写回主状态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewStatus {
    /// 未提交。
    NotSubmitted,
    /// 待采购确认。
    PendingProcurementConfirmation,
    /// 待低毛利上级确认。
    PendingLowMarginSuperior,
    /// 待销售领导。
    PendingSalesLeader,
    /// 待运营。
    PendingOperations,
    /// 已通过。
    Approved,
    /// 已驳回。
    Rejected,
    /// 审批中。
    InApproval,
}

impl ReviewStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::NotSubmitted => "未提交",
            Self::PendingProcurementConfirmation => "待采购确认",
            Self::PendingLowMarginSuperior => "待低毛利上级确认",
            Self::PendingSalesLeader => "待销售领导",
            Self::PendingOperations => "待运营",
            Self::Approved => "已通过",
            Self::Rejected => "已驳回",
            Self::InApproval => "审批中",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotSubmitted => "NOT_SUBMITTED",
            Self::PendingProcurementConfirmation => "PENDING_PROCUREMENT_CONFIRMATION",
            Self::PendingLowMarginSuperior => "PENDING_LOW_MARGIN_SUPERIOR",
            Self::PendingSalesLeader => "PENDING_SALES_LEADER",
            Self::PendingOperations => "PENDING_OPERATIONS",
            Self::Approved => "APPROVED",
            Self::Rejected => "REJECTED",
            Self::InApproval => "IN_APPROVAL",
        }
    }

    /// 判断当前审核轨是否应存在开放审批任务。
    ///
    /// # 返回
    /// 统一审批中或兼容历史待审阶段返回 `true`；未提交、已通过和已驳回返回 `false`。
    pub fn has_active_review_task(self) -> bool {
        !matches!(self, Self::NotSubmitted | Self::Approved | Self::Rejected)
    }
}

impl DocumentState for ReviewStatus {
    /// 审核轨固定邻接。
    ///
    /// `SalesOrder` 与 `VoucherSalesOrder` 目标路径均为
    /// `NOT_SUBMITTED → IN_APPROVAL → APPROVED`，撤回
    /// `IN_APPROVAL → NOT_SUBMITTED`。驳回不改业务状态。
    /// 旧逐节点复核态与 `REJECTED` 不得由新提交写入；邻接仅保留给未删除旧数据。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::NotSubmitted => &[
                Self::PendingProcurementConfirmation,
                Self::PendingSalesLeader,
                Self::InApproval,
            ],
            Self::InApproval => &[Self::Approved, Self::NotSubmitted],
            Self::PendingProcurementConfirmation => {
                &[Self::PendingLowMarginSuperior, Self::Approved, Self::Rejected]
            }
            Self::PendingLowMarginSuperior => &[Self::PendingProcurementConfirmation],
            Self::PendingSalesLeader => &[Self::PendingOperations, Self::Rejected],
            Self::PendingOperations => &[Self::Approved, Self::Rejected],
            Self::Approved => &[],
            Self::Rejected => &[Self::NotSubmitted],
        }
    }
}

/// 履约进度（数据模型 §6.4：未开始、部分履约、已完成；展示复合态不写回主状态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FulfillmentProgress {
    /// 未开始。
    NotStarted,
    /// 部分履约。
    PartiallyFulfilled,
    /// 已完成。
    Completed,
}

impl FulfillmentProgress {
    /// 返回进度的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::NotStarted => "未开始",
            Self::PartiallyFulfilled => "部分履约",
            Self::Completed => "已完成",
        }
    }

    /// 返回进度的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotStarted => "NOT_STARTED",
            Self::PartiallyFulfilled => "PARTIALLY_FULFILLED",
            Self::Completed => "COMPLETED",
        }
    }
}

/// 回款进度（数据模型 §6.4：未收、部分回款、已结清）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CollectionProgress {
    /// 未收。
    NotCollected,
    /// 部分回款。
    PartiallyCollected,
    /// 已结清。
    Settled,
}

impl CollectionProgress {
    /// 返回进度的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::NotCollected => "未收",
            Self::PartiallyCollected => "部分回款",
            Self::Settled => "已结清",
        }
    }

    /// 返回进度的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotCollected => "NOT_COLLECTED",
            Self::PartiallyCollected => "PARTIALLY_COLLECTED",
            Self::Settled => "SETTLED",
        }
    }

    /// 从应收子账开放余额与已结清金额派生回款进度。
    ///
    /// # 参数
    /// * `balances` - `(开放余额, 已结清金额)` 序列
    ///
    /// # 返回
    /// 无子账或尚无结清金额时返回 `NotCollected`；部分结清返回
    /// `PartiallyCollected`；全部子账开放余额清零且已有结清金额时返回 `Settled`。
    pub fn from_receivable_balances<I>(balances: I) -> Self
    where
        I: IntoIterator<Item = (Amount, Amount)>,
    {
        let mut has_account = false;
        let mut all_settled = true;
        let mut any_settled = false;
        for (open_total, settled_total) in balances {
            has_account = true;
            let settled = settled_total.to_decimal() > rust_decimal::Decimal::ZERO;
            all_settled &= open_total.to_decimal() == rust_decimal::Decimal::ZERO && settled;
            any_settled |= settled;
        }
        match (has_account, all_settled, any_settled) {
            (true, true, _) => Self::Settled,
            (_, _, true) => Self::PartiallyCollected,
            _ => Self::NotCollected,
        }
    }
}

/// 开票进度（数据模型 §6.4：未开、部分开票、已完成）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InvoiceProgress {
    /// 未开。
    NotInvoiced,
    /// 部分开票。
    PartiallyInvoiced,
    /// 已完成。
    Completed,
}

impl InvoiceProgress {
    /// 返回进度的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::NotInvoiced => "未开",
            Self::PartiallyInvoiced => "部分开票",
            Self::Completed => "已完成",
        }
    }

    /// 返回进度的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotInvoiced => "NOT_INVOICED",
            Self::PartiallyInvoiced => "PARTIALLY_INVOICED",
            Self::Completed => "COMPLETED",
        }
    }

    /// 从应收子账可开票余额与已开票金额派生开票进度。
    ///
    /// # 参数
    /// * `balances` - `(剩余可开票余额, 已开票金额)` 序列
    ///
    /// # 返回
    /// 无子账或尚无开票金额时返回 `NotInvoiced`；部分开票返回
    /// `PartiallyInvoiced`；全部子账可开票余额清零且已有开票金额时返回 `Completed`。
    pub fn from_receivable_balances<I>(balances: I) -> Self
    where
        I: IntoIterator<Item = (Amount, Amount)>,
    {
        let mut has_account = false;
        let mut all_invoiced = true;
        let mut any_invoiced = false;
        for (open_invoiceable_total, invoiced_total) in balances {
            has_account = true;
            let invoiced = invoiced_total.to_decimal() > rust_decimal::Decimal::ZERO;
            all_invoiced &= open_invoiceable_total.to_decimal() == rust_decimal::Decimal::ZERO && invoiced;
            any_invoiced |= invoiced;
        }
        match (has_account, all_invoiced, any_invoiced) {
            (true, true, _) => Self::Completed,
            (_, _, true) => Self::PartiallyInvoiced,
            _ => Self::NotInvoiced,
        }
    }
}

/// 关闭状态（数据模型 §6.4：未满足关闭、可关闭、已关闭；进入 `CLOSED` 只表示
/// `close_status` 置为关闭，主状态不保存该展示复合态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CloseStatus {
    /// 未满足关闭。
    NotSatisfied,
    /// 可关闭。
    Closeable,
    /// 已关闭。
    Closed,
}

impl CloseStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::NotSatisfied => "未满足关闭",
            Self::Closeable => "可关闭",
            Self::Closed => "已关闭",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotSatisfied => "NOT_SATISFIED",
            Self::Closeable => "CLOSEABLE",
            Self::Closed => "CLOSED",
        }
    }

    /// 从履约与回款进度派生关闭状态。
    ///
    /// # 参数
    /// * `fulfillment` - 销售单履约进度
    /// * `collection` - 销售单回款进度
    ///
    /// # 返回
    /// 履约完成且回款结清返回 `Closed`；仅满足其一返回 `Closeable`；均未满足返回
    /// `NotSatisfied`。开票进度不参与关闭判定。
    pub fn from_progress(fulfillment: FulfillmentProgress, collection: CollectionProgress) -> Self {
        match (
            fulfillment == FulfillmentProgress::Completed,
            collection == CollectionProgress::Settled,
        ) {
            (true, true) => Self::Closed,
            (true, false) | (false, true) => Self::Closeable,
            (false, false) => Self::NotSatisfied,
        }
    }
}

/// 稳定明细行状态（数据模型 §6.4：有效、被后续版本移除）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LineStatus {
    /// 有效。
    Active,
    /// 被后续版本移除。
    Removed,
}

impl LineStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "有效",
            Self::Removed => "已移除",
        }
    }
}

impl DocumentState for LineStatus {
    /// 有效行可被后续版本移除；移除行为终态（不复用历史行号，§6.4）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Active => &[Self::Removed],
            Self::Removed => &[],
        }
    }
}
