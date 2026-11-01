//! 后补合同的条款核对；不修改任一来源快照。
use std::str::FromStr;

use erp_core::ids::CustomerAccountId;
use erp_core::{Error, Result};
use rust_decimal::Decimal;
use serde::Serialize;

use super::{InvoiceRequirementSnapshot, PaymentTermSnapshot};

/// 核对销售客户与合同对方身份；兼容旧客户端省略客户的请求。
///
/// # 参数
/// `expected` 为合同权威客户，`selected` 为销售命令或已保存销售单的客户。
/// # 返回
/// 未声明客户或身份相同时通过。
/// # 错误
/// 身份不同时拒绝，禁止按合同静默更换已选择的客户。
pub fn ensure_contract_customer(
    expected: &CustomerAccountId,
    selected: Option<&CustomerAccountId>,
) -> Result<()> {
    if selected.is_some_and(|id| id != expected) {
        return Err(Error::from("销售单客户必须与合同对方主体对应的客户一致"));
    }
    Ok(())
}

/// 待核对的结构化商业条款。
#[derive(Debug, Clone)]
pub struct ContractTerms {
    pub payment: PaymentTermSnapshot,
    pub invoice: InvoiceRequirementSnapshot,
}

/// 单项核对结果，保留两侧原始展示值。
#[derive(Debug, Serialize)]
pub struct ContractTermCheck {
    pub field: &'static str,
    pub label: &'static str,
    pub sales_value: String,
    pub contract_value: String,
    pub matches: bool,
}

/// 补录前展示及事务内最终准入共用的核对结果。
#[derive(Debug, Serialize)]
pub struct ContractBindingCheck {
    pub basis: String,
    pub matches: bool,
    pub items: Vec<ContractTermCheck>,
}

impl ContractTerms {
    /// 逐项比较付款代码、开票类型和百分数税点。
    ///
    /// # 参数
    /// * `contract` - 合同当前有效修订的条款
    /// * `basis` - 原销售单条款来源说明
    /// # 返回
    /// 返回完整核对结果；空值及无法解析的税点均不匹配。
    /// # 错误
    /// 无；差异记录在返回值中。
    pub fn compare(&self, contract: &Self, basis: String) -> ContractBindingCheck {
        let items = vec![
            ContractTermCheck {
                field: "payment_term",
                label: "付款条件",
                sales_value: self.payment.payment_term_name.clone(),
                contract_value: contract.payment.payment_term_name.clone(),
                matches: same_text(&self.payment.payment_term_code, &contract.payment.payment_term_code),
            },
            ContractTermCheck {
                field: "invoice_type",
                label: "开票要求",
                sales_value: self.invoice.invoice_type.clone(),
                contract_value: contract.invoice.invoice_type.clone(),
                matches: same_text(&self.invoice.invoice_type, &contract.invoice.invoice_type),
            },
            ContractTermCheck {
                field: "tax_point",
                label: "税率（%）",
                sales_value: self.invoice.tax_point.clone(),
                contract_value: contract.invoice.tax_point.clone(),
                matches: tax_percent(&self.invoice.tax_point)
                    .is_some_and(|value| Some(value) == tax_percent(&contract.invoice.tax_point)),
            },
        ];
        ContractBindingCheck { basis, matches: items.iter().all(|item| item.matches), items }
    }
}

impl ContractBindingCheck {
    /// 拒绝任一不一致条款，不允许补录命令覆盖原单。
    ///
    /// # 参数
    /// 无；使用本次核对结果。
    /// # 返回
    /// 全部一致时返回成功。
    /// # 错误
    /// 返回每项差异及后续处理指引。
    pub fn ensure_matches(&self) -> Result<()> {
        if self.matches {
            return Ok(());
        }
        let differences = self
            .items
            .iter()
            .filter(|item| !item.matches)
            .map(|item| {
                format!("{}：原单「{}」，合同「{}」", item.label, item.sales_value, item.contract_value)
            })
            .collect::<Vec<_>>()
            .join("；");
        Err(Error::from(format!(
            "合同条款不一致，不能补录。{differences}。请先修正销售草稿或完成销售变更，再重新核对"
        )))
    }
}

