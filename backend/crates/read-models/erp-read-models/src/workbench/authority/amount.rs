//! 权威事实复用工作台既有的唯一金额格式与简报规则。
pub(super) use crate::workbench::brief::non_empty;
pub(super) use crate::workbench::presentation::{format_yuan, purchase_review_impact_summary};
#[cfg(test)]
mod tests {
    use erp_core::money::Amount;

    use super::*;

    #[test]
    fn format_yuan_groups_thousands_and_keeps_nonzero_fraction() {
        let whole: Amount = "12800".parse().expect("test amount");
        let frac: Amount = "12800.5".parse().expect("test amount");
        assert_eq!(format_yuan(&whole), "¥12,800");
        assert_eq!(format_yuan(&frac), "¥12,800.5");
    }

    #[test]
    fn purchase_review_impact_appends_scale_and_prepay_gate() {
        let amount: Amount = "8000".parse().expect("test amount");
        assert_eq!(
            purchase_review_impact_summary(Some(3), Some(&amount), true),
            "不审核则不能形成应付、不能付款 · 3 行 / ¥8,000 · 先款后货"
        );
    }
}
