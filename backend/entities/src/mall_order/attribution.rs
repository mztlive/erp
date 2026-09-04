//! 支付来源归属与汇总规则（INT-E03 领域所有权）。
//!
//! CARD/WECHAT 归属判定与全来源汇总只归属本模块；Service 只做已解析卡事实
//! 注入与视图映射，不复制归属分支。

use crate::mall_order::payment_source::{MallPaymentSource, PaymentSourceType};
use crate::mall_order::types::AttributionStatus;
use crate::money::Amount;

/// 全来源归属汇总（INT-E03 唯一汇总点）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttributionRollup {
    /// 已归集来源数。
    pub attributed: usize,
    /// 待归属来源数。
    pub pending: usize,
    /// 订单级归属状态。
    pub order_status: AttributionStatus,
}

impl AttributionRollup {
    /// 从支付来源集合汇总归属（保持输入计数，不去重）。
    ///
    /// # 用途
    /// 统一列表详情的订单级归属判定：全部已归集时为已归集，否则为待归属。
    ///
    /// # 参数
    /// * `sources` - 当前订单的支付来源集合
    ///
    /// # 返回
    /// 返回已归集/待归属计数与订单级状态；空集合视为已归集，与旧详情
    /// `any(pending) ? Pending : Attributed` 语义一致。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 关键约束
    /// 不访问 I/O；空集合映射为已归集，保持既有无来源订单的详情状态不变。
    pub fn from_sources(sources: &[MallPaymentSource]) -> Self {
        let attributed = sources
            .iter()
            .filter(|source| source.attribution_status == AttributionStatus::Attributed)
            .count();
        let pending = sources.len().saturating_sub(attributed);
        let order_status = if pending == 0 {
            AttributionStatus::Attributed
        } else {
            AttributionStatus::PendingAttribution
        };
        Self {
            attributed,
            pending,
            order_status,
        }
    }

    /// 判断订单级归属是否为已归集。
    ///
    /// # 参数
    /// * `self` - 归属汇总
    ///
    /// # 返回
    /// 全部来源已归集时返回 `true`。
    pub fn is_attributed(self) -> bool {
        self.order_status == AttributionStatus::Attributed
    }
}

impl MallPaymentSource {
    /// 按来源类型与已解析卡事实判定归属（INT-E03 唯一判定点）。
    ///
    /// # 用途
    /// CARD 有映射时已归集、无映射时待归属；WECHAT 恒为已归集。
    ///
    /// # 参数
    /// * `source_type` - 支付来源类型
    /// * `card_mapped` - CARD 来源是否已映射到卡实例；WECHAT 时忽略
    ///
    /// # 返回
    /// 返回归属状态。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 关键约束
    /// 不访问 I/O、不改变既有状态码与 unknown 语义；卡事实由 Service 注入。
    pub fn decide_attribution(source_type: PaymentSourceType, card_mapped: bool) -> AttributionStatus {
        match (source_type, card_mapped) {
            (PaymentSourceType::Card, true) => AttributionStatus::Attributed,
            (PaymentSourceType::Card, false) => AttributionStatus::PendingAttribution,
            (PaymentSourceType::Wechat, _) => AttributionStatus::Attributed,
        }
    }
}

/// 按来源序号统计金额与归属（列表行支付构成的领域输入）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceComposition {
    /// 卡券金额合计。
    pub card_amount: crate::money::Amount,
    /// 微信金额合计。
    pub wechat_amount: crate::money::Amount,
    /// 来源条数。
    pub source_count: usize,
}

