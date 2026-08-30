//! `receivable_account` 应收往来子账（数据模型 §6.8）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{CustomerAccountId, PartyId, ReceivableAccountId, SalesOrderId, SalesOrderRevisionId};
use crate::money::Amount;
use crate::sales_order::BusinessType;
use crate::validation::normalize_optional_text;

/// 复核证据引用最大长度。
const EVIDENCE_MAX_LEN: usize = 512;
/// 复核人标识最大长度。
const ACTOR_MAX_LEN: usize = 128;

/// 往来子账状态（数据模型 §6.8：未结、部分结清、已结清）。
///
/// 由开放余额派生：`open_total == 0` 为已结清，`settled_total == 0` 为未结，
/// 其余为部分结清；过账事务通过 [`ReceivableAccount::sync_totals`] 维护。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceivableAccountStatus {
    /// 未结。
    Open,
    /// 部分结清。
    PartiallySettled,
    /// 已结清。
    Settled,
}

impl ReceivableAccountStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Open => "未结",
            Self::PartiallySettled => "部分结清",
            Self::Settled => "已结清",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::PartiallySettled => "partially_settled",
            Self::Settled => "settled",
        }
    }
}

/// 卡券票款复核状态（数据模型 §6.8：不适用、卡券期初待复核、已复核、同步差额待复核）。
///
/// 仅为事务内同步的查询缓存，权威记录是 `receivable_funds_review` 复核链。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountReviewStatus {
    /// 不适用（非卡券期初应收）。
    NotApplicable,
    /// 卡券期初待复核。
    OpeningPending,
    /// 已复核。
    Reviewed,
    /// 同步差额待复核。
    SyncDeltaPending,
}

impl AccountReviewStatus {
    /// 按来源销售单业务性质返回新建应收账户的唯一合法初始状态。
    ///
    /// # 参数
    /// * `business_type` - 来源销售单创建后不可变的业务性质
    ///
    /// # 返回
    /// 卡券销售返回期初待复核，实物及服务销售返回不适用。
    pub fn initial_for_sales_business_type(business_type: BusinessType) -> Self {
        match business_type {
            BusinessType::Voucher => Self::OpeningPending,
            BusinessType::GoodsService => Self::NotApplicable,
        }
    }

    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::NotApplicable => "不适用",
            Self::OpeningPending => "卡券期初待复核",
            Self::Reviewed => "已复核",
            Self::SyncDeltaPending => "同步差额待复核",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotApplicable => "not_applicable",
            Self::OpeningPending => "opening_pending",
            Self::Reviewed => "reviewed",
            Self::SyncDeltaPending => "sync_delta_pending",
        }
    }
}

/// 应收往来子账创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceivableAccountData {
    /// 来源销售单。
    pub sales_order_id: SalesOrderId,
    /// 往来子账序号（同一销售单内从 1 递增，更换结算主体时以新序号另建子账）。
    pub account_seq: u32,
    /// 企业客户经营归属（不作为资金核销相等键）。
    pub customer_id: CustomerAccountId,
    /// 收款和开票往来主体（生效销售版本的 `settlement_party_id`）。
    pub counterparty_party_id: PartyId,
    /// 本子账开始适用的销售版本。
    pub source_sales_order_revision_id: SalesOrderRevisionId,
    /// 卡券票款复核状态缓存。
    pub review_status: AccountReviewStatus,
    /// 最近一次正式复核人。
    pub reviewed_by: Option<String>,
    /// 最近一次正式复核时间。
    pub reviewed_at: Option<Instant>,
    /// 最近一次正式复核证据引用。
    pub review_evidence_reference: Option<String>,
    /// 含税应收总额。
    pub gross_total: Amount,
    /// 已核销含税总额。
    pub settled_total: Amount,
    /// 可开票含税总额。
    pub invoiceable_total: Amount,
    /// 净已开含税总额。
    pub invoiced_total: Amount,
}

