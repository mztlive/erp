//! 销售变更应收差额 VO（SALES-E11）。
//!
//! 封装销售变更新版本含税总额与当前生效版本含税总额之间的签约差额方向
//! 与绝对金额。零差额表示无需记账，调用方不得产生分录；正差额为应收增加，
//! 负差额为应收减少。所有金额均为 `Amount`（2 位小数），由领域层保证精度。

use crate::errors::{Error, Result};
use crate::money::Amount;
use crate::receivable::receivable_account::{
    AccountReviewStatus, ReceivableAccount, ReceivableAccountUpdate,
};
use crate::receivable::EntryDirection;
use crate::sales_order::BusinessType;

/// 销售变更应收差额 VO。
///
/// 仅在新旧含税总额不一致时存在；`direction` 表达记账方向，`absolute_amount`
/// 为正数金额，供分录工厂使用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReceivableDelta {
    /// 记账方向。
    direction: EntryDirection,
    /// 绝对金额（恒为正）。
    absolute_amount: Amount,
}

impl ReceivableDelta {
    /// 由新旧含税总额推导应收差额。
    ///
    /// # 参数
    /// * `new_gross` - 新版本含税总额
    /// * `current_gross` - 当前生效版本含税总额
    ///
    /// # 返回
    /// 差额为零时返回 `Ok(None)`（无分录）；非零时返回 `Ok(Some(delta))`，
    /// 其中方向与绝对金额已封装。
    ///
    /// # 错误
    /// 绝对金额无法表示为合法 `Amount`（如小数位非法或溢出）时返回错误。
    ///
    /// # 关键业务约束
    /// 该方法不触及持久化或账户存在性；零差额不得下沉为方向与金额的伪 VO。
    pub fn try_from_gross(new_gross: Amount, current_gross: Amount) -> Result<Option<Self>> {
        let delta = new_gross.to_decimal() - current_gross.to_decimal();
        if delta.is_zero() {
            return Ok(None);
        }
        let direction = if delta.is_sign_positive() {
            EntryDirection::Increase
        } else {
            EntryDirection::Decrease
        };
        let absolute = delta.abs();
        let absolute_amount = Amount::try_from(absolute)
            .map_err(|_| Error::from(format!("差额绝对金额超出合法范围：{absolute}")))?;
        if absolute_amount.to_decimal().is_zero() {
            return Err(Error::from("差额绝对金额必须为正数"));
        }
        Ok(Some(Self {
            direction,
            absolute_amount,
        }))
    }

    /// 返回差额方向。
    ///
    /// # 返回
    /// `Increase` 表示应收增加，`Decrease` 表示应收冲减。
    pub fn direction(self) -> EntryDirection {
        self.direction
    }

    /// 返回差额绝对金额。
    ///
    /// # 返回
    /// 恒为正数 `Amount`。
    pub fn absolute_amount(self) -> Amount {
        self.absolute_amount
    }
}

impl AccountReviewStatus {
    /// 销售变更差额允许的复核状态迁移。
    ///
    /// # 参数
    /// * `self` - 当前复核状态（账户现有状态）
    /// * `business_type` - 销售单业务性质
    ///
    /// # 返回
    /// 下一复核状态：卡券已复核 -> 同步差额待复核；实物服务不适用 -> 仍为不适用。
    ///
    /// # 错误
    /// 卡券上一轮票款复核尚未完成，或非卡券账户复核状态非法时返回错误。
    ///
    /// # 关键业务约束
    /// 该方法仅判定状态机是否允许形成差额，不读取仓储或判断账户存在性；
    /// `OpeningPending` / `SyncDeltaPending` 的卡券账户不得再次形成差额，
    /// 实物服务账户只有 `NotApplicable` 允许。
    pub fn transition_for_sales_change_delta(&self, business_type: BusinessType) -> Result<Self> {
        match (business_type, *self) {
            (BusinessType::Voucher, AccountReviewStatus::Reviewed) => {
                Ok(AccountReviewStatus::SyncDeltaPending)
            }
            (BusinessType::Voucher, _) => Err(Error::from(
                "卡券上一轮票款复核尚未通过，不能生效新的销售变更差额",
            )),
            (BusinessType::GoodsService, AccountReviewStatus::NotApplicable) => {
                Ok(AccountReviewStatus::NotApplicable)
            }
            (BusinessType::GoodsService, _) => Err(Error::from(
                "非卡券应收账户的票款复核状态不合法，不能生效销售变更",
            )),
        }
    }
}

