//! 开票申请的来源准入与购销双方资料校验；事实由组合层读取。
use erp_core::money::Amount;
use erp_core::{Error, Result};

use super::InvoiceRequestData;

/// 当前销售来源及购销双方的法定开票资料。
pub struct InvoiceRequestSource {
    pub effective: bool,
    pub invoice_title: String,
    pub tax_number: String,
    pub seller_name: String,
    pub seller_tax_number: String,
}

impl InvoiceRequestSource {
    /// 返回当前不能申请开票的原因。
    /// 结果：可申请时返回 None，否则返回面向经办人的原因。
    ///
    /// 输入约定：
    /// - `available`：扣除审批中与已批准待开票占用后的额度。
    pub fn unavailable_reason(&self, available: Amount) -> Option<&'static str> {
        if !self.effective {
            Some("销售单尚未生效，暂不能申请开票")
        } else if available <= Amount::zero() {
            Some("暂无可申请开票金额")
        } else {
            self.missing_identity_reason()
        }
    }

    fn missing_identity_reason(&self) -> Option<&'static str> {
        if self.invoice_title.trim().is_empty() {
            Some("请先在结算主体档案中补齐法定名称，再申请开票")
        } else if self.tax_number.trim().is_empty() {
            Some("请先在结算主体档案中补齐统一社会信用代码，再申请开票")
        } else if self.seller_name.trim().is_empty() || self.seller_tax_number.trim().is_empty() {
            Some("请先在我方开票主体档案中补齐法定名称和纳税人识别号，再申请开票")
        } else {
            None
        }
    }

    fn validate_identity(&self, data: &InvoiceRequestData) -> Result<()> {
        if data.invoice_title.trim() != self.invoice_title || data.tax_number.trim() != self.tax_number {
            return Err(Error::from("开票抬头或税号与结算主体档案不一致，请刷新后重新核对"));
        }
        Ok(())
    }

    /// 核对购销双方并冻结销方档案。提交时传入可用额度；仅保存编辑资料时传入 `None`。
    pub fn prepare_data(
        &self,
        mut data: InvoiceRequestData,
        available: Option<Amount>,
    ) -> Result<InvoiceRequestData> {
        if let Some(reason) = self.unavailable_reason(available.unwrap_or(data.amount)) {
            return Err(Error::from(reason));
        }
        self.validate_identity(&data)?;
        if data.lines.is_empty() {
            return Err(Error::from("请填写至少一条开票明细"));
        }
        if data.seller_name.as_ref().is_some_and(|name| name.trim() != self.seller_name)
            || data.seller_tax_number.as_ref().is_some_and(|tax| tax.trim() != self.seller_tax_number)
        {
            return Err(Error::from("我方开票主体资料已变化，请刷新后重新核对"));
        }
        data.seller_name = Some(self.seller_name.clone());
        data.seller_tax_number = Some(self.seller_tax_number.clone());
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::entity::receivable::InvoiceRequestLine;

    fn source() -> InvoiceRequestSource {
        InvoiceRequestSource {
            effective: true,
            invoice_title: "结算公司".into(),
            tax_number: "91310000123456789A".into(),
            seller_name: "我方公司".into(),
            seller_tax_number: "91110108123456789X".into(),
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
            seller_name: None,
            seller_tax_number: None,
            lines: vec![InvoiceRequestLine {
                content: "服务费".into(),
                quantity: "2".parse().unwrap(),
                unit_price: "50".parse().unwrap(),
            }],
            reason: "按约定开票".into(),
        };

        let saved = source.prepare_data(data.clone(), Some(amount)).unwrap();
        assert_eq!(saved.seller_name.as_deref(), Some(source.seller_name.as_str()));
        assert_eq!(saved.seller_tax_number.as_deref(), Some(source.seller_tax_number.as_str()));
        assert!(source.prepare_data(data.clone(), Some(Amount::zero())).is_err());
        assert!(source.prepare_data(data.clone(), None).is_ok());

        let changed_title = InvoiceRequestData { invoice_title: "另一公司".into(), ..data.clone() };
        assert!(source.prepare_data(changed_title, None).is_err());
        let changed_tax = InvoiceRequestData { tax_number: "91310000987654321A".into(), ..data.clone() };
        assert!(source.prepare_data(changed_tax, None).is_err());
        let changed_seller = InvoiceRequestData { seller_name: Some("另一公司".into()), ..data.clone() };
        assert!(source.prepare_data(changed_seller, None).is_err());
        let no_lines = InvoiceRequestData { lines: Vec::new(), ..data };
        assert!(source.prepare_data(no_lines, None).is_err());
    }
}