/// 应收往来子账更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ReceivableAccountUpdate {
    /// 复核状态缓存；`None` 表示不修改。
    pub review_status: Option<AccountReviewStatus>,
    /// 最近复核人；`None` 表示不修改，`Some("")` 清除（仅允许配合非已复核状态）。
    pub reviewed_by: Option<String>,
    /// 最近复核时间；复核三件套（人/时间/证据）必须整体设置或整体省略。
    pub reviewed_at: Option<Instant>,
    /// 最近复核证据引用；与 `reviewed_by` 成对出现。
    pub review_evidence_reference: Option<String>,
    /// 变更后的应收含税总额；只允许权威差额事实与它在同一事务内提交。
    pub gross_total: Option<Amount>,
    /// 变更后的可开票含税总额；与 `gross_total` 共同重算开放额度。
    pub invoiceable_total: Option<Amount>,
}

/// 应收往来子账实体（稳定主表，数据模型 §6.8）。
///
/// `sales_order_id + account_seq` 唯一；`open_total` / `open_invoiceable_total`
/// 为派生汇总，均不得为负；`status` 由开放余额派生。
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct ReceivableAccount {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<ReceivableAccountStatus>,
    /// 来源销售单。
    pub sales_order_id: SalesOrderId,
    /// 往来子账序号。
    pub account_seq: u32,
    /// 企业客户经营归属。
    pub customer_id: CustomerAccountId,
    /// 收款和开票往来主体。
    pub counterparty_party_id: PartyId,
    /// 本子账开始适用的销售版本。
    pub source_sales_order_revision_id: SalesOrderRevisionId,
    /// 卡券票款复核状态缓存。
    pub review_status: AccountReviewStatus,
    /// 最近一次正式复核人。
    pub reviewed_by: Option<String>,
    /// 最近一次正式复核时间。
    pub reviewed_at: Option<Instant>,
    /// 最近一次正式复核证据引用。
    pub review_evidence_reference: Option<String>,
    /// 含税应收总额。
    pub gross_total: Amount,
    /// 已核销含税总额。
    pub settled_total: Amount,
    /// 剩余开放含税余额（`gross_total - settled_total`）。
    pub open_total: Amount,
    /// 可开票含税总额。
    pub invoiceable_total: Amount,
    /// 净已开含税总额。
    pub invoiced_total: Amount,
    /// 剩余可开票含税额度（`invoiceable_total - invoiced_total`）。
    pub open_invoiceable_total: Amount,
}

impl PartialEq for ReceivableAccount {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.sales_order_id == other.sales_order_id
            && self.account_seq == other.account_seq
            && self.customer_id == other.customer_id
            && self.counterparty_party_id == other.counterparty_party_id
            && self.source_sales_order_revision_id == other.source_sales_order_revision_id
            && self.review_status == other.review_status
            && self.reviewed_by == other.reviewed_by
            && self.reviewed_at == other.reviewed_at
            && self.review_evidence_reference == other.review_evidence_reference
            && self.gross_total == other.gross_total
            && self.settled_total == other.settled_total
            && self.open_total == other.open_total
            && self.invoiceable_total == other.invoiceable_total
            && self.invoiced_total == other.invoiced_total
            && self.open_invoiceable_total == other.open_invoiceable_total
    }
}

impl Eq for ReceivableAccount {}

