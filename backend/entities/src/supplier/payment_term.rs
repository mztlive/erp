//! 供应商采购付款条件的受控代码与结算方式映射。

use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};

/// 结算方式（§6.2：预付款、先用后付、现结等受控代码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementMode {
    /// 预付款。
    Prepayment,
    /// 先用后付。
    PayAfterUse,
    /// 现结。
    CashSettlement,
}

impl SettlementMode {
    /// 返回面向业务用户的中文标签。
    ///
    /// # 返回
    /// 返回结算方式中文名称。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Prepayment => "预付款",
            Self::PayAfterUse => "先用后付",
            Self::CashSettlement => "现结",
        }
    }

    /// 返回用于持久化和接口传输的稳定代码。
    ///
    /// # 返回
    /// 返回 snake_case 结算方式代码。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Prepayment => "prepayment",
            Self::PayAfterUse => "pay_after_use",
            Self::CashSettlement => "cash_settlement",
        }
    }
}

/// 可形成采购计划付款日的供应商付款条件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupplierPaymentTerm {
    /// 先款比例代码 100%，审批通过日形成计划付款日。
    Prepay100,
    /// 先款比例代码 50%，审批通过日形成计划付款日。
    Prepay50,
    /// 先款比例代码 30%，审批通过日形成计划付款日。
    Prepay30,
    /// 采购最终审批通过日形成计划付款日，不启用先款门禁。
    CashOnApproval,
    /// 最晚预计交付日后 15 天形成计划付款日。
    PostpayNet15,
    /// 最晚预计交付日后 30 天形成计划付款日。
    PostpayNet30,
}

impl SupplierPaymentTerm {
    /// 解析并规范化供应商付款条件。
    ///
    /// 接受存量 `NET-15`、`NET-30`、`PREPAY-30` 和中文现结/预付款别名；
    /// 新写入统一使用稳定代码。只有“先用后付”而没有账期时拒绝继续。
    ///
    /// # 参数
    /// * `raw` - 付款条件代码或兼容别名
    ///
    /// # 返回
    /// 返回可计算计划付款日的付款条件。
    ///
    /// # 错误
    /// 空值、合同自由文本或缺少具体账期的条件返回业务错误。
    pub fn parse(raw: &str) -> Result<Self> {
        let code = raw.trim().to_uppercase();
        match code.as_str() {
            "PREPAY_100" | "PREPAY-100" | "预付款" | "先款" => Ok(Self::Prepay100),
            "PREPAY_50" | "PREPAY-50" => Ok(Self::Prepay50),
            "PREPAY_30" | "PREPAY-30" => Ok(Self::Prepay30),
            "CASH_ON_APPROVAL" | "CASH-ON-APPROVAL" | "现结" => Ok(Self::CashOnApproval),
            "POSTPAY_NET15" | "POSTPAY-NET15" | "NET15" | "NET-15" => Ok(Self::PostpayNet15),
            "POSTPAY_NET30" | "POSTPAY-NET30" | "NET30" | "NET-30" => Ok(Self::PostpayNet30),
            _ => Err(Error::from(
                "付款条件缺少可计算规则，请在供应商资料中选择具体付款条件",
            )),
        }
    }

    /// 返回规范化后的稳定付款条件代码。
    ///
    /// # 返回
    /// 返回采购单、提交和供应商商务版本统一使用的代码。
    pub fn code(self) -> &'static str {
        match self {
            Self::Prepay100 => "PREPAY_100",
            Self::Prepay50 => "PREPAY_50",
            Self::Prepay30 => "PREPAY_30",
            Self::CashOnApproval => "CASH_ON_APPROVAL",
            Self::PostpayNet15 => "POSTPAY_NET15",
            Self::PostpayNet30 => "POSTPAY_NET30",
        }
    }

    /// 返回面向采购与财务用户的付款条件名称。
    ///
    /// # 返回
    /// 返回与计划付款日规则一致的中文标签。
    pub fn label(self) -> &'static str {
        match self {
            Self::Prepay100 => "先款 100%",
            Self::Prepay50 => "先款 50%",
            Self::Prepay30 => "先款 30%",
            Self::CashOnApproval => "现结（审批通过日）",
            Self::PostpayNet15 => "货到 15 天",
            Self::PostpayNet30 => "货到 30 天",
        }
    }

    /// 返回付款条件归属的结算方式。
    ///
    /// # 返回
    /// 返回必须与供应商商务版本一致的结算方式。
    pub fn settlement_mode(self) -> SettlementMode {
        match self {
            Self::Prepay100 | Self::Prepay50 | Self::Prepay30 => SettlementMode::Prepayment,
            Self::CashOnApproval => SettlementMode::CashSettlement,
            Self::PostpayNet15 | Self::PostpayNet30 => SettlementMode::PayAfterUse,
        }
    }

    /// 判断付款条件是否启用先款后货门禁。
    ///
    /// # 返回
    /// 预付比例条件返回 `true`，现结和后付条件返回 `false`。
    pub fn prepay_gate(self) -> bool {
        matches!(self, Self::Prepay100 | Self::Prepay50 | Self::Prepay30)
    }

    /// 返回以最晚预计交付日为基准的账期天数。
    ///
    /// # 返回
    /// 后付条件返回 15 或 30；审批日付款条件返回 `None`。
    pub fn days_after_delivery(self) -> Option<u64> {
        match self {
            Self::PostpayNet15 => Some(15),
            Self::PostpayNet30 => Some(30),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SettlementMode, SupplierPaymentTerm};

    #[test]
    fn payment_terms_normalize_legacy_aliases() {
        assert_eq!(
            SupplierPaymentTerm::parse(" NET-30 ").unwrap().code(),
            "POSTPAY_NET30"
        );
        assert_eq!(
            SupplierPaymentTerm::parse("现结").unwrap().code(),
            "CASH_ON_APPROVAL"
        );
        assert_eq!(SupplierPaymentTerm::parse("预付款").unwrap().code(), "PREPAY_100");
    }

    #[test]
    fn payment_terms_expose_settlement_and_due_contracts() {
        let prepay = SupplierPaymentTerm::parse("PREPAY_30").unwrap();
        assert_eq!(prepay.settlement_mode(), SettlementMode::Prepayment);
        assert!(prepay.prepay_gate());
        assert_eq!(prepay.days_after_delivery(), None);

        let postpay = SupplierPaymentTerm::parse("POSTPAY_NET15").unwrap();
        assert_eq!(postpay.settlement_mode(), SettlementMode::PayAfterUse);
        assert!(!postpay.prepay_gate());
        assert_eq!(postpay.days_after_delivery(), Some(15));
    }

    #[test]
    fn payment_terms_reject_ambiguous_or_free_text_values() {
        assert!(SupplierPaymentTerm::parse("先用后付").is_err());
        assert!(SupplierPaymentTerm::parse("CONTRACT").is_err());
        assert!(SupplierPaymentTerm::parse("默认付款条件").is_err());
        assert!(SupplierPaymentTerm::parse(" ").is_err());
    }
}
