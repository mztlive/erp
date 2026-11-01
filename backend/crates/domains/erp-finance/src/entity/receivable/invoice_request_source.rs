//! 开票申请的来源准入与购方资料校验；事实由组合层读取。
use erp_core::money::Amount;
use erp_core::{Error, Result};

use super::InvoiceRequestData;

/// 当前销售来源及应收结算主体的法定开票资料。
pub struct InvoiceRequestSource {
    pub effective: bool,
    pub invoice_title: String,
    pub tax_number: String,
}

impl InvoiceRequestSource {
    /// 返回当前不能申请开票的原因。
    /// # 参数
    /// * `available` - 扣除审批中与已批准待开票占用后的额度。
    /// # 返回
    /// 可申请时返回 None，否则返回面向经办人的原因。
    /// # 错误
    /// 不返回错误。
    pub fn unavailable_reason(&self, available: Amount) -> Option<&'static str> {
        if !self.effective {
            Some("销售单尚未生效，暂不能申请开票")
        } else if available <= Amount::zero() {
            Some("暂无可申请开票金额")
        } else if self.invoice_title.trim().is_empty() {
            Some("请先在结算主体档案中补齐法定名称，再申请开票")
        } else if self.tax_number.trim().is_empty() {
            Some("请先在结算主体档案中补齐统一社会信用代码，再申请开票")
        } else {
            None
        }
    }

    /// 校验准入及页面确认的购方资料，禁止申请人覆盖主体身份。
    /// # 参数
    /// * `data` - 页面提交的开票资料。
    /// * `available` - 当前可申请额度。
    /// # 返回
    /// 资料与当前主体一致且准入满足时返回成功。
    /// # 错误
    /// 来源未生效、无额度、档案不全或页面资料已变化时返回错误。
    pub fn validate(&self, data: &InvoiceRequestData, available: Amount) -> Result<()> {
        if let Some(reason) = self.unavailable_reason(available) {
            return Err(Error::from(reason));
        }
        if data.invoice_title.trim() != self.invoice_title || data.tax_number.trim() != self.tax_number {
            return Err(Error::from("开票抬头或税号与结算主体档案不一致，请刷新后重新核对"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn source() -> InvoiceRequestSource {
        InvoiceRequestSource {
            effective: true,
            invoice_title: "结算公司".into(),
            tax_number: "91310000123456789A".into(),
        }
    }

    #[test]
    fn eligibility_requires_effective_source_positive_capacity_and_complete_identity() {
        let positive = Amount::from_str("0.01").unwrap();
        let mut source = source();
        assert!(source.unavailable_reason(positive).is_none());
        for value in ["0", "-0.01"] {
            assert_eq!(
                source.unavailable_reason(Amount::from_str(value).unwrap()),
                Some("暂无可申请开票金额")
            );
        }
        source.effective = false;
        assert_eq!(source.unavailable_reason(positive), Some("销售单尚未生效，暂不能申请开票"));
        source.effective = true;
        source.tax_number.clear();
        assert_eq!(
            source.unavailable_reason(positive),
            Some("请先在结算主体档案中补齐统一社会信用代码，再申请开票")
        );
        source.invoice_title.clear();
        assert_eq!(source.unavailable_reason(positive), Some("请先在结算主体档案中补齐法定名称，再申请开票"));
    }

    #[test]
    fn rejects_changed_or_tampered_buyer_identity() {
        let source = source();
        let amount = Amount::from_str("100").unwrap();
        let data = InvoiceRequestData {
            amount,
            invoice_title: source.invoice_title.clone(),
            tax_number: source.tax_number.clone(),
            invoice_content: "服务费".into(),
            reason: "按约定开票".into(),
        };
        assert!(source.validate(&data, amount).is_ok());
        assert!(source.validate(&data, Amount::zero()).is_err());
        let changed_title = InvoiceRequestData { invoice_title: "另一公司".into(), ..data.clone() };
        assert!(source.validate(&changed_title, amount).is_err());
        let changed_tax = InvoiceRequestData { tax_number: "91310000987654321A".into(), ..data };
        assert!(source.validate(&changed_tax, amount).is_err());
    }
}