impl ReceivableAccount {
    /// 创建应收往来子账。
    ///
    /// 完成复核三件套（人/时间/证据）的一致性校验、金额非负与上限校验，并派生
    /// `open_total`、`open_invoiceable_total` 与 `status`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ReceivableAccountId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的子账实体。
    ///
    /// # 错误
    /// 当子账序号为 0、复核三件套与复核状态不一致、金额为负或已核销/已开票
    /// 超过对应总额时返回错误。
    pub fn new(
        id: ReceivableAccountId,
        data: ReceivableAccountData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let (reviewed_by, reviewed_at, evidence) = Self::validate_review(
            data.review_status,
            data.reviewed_by,
            data.reviewed_at,
            data.review_evidence_reference,
        )?;
        if data.account_seq == 0 {
            return Err(Error::from("往来子账序号必须从 1 开始"));
        }
        let (open_total, open_invoiceable) = validate_totals(
            data.gross_total,
            data.settled_total,
            data.invoiceable_total,
            data.invoiced_total,
        )?;
        let status = derive_status(open_total, data.settled_total);

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(status, created_by),
            sales_order_id: data.sales_order_id,
            account_seq: data.account_seq,
            customer_id: data.customer_id,
            counterparty_party_id: data.counterparty_party_id,
            source_sales_order_revision_id: data.source_sales_order_revision_id,
            review_status: data.review_status,
            reviewed_by,
            reviewed_at,
            review_evidence_reference: evidence,
            gross_total: data.gross_total,
            settled_total: data.settled_total,
            open_total,
            invoiceable_total: data.invoiceable_total,
            invoiced_total: data.invoiced_total,
            open_invoiceable_total: open_invoiceable,
        })
    }

    /// 更新子账复核缓存字段。
    ///
    /// 复核三件套（人/时间/证据）必须整体设置或整体省略；`reviewed_by` /
    /// `review_evidence_reference` 传入 `Some("")` 表示清除。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当复核三件套与复核状态不一致或文本超长时返回错误。
    pub fn update(&mut self, update: ReceivableAccountUpdate, updated_by: impl Into<String>) -> Result<()> {
        let review_status = update.review_status.unwrap_or(self.review_status);
        let gross_total = update.gross_total.unwrap_or(self.gross_total);
        let invoiceable_total = update.invoiceable_total.unwrap_or(self.invoiceable_total);
        let (open_total, open_invoiceable_total) = validate_totals(
            gross_total,
            self.settled_total,
            invoiceable_total,
            self.invoiced_total,
        )?;
        let (reviewed_by, reviewed_at, evidence) = if update.reviewed_by.is_none()
            && update.reviewed_at.is_none()
            && update.review_evidence_reference.is_none()
        {
            (
                self.reviewed_by.clone(),
                self.reviewed_at,
                self.review_evidence_reference.clone(),
            )
        } else {
            Self::validate_review(
                review_status,
                update.reviewed_by,
                update.reviewed_at,
                update.review_evidence_reference,
            )?
        };
        self.review_status = review_status;
        self.gross_total = gross_total;
        self.invoiceable_total = invoiceable_total;
        self.open_total = open_total;
        self.open_invoiceable_total = open_invoiceable_total;
        self.stable.status = derive_status(open_total, self.settled_total);
        self.reviewed_by = reviewed_by;
        self.reviewed_at = reviewed_at;
        self.review_evidence_reference = evidence;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 过账后同步汇总并派生状态。
    ///
    /// 由回款/发票分配过账事务在锁定子账后调用，重算 `open_total`、
    /// `open_invoiceable_total` 并派生 `status`（数据模型 §6.8「事务内同步汇总」）。
    ///
    /// # 参数
    /// * `settled_total` - 重算后的已核销含税总额
    /// * `invoiced_total` - 重算后的净已开含税总额
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当金额为负或已核销/已开票超过对应总额时返回错误。
    pub fn sync_totals(
        &mut self,
        settled_total: Amount,
        invoiced_total: Amount,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        let (open_total, open_invoiceable) = validate_totals(
            self.gross_total,
            settled_total,
            self.invoiceable_total,
            invoiced_total,
        )?;
        self.settled_total = settled_total;
        self.invoiced_total = invoiced_total;
        self.open_total = open_total;
        self.open_invoiceable_total = open_invoiceable;
        self.stable.status = derive_status(open_total, settled_total);
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 判断子账是否已结清。
    ///
    /// # 返回
    /// 开放余额为零时返回 `true`。
    pub fn is_settled(&self) -> bool {
        self.stable.status() == ReceivableAccountStatus::Settled
    }

    /// 校验复核三件套与复核状态的一致性。
    ///
    /// 规则（数据模型 §6.8）：`reviewed` 状态必须携带复核人/时间/证据三件套，
    /// 非 `reviewed` 状态不得携带；三件套内部必须成对出现。
    ///
    /// # 参数
    /// * `review_status` - 复核状态
    /// * `reviewed_by` - 复核人
    /// * `reviewed_at` - 复核时间
    /// * `evidence` - 复核证据引用
    ///
    /// # 返回
    /// 返回规范化后的复核三件套。
    ///
    /// # 错误
    /// 三件套不完整或与复核状态矛盾时返回错误。
    fn validate_review(
        review_status: AccountReviewStatus,
        reviewed_by: Option<String>,
        reviewed_at: Option<Instant>,
        evidence: Option<String>,
    ) -> Result<(Option<String>, Option<Instant>, Option<String>)> {
        let reviewed_by = normalize_optional_text(reviewed_by, "复核人", ACTOR_MAX_LEN)?;
        let evidence = normalize_optional_text(evidence, "复核证据引用", EVIDENCE_MAX_LEN)?;
        let trio_complete =
            reviewed_by.is_some() == reviewed_at.is_some() && reviewed_at.is_some() == evidence.is_some();
        if !trio_complete {
            return Err(Error::from("复核人、复核时间与复核证据必须同时提供或同时省略"));
        }
        match review_status {
            AccountReviewStatus::Reviewed if !trio_complete || reviewed_at.is_none() => {
                Err(Error::from("已复核状态必须携带复核人、复核时间与复核证据"))
            }
            AccountReviewStatus::NotApplicable
            | AccountReviewStatus::OpeningPending
            | AccountReviewStatus::SyncDeltaPending
                if trio_complete && reviewed_at.is_some() =>
            {
                Err(Error::from("未复核状态不得携带复核人、复核时间与复核证据"))
            }
            _ => Ok((reviewed_by, reviewed_at, evidence)),
        }
    }
}

/// 校验子账汇总四金额并派生开放余额。
///
/// # 参数
/// * `gross_total` - 含税总额
/// * `settled_total` - 已核销总额
/// * `invoiceable_total` - 可开票总额
/// * `invoiced_total` - 净已开总额
///
/// # 返回
/// 返回 `(open_total, open_invoiceable_total)`，均不得为负。
///
/// # 错误
/// 金额为负或已核销/已开票超过对应总额时返回错误。
fn validate_totals(
    gross_total: Amount,
    settled_total: Amount,
    invoiceable_total: Amount,
    invoiced_total: Amount,
) -> Result<(Amount, Amount)> {
    if gross_total.to_decimal().is_sign_negative()
        || settled_total.to_decimal().is_sign_negative()
        || invoiceable_total.to_decimal().is_sign_negative()
        || invoiced_total.to_decimal().is_sign_negative()
    {
        return Err(Error::from("子账汇总金额不得为负"));
    }
    if settled_total > gross_total {
        return Err(Error::from("已核销总额不得超过含税应收总额"));
    }
    if invoiced_total > invoiceable_total {
        return Err(Error::from("净已开金额不得超过可开票总额"));
    }
    Ok((
        gross_total.checked_sub(settled_total),
        invoiceable_total.checked_sub(invoiced_total),
    ))
}

/// 由开放余额派生子账状态。
///
/// # 参数
/// * `open_total` - 开放余额
/// * `settled_total` - 已核销总额
///
/// # 返回
/// 开放余额为零时为已结清，已核销为零时为未结，其余为部分结清。
fn derive_status(open_total: Amount, settled_total: Amount) -> ReceivableAccountStatus {
    if open_total.to_decimal().is_zero() {
        ReceivableAccountStatus::Settled
    } else if settled_total.to_decimal().is_zero() {
        ReceivableAccountStatus::Open
    } else {
        ReceivableAccountStatus::PartiallySettled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Amount;
    use std::str::FromStr;

    fn data() -> ReceivableAccountData {
        ReceivableAccountData {
            sales_order_id: SalesOrderId::new("so-1"),
            account_seq: 1,
            customer_id: CustomerAccountId::new("cust-1"),
            counterparty_party_id: PartyId::new("party-1"),
            source_sales_order_revision_id: SalesOrderRevisionId::new("so-1-r1"),
            review_status: AccountReviewStatus::NotApplicable,
            reviewed_by: None,
            reviewed_at: None,
            review_evidence_reference: None,
            gross_total: Amount::from_str("1000.00").unwrap(),
            settled_total: Amount::from_str("0.00").unwrap(),
            invoiceable_total: Amount::from_str("1000.00").unwrap(),
            invoiced_total: Amount::from_str("0.00").unwrap(),
        }
    }

    #[test]
    fn new_derives_open_totals_and_status() {
        let account = ReceivableAccount::new(ReceivableAccountId::new("ra-1"), data(), "admin-1").unwrap();

        assert_eq!(account.open_total, Amount::from_str("1000.00").unwrap());
        assert_eq!(
            account.open_invoiceable_total,
            Amount::from_str("1000.00").unwrap()
        );
        assert_eq!(account.stable.status(), ReceivableAccountStatus::Open);
        assert_eq!(account.stable.created_by, "admin-1");
        assert_eq!(account.stable.updated_by, "admin-1");
    }

    #[test]
    fn new_rejects_zero_seq_negative_and_over_settled() {
        let zero_seq = ReceivableAccountData {
            account_seq: 0,
            ..data()
        };
        assert!(ReceivableAccount::new(ReceivableAccountId::new("ra-2"), zero_seq, "admin").is_err());

        let negative = ReceivableAccountData {
            settled_total: Amount::from_str("-1.00").unwrap(),
            ..data()
        };
        assert!(ReceivableAccount::new(ReceivableAccountId::new("ra-3"), negative, "admin").is_err());

        let over_settled = ReceivableAccountData {
            settled_total: Amount::from_str("1001.00").unwrap(),
            ..data()
        };
        assert!(ReceivableAccount::new(ReceivableAccountId::new("ra-4"), over_settled, "admin").is_err());
    }

    #[test]
    fn new_enforces_review_trio_consistency() {
        let half_pair = ReceivableAccountData {
            review_status: AccountReviewStatus::Reviewed,
            reviewed_by: Some("  reviewer-1 ".to_string()),
            reviewed_at: Some(Instant::from_unix_secs(1_700_000_000)),
            review_evidence_reference: None,
            ..data()
        };
        assert!(ReceivableAccount::new(ReceivableAccountId::new("ra-5"), half_pair, "admin").is_err());

        let reviewed = ReceivableAccountData {
            review_status: AccountReviewStatus::Reviewed,
            reviewed_by: Some(" reviewer-1 ".to_string()),
            reviewed_at: Some(Instant::from_unix_secs(1_700_000_000)),
            review_evidence_reference: Some(" evid-1 ".to_string()),
            ..data()
        };
        let account = ReceivableAccount::new(ReceivableAccountId::new("ra-6"), reviewed, "admin").unwrap();
        assert_eq!(account.reviewed_by.as_deref(), Some("reviewer-1"));
        assert_eq!(account.review_evidence_reference.as_deref(), Some("evid-1"));

        let stale_cache = ReceivableAccountData {
            review_status: AccountReviewStatus::OpeningPending,
            reviewed_by: Some("reviewer-1".to_string()),
            reviewed_at: Some(Instant::from_unix_secs(1_700_000_000)),
            review_evidence_reference: Some("evid-1".to_string()),
            ..data()
        };
        assert!(ReceivableAccount::new(ReceivableAccountId::new("ra-7"), stale_cache, "admin").is_err());
    }

    #[test]
    fn sync_totals_recomputes_balance_and_status() {
        let mut account =
            ReceivableAccount::new(ReceivableAccountId::new("ra-1"), data(), "admin-1").unwrap();

        account
            .sync_totals(
                Amount::from_str("600.00").unwrap(),
                Amount::from_str("0.00").unwrap(),
                "system",
            )
            .unwrap();
        assert_eq!(account.stable.status(), ReceivableAccountStatus::PartiallySettled);
        assert_eq!(account.open_total, Amount::from_str("400.00").unwrap());

        account
            .sync_totals(
                Amount::from_str("1000.00").unwrap(),
                Amount::from_str("1000.00").unwrap(),
                "system",
            )
            .unwrap();
        assert_eq!(account.stable.status(), ReceivableAccountStatus::Settled);
        assert!(account.is_settled());
        assert_eq!(account.open_total, Amount::from_str("0.00").unwrap());
        assert_eq!(account.open_invoiceable_total, Amount::from_str("0.00").unwrap());

        assert!(account
            .sync_totals(
                Amount::from_str("1001.00").unwrap(),
                Amount::from_str("0.00").unwrap(),
                "system",
            )
            .is_err());
    }

    #[test]
    fn update_applies_review_cache_fields() {
        let mut account =
            ReceivableAccount::new(ReceivableAccountId::new("ra-1"), data(), "admin-1").unwrap();

        account
            .update(
                ReceivableAccountUpdate {
                    review_status: Some(AccountReviewStatus::Reviewed),
                    reviewed_by: Some(" reviewer-2 ".to_string()),
                    reviewed_at: Some(Instant::from_unix_secs(1_700_000_100)),
                    review_evidence_reference: Some(" evid-2 ".to_string()),
                    gross_total: None,
                    invoiceable_total: None,
                },
                "admin-2",
            )
            .unwrap();
        assert_eq!(account.review_status, AccountReviewStatus::Reviewed);
        assert_eq!(account.reviewed_by.as_deref(), Some("reviewer-2"));
        assert_eq!(account.review_evidence_reference.as_deref(), Some("evid-2"));
        assert_eq!(account.stable.updated_by, "admin-2");
        assert_eq!(account.stable.created_by, "admin-1", "touch 不修改创建人");

        assert!(account
            .update(
                ReceivableAccountUpdate {
                    review_status: Some(AccountReviewStatus::OpeningPending),
                    reviewed_by: Some("reviewer-3".to_string()),
                    reviewed_at: Some(Instant::from_unix_secs(1_700_000_200)),
                    review_evidence_reference: None,
                    gross_total: None,
                    invoiceable_total: None,
                },
                "admin-3",
            )
            .is_err());
    }

    #[test]
    fn enums_serialize_with_stable_codes_and_labels() {
        assert_eq!(
            serde_json::to_string(&ReceivableAccountStatus::PartiallySettled).unwrap(),
            "\"partially_settled\""
        );
        assert_eq!(
            serde_json::to_string(&AccountReviewStatus::SyncDeltaPending).unwrap(),
            "\"sync_delta_pending\""
        );
        assert_eq!(ReceivableAccountStatus::Open.label(), "未结");
        assert_eq!(AccountReviewStatus::Reviewed.label(), "已复核");
        assert_eq!(ReceivableAccountStatus::Settled.as_str(), "settled");
        assert_eq!(AccountReviewStatus::OpeningPending.as_str(), "opening_pending");
    }

    #[test]
    fn initial_review_status_is_derived_from_sales_business_type() {
        assert_eq!(
            AccountReviewStatus::initial_for_sales_business_type(BusinessType::Voucher),
            AccountReviewStatus::OpeningPending
        );
        assert_eq!(
            AccountReviewStatus::initial_for_sales_business_type(BusinessType::GoodsService),
            AccountReviewStatus::NotApplicable
        );
    }

    #[test]
    fn long_evidence_reference_is_rejected() {
        let long = ReceivableAccountData {
            review_status: AccountReviewStatus::Reviewed,
            reviewed_by: Some("reviewer-1".to_string()),
            reviewed_at: Some(Instant::from_unix_secs(1_700_000_000)),
            review_evidence_reference: Some("e".repeat(513)),
            ..data()
        };
        assert!(ReceivableAccount::new(ReceivableAccountId::new("ra-8"), long, "admin").is_err());
    }
}
