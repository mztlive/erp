//! 采购消费方测试所用受控付款事实替身，不访问供应商资料。

use erp_core::{Error, Result};

use super::facts::PaymentTermFact;

/// 返回原测试夹具的付款条件事实；生产解析由供应商适配器提供。
pub(crate) fn payment_term(raw: &str) -> Result<PaymentTermFact> {
    let (canonical_code, prepay_gate, days_after_delivery) = match raw.trim() {
        "PREPAY_100" | "PREPAY-100" => ("PREPAY_100", true, None),
        "PREPAY_50" | "PREPAY-50" => ("PREPAY_50", true, None),
        "PREPAY_30" | "PREPAY-30" => ("PREPAY_30", true, None),
        "CASH_ON_APPROVAL" | "现结" => ("CASH_ON_APPROVAL", false, None),
        "NET-15" | "POSTPAY_NET15" => ("POSTPAY_NET15", false, Some(15)),
        "NET-30" | "NET30" | "POSTPAY_NET30" => ("POSTPAY_NET30", false, Some(30)),
        _ => {
            return Err(Error::from("付款条件缺少可计算规则，请在供应商资料中选择具体付款条件"));
        },
    };
    let mut fact = PaymentTermFact::new(canonical_code).with_prepay_gate(prepay_gate);
    if let Some(days) = days_after_delivery {
        fact = fact.with_days_after_delivery(days);
    }
    match canonical_code {
        "PREPAY_100" => Ok(fact.with_prepay_minimum_ratio("1.0".parse().unwrap())),
        "PREPAY_50" => Ok(fact.with_prepay_minimum_ratio("0.5".parse().unwrap())),
        "PREPAY_30" => Ok(fact.with_prepay_minimum_ratio("0.3".parse().unwrap())),
        _ => Ok(fact),
    }
}