impl SourceComposition {
    /// 从支付来源集合折叠金额构成（精确加法，不舍入）。
    ///
    /// # 用途
    /// 为列表行支付构成提供领域级金额折叠，避免 Service 手工按类型求和。
    ///
    /// # 参数
    /// * `sources` - 当前订单的支付来源集合
    ///
    /// # 返回
    /// 返回卡券/微信精确合计与来源条数。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 关键约束
    /// 只做展示输入的精确折叠，不改变金额精度语义。
    pub fn from_sources(sources: &[MallPaymentSource]) -> Self {
        let mut card_amount = Amount::zero();
        let mut wechat_amount = Amount::zero();
        for source in sources {
            match source.source_type {
                PaymentSourceType::Card => {
                    card_amount = card_amount.checked_add(source.amount);
                }
                PaymentSourceType::Wechat => {
                    wechat_amount = wechat_amount.checked_add(source.amount);
                }
            }
        }
        Self {
            card_amount,
            wechat_amount,
            source_count: sources.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AttributionRollup, MallPaymentSource, PaymentSourceType, SourceComposition};
    use crate::ids::{MallOrderId, MallPaymentSourceId};
    use crate::mall_order::payment_source::MallPaymentSourceData;
    use crate::mall_order::types::AttributionStatus;
    use crate::money::Amount;
    use std::str::FromStr;

    /// 构造指定归属的微信来源。
    ///
    /// # 参数
    /// * `id` - 来源主键
    /// * `status` - 归属状态
    ///
    /// # 返回
    /// 返回测试用支付来源。
    fn source(id: &str, status: AttributionStatus) -> MallPaymentSource {
        MallPaymentSource::new(
            MallPaymentSourceId::new(id),
            MallPaymentSourceData {
                mall_order_id: MallOrderId::new("order-1"),
                source_no: 1,
                source_type: PaymentSourceType::Wechat,
                amount: Amount::from_str("10.00").unwrap(),
                source_card_instance_ref: None,
                mall_card_instance_id: None,
                wechat_payment_ref: Some(format!("wx-{id}")),
                attribution_status: status,
            },
        )
        .unwrap()
    }

    /// 归属判定：CARD 已映射/未映射、WECHAT 恒已归集。
    #[test]
    fn decide_attribution_covers_card_mapped_unmapped_and_wechat() {
        assert_eq!(
            MallPaymentSource::decide_attribution(PaymentSourceType::Card, true),
            AttributionStatus::Attributed
        );
        assert_eq!(
            MallPaymentSource::decide_attribution(PaymentSourceType::Card, false),
            AttributionStatus::PendingAttribution
        );
        assert_eq!(
            MallPaymentSource::decide_attribution(PaymentSourceType::Wechat, false),
            AttributionStatus::Attributed
        );
        assert_eq!(
            MallPaymentSource::decide_attribution(PaymentSourceType::Wechat, true),
            AttributionStatus::Attributed
        );
    }

    /// 汇总：混合来源待归属、全部同归属已归集、空集合沿用旧详情已归集语义。
    #[test]
    fn rollup_covers_mixed_uniform_and_empty_sources() {
        let mixed = vec![
            source("s-1", AttributionStatus::Attributed),
            source("s-2", AttributionStatus::PendingAttribution),
        ];
        let rollup = AttributionRollup::from_sources(&mixed);
        assert_eq!(rollup.order_status, AttributionStatus::PendingAttribution);
        assert_eq!(rollup.attributed, 1);
        assert_eq!(rollup.pending, 1);
        assert!(!rollup.is_attributed());

        let uniform = vec![
            source("s-1", AttributionStatus::Attributed),
            source("s-2", AttributionStatus::Attributed),
        ];
        let rollup = AttributionRollup::from_sources(&uniform);
        assert_eq!(rollup.order_status, AttributionStatus::Attributed);
        assert!(rollup.is_attributed());

        let empty = AttributionRollup::from_sources(&[]);
        assert_eq!(empty.order_status, AttributionStatus::Attributed);
        assert!(empty.is_attributed());
    }

    /// 金额构成：卡券/微信精确折叠与条数统计。
    #[test]
    fn composition_folds_amounts_and_counts_by_type() {
        let sources = vec![
            source("s-1", AttributionStatus::Attributed),
            source("s-2", AttributionStatus::Attributed),
        ];
        let composition = SourceComposition::from_sources(&sources);
        assert_eq!(composition.wechat_amount, Amount::from_str("20.00").unwrap());
        assert_eq!(composition.card_amount, Amount::zero());
        assert_eq!(composition.source_count, 2);
    }
}