fn same_text(left: &str, right: &str) -> bool {
    !left.trim().is_empty() && left.trim() == right.trim()
}

// 税点按百分数比较；6、6.00、6% 等价，0.06 不推断为 6%。
fn tax_percent(raw: &str) -> Option<Decimal> {
    let text = raw.trim().strip_suffix('%').unwrap_or(raw.trim()).trim();
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit() || byte == b'.') {
        return None;
    }
    Decimal::from_str(text).ok().filter(|value| *value >= Decimal::ZERO && *value <= Decimal::ONE_HUNDRED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_customer_accepts_same_or_legacy_omission_and_rejects_different_identity() {
        let customer = CustomerAccountId::new("customer-1");
        ensure_contract_customer(&customer, None).unwrap();
        ensure_contract_customer(&customer, Some(&customer)).unwrap();
        let error = ensure_contract_customer(&customer, Some(&CustomerAccountId::new("other"))).unwrap_err();
        assert!(error.to_string().contains("客户必须与合同"));
    }

    fn terms() -> ContractTerms {
        ContractTerms {
            payment: PaymentTermSnapshot::new("PREPAY_100", "先款 100%").unwrap(),
            invoice: InvoiceRequirementSnapshot::new("增值税专用发票", "6").unwrap(),
        }
    }

    #[test]
    fn equivalent_tax_and_renamed_payment_label_preserve_snapshots() {
        let sales = terms();
        let mut contract = terms();
        contract.invoice.tax_point = "6.00%".into();
        contract.payment.payment_term_name = "全额预付".into();
        let check = sales.compare(&contract, "当前生效版本".into());
        check.ensure_matches().unwrap();
        assert_eq!(check.items[0].sales_value, "先款 100%");
        assert_eq!(sales.invoice.tax_point, "6");
        assert_eq!(contract.invoice.tax_point, "6.00%");
    }

    #[test]
    fn every_difference_is_returned_and_rejected() {
        let mut contract = terms();
        contract.payment = PaymentTermSnapshot::new("POSTPAY_NET30", "货到 30 天").unwrap();
        contract.invoice = InvoiceRequirementSnapshot::new("增值税普通发票", "13").unwrap();
        let check = terms().compare(&contract, "当前草稿".into());
        assert!(check.items.iter().all(|item| !item.matches));
        let error = check.ensure_matches().unwrap_err().to_string();
        for label in ["付款条件", "开票要求", "税率"] {
            assert!(error.contains(label));
        }
    }

    #[test]
    fn a_single_difference_is_sufficient_to_block_binding() {
        for field in ["payment_term", "invoice_type", "tax_point"] {
            let mut contract = terms();
            match field {
                "payment_term" => contract.payment.payment_term_code = "POSTPAY_NET30".into(),
                "invoice_type" => contract.invoice.invoice_type = "不开发票".into(),
                _ => contract.invoice.tax_point = "13".into(),
            }
            let check = terms().compare(&contract, "当前草稿".into());
            assert!(check.ensure_matches().is_err());
            let differences = check.items.iter().filter(|item| !item.matches).collect::<Vec<_>>();
            assert_eq!(differences.len(), 1);
            assert_eq!(differences[0].field, field);
        }
    }

    #[test]
    fn missing_or_ambiguous_terms_fail_closed() {
        for raw in ["", "未知", "-1", "101", "1e1", "0.06"] {
            let mut contract = terms();
            contract.invoice.tax_point = raw.into();
            assert!(!terms().compare(&contract, "草稿".into()).matches, "{raw}");
        }
        let mut empty = terms();
        empty.payment.payment_term_code.clear();
        empty.invoice.invoice_type.clear();
        empty.invoice.tax_point.clear();
        assert!(!empty.compare(&empty, "草稿".into()).matches);
    }
}