impl ReceivableAccount {
    /// 销售变更差额对应的复核缓存更新（不含总额）。
    ///
    /// # 参数
    /// * `business_type` - 销售单业务性质
    ///
    /// # 返回
    /// 仅含复核字段的 `ReceivableAccountUpdate`：卡券返回待复核迁移并清除上一轮
    /// 缓存（`reviewed_by=""` / `review_evidence_reference=""`），实物服务返回空更新。
    ///
    /// # 错误
    /// 状态不允许形成差额时返回错误。
    ///
    /// # 关键业务约束
    /// 不修改含税总额；调用方需另行注入 `gross_total` / `invoiceable_total`。
    pub fn sales_change_delta_review_update(
        &self,
        business_type: BusinessType,
    ) -> Result<ReceivableAccountUpdate> {
        let next = self
            .review_status
            .transition_for_sales_change_delta(business_type)?;
        match (business_type, next) {
            (BusinessType::Voucher, AccountReviewStatus::SyncDeltaPending) => Ok(ReceivableAccountUpdate {
                review_status: Some(AccountReviewStatus::SyncDeltaPending),
                reviewed_by: Some(String::new()),
                reviewed_at: None,
                review_evidence_reference: Some(String::new()),
                ..Default::default()
            }),
            (BusinessType::GoodsService, AccountReviewStatus::NotApplicable) => {
                Ok(ReceivableAccountUpdate::default())
            }
            _ => Err(Error::from("复核状态迁移结果与业务性质不一致")),
        }
    }

