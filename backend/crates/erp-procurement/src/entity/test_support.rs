//! 采购消费方测试所用受控付款事实替身，不访问供应商资料。

use super::facts::PaymentTermFact;
use erp_core::{Error, Result};

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
            return Err(Error::from(
                "付款条件缺少可计算规则，请在供应商资料中选择具体付款条件",
            ));
        }
    };
    Ok(PaymentTermFact {
        canonical_code: canonical_code.to_string(),
        prepay_gate,
        prepay_minimum_ratio: match canonical_code {
            "PREPAY_100" => Some("1.0".parse().unwrap()),
            "PREPAY_50" => Some("0.5".parse().unwrap()),
            "PREPAY_30" => Some("0.3".parse().unwrap()),
            _ => None,
        },
        days_after_delivery,
        calendar_due: None,
    })
}