    /// 销售变更差额对应的完整账户更新（含新总额）。
    ///
    /// # 参数
    /// * `business_type` - 销售单业务性质
    /// * `new_gross` - 新版本含税总额（将同时写入 `gross_total` 与 `invoiceable_total`）
    ///
    /// # 返回
    /// 含复核迁移与新总额的 `ReceivableAccountUpdate`，可直接用于 `ReceivableAccount::update`。
    ///
    /// # 错误
    /// 复核状态不允许或新总额校验失败（由 `update` 完成）时返回错误。
    ///
    /// # 关键业务约束
    /// 该方法不判断账户存在性；仅对已加载账户计算确定性更新。
    pub fn sales_change_delta_update(
        &self,
        business_type: BusinessType,
        new_gross: Amount,
    ) -> Result<ReceivableAccountUpdate> {
        let mut update = self.sales_change_delta_review_update(business_type)?;
        update.gross_total = Some(new_gross);
        update.invoiceable_total = Some(new_gross);
        Ok(update)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn amt(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    #[test]
    fn delta_zero_returns_none() {
        let delta = ReceivableDelta::try_from_gross(amt("100.00"), amt("100.00")).unwrap();
        assert!(delta.is_none());
    }

    #[test]
    fn delta_positive_is_increase() {
        let delta = ReceivableDelta::try_from_gross(amt("150.00"), amt("100.00"))
            .unwrap()
            .unwrap();
        assert_eq!(delta.direction(), EntryDirection::Increase);
        assert_eq!(delta.absolute_amount(), amt("50.00"));
    }

    #[test]
    fn delta_negative_is_decrease() {
        let delta = ReceivableDelta::try_from_gross(amt("80.00"), amt("100.00"))
            .unwrap()
            .unwrap();
        assert_eq!(delta.direction(), EntryDirection::Decrease);
        assert_eq!(delta.absolute_amount(), amt("20.00"));
    }

    #[test]
    fn delta_small_precision_boundary() {
        let delta = ReceivableDelta::try_from_gross(amt("100.01"), amt("100.00"))
            .unwrap()
            .unwrap();
        assert_eq!(delta.absolute_amount(), amt("0.01"));
        assert_eq!(delta.direction(), EntryDirection::Increase);
    }

    #[test]
    fn delta_negative_small_precision() {
        let delta = ReceivableDelta::try_from_gross(amt("99.99"), amt("100.00"))
            .unwrap()
            .unwrap();
        assert_eq!(delta.absolute_amount(), amt("0.01"));
        assert_eq!(delta.direction(), EntryDirection::Decrease);
    }

    #[test]
    fn delta_zero_with_large_amounts() {
        let delta = ReceivableDelta::try_from_gross(amt("99999999.99"), amt("99999999.99")).unwrap();
        assert!(delta.is_none());
    }

    #[test]
    fn review_transition_voucher_reviewed_allows_sync_delta() {
        let next = AccountReviewStatus::Reviewed
            .transition_for_sales_change_delta(BusinessType::Voucher)
            .unwrap();
        assert_eq!(next, AccountReviewStatus::SyncDeltaPending);
    }

    #[test]
    fn review_transition_voucher_unreviewed_rejects() {
        assert!(AccountReviewStatus::OpeningPending
            .transition_for_sales_change_delta(BusinessType::Voucher)
            .is_err());
        assert!(AccountReviewStatus::SyncDeltaPending
            .transition_for_sales_change_delta(BusinessType::Voucher)
            .is_err());
        assert!(AccountReviewStatus::NotApplicable
            .transition_for_sales_change_delta(BusinessType::Voucher)
            .is_err());
    }

    #[test]
    fn review_transition_goods_service_not_applicable_keeps() {
        let next = AccountReviewStatus::NotApplicable
            .transition_for_sales_change_delta(BusinessType::GoodsService)
            .unwrap();
        assert_eq!(next, AccountReviewStatus::NotApplicable);
    }

    #[test]
    fn review_transition_goods_service_illegal_rejects() {
        assert!(AccountReviewStatus::Reviewed
            .transition_for_sales_change_delta(BusinessType::GoodsService)
            .is_err());
        assert!(AccountReviewStatus::OpeningPending
            .transition_for_sales_change_delta(BusinessType::GoodsService)
            .is_err());
        assert!(AccountReviewStatus::SyncDeltaPending
            .transition_for_sales_change_delta(BusinessType::GoodsService)
            .is_err());
    }

    #[test]
    fn account_delta_review_update_voucher_clears_cache() {
        let account = make_account(AccountReviewStatus::Reviewed);
        let update = account
            .sales_change_delta_review_update(BusinessType::Voucher)
            .unwrap();
        assert_eq!(update.review_status, Some(AccountReviewStatus::SyncDeltaPending));
        assert_eq!(update.reviewed_by.as_deref(), Some(""));
        assert!(update.reviewed_at.is_none());
        assert_eq!(update.review_evidence_reference.as_deref(), Some(""));
    }

    #[test]
    fn account_delta_review_update_goods_service_is_empty() {
        let account = make_account(AccountReviewStatus::NotApplicable);
        let update = account
            .sales_change_delta_review_update(BusinessType::GoodsService)
            .unwrap();
        assert_eq!(update, ReceivableAccountUpdate::default());
    }

    #[test]
    fn account_delta_update_sets_new_gross() {
        let account = make_account(AccountReviewStatus::Reviewed);
        let update = account
            .sales_change_delta_update(BusinessType::Voucher, amt("120.00"))
            .unwrap();
        assert_eq!(update.gross_total, Some(amt("120.00")));
        assert_eq!(update.invoiceable_total, Some(amt("120.00")));
        assert_eq!(update.review_status, Some(AccountReviewStatus::SyncDeltaPending));
    }

    #[test]
    fn account_delta_update_goods_service_sets_gross_without_review() {
        let account = make_account(AccountReviewStatus::NotApplicable);
        let update = account
            .sales_change_delta_update(BusinessType::GoodsService, amt("200.00"))
            .unwrap();
        assert_eq!(update.gross_total, Some(amt("200.00")));
        assert_eq!(update.invoiceable_total, Some(amt("200.00")));
        assert_eq!(update.review_status, None);
    }

    fn make_account(status: AccountReviewStatus) -> ReceivableAccount {
        use crate::common::time::Instant;
        use crate::ids::{
            CustomerAccountId, PartyId, ReceivableAccountId, SalesOrderId, SalesOrderRevisionId,
        };
        let reviewed = if status == AccountReviewStatus::Reviewed {
            Some("reviewer-1".to_string())
        } else {
            None
        };
        let reviewed_at = if status == AccountReviewStatus::Reviewed {
            Some(Instant::from_unix_secs(1_700_000_000))
        } else {
            None
        };
        let evidence = if status == AccountReviewStatus::Reviewed {
            Some("evid-1".to_string())
        } else {
            None
        };
        ReceivableAccount::new(
            ReceivableAccountId::new("ra-1"),
            crate::receivable::ReceivableAccountData {
                sales_order_id: SalesOrderId::new("so-1"),
                account_seq: 1,
                customer_id: CustomerAccountId::new("cust-1"),
                counterparty_party_id: PartyId::new("party-1"),
                source_sales_order_revision_id: SalesOrderRevisionId::new("so-1-r1"),
                review_status: status,
                reviewed_by: reviewed,
                reviewed_at,
                review_evidence_reference: evidence,
                gross_total: amt("100.00"),
                settled_total: amt("0.00"),
                invoiceable_total: amt("100.00"),
                invoiced_total: amt("0.00"),
            },
            "admin-1",
        )
        .unwrap()
    }
}
