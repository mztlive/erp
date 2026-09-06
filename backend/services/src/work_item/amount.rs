//! Private amount and impact helpers for remaining-domain object facts.

use erp_core::money::Amount;

/// Drop blank text after trimming.
pub(super) fn non_empty(value: &str) -> Option<String> {
    let text = value.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Format a tax-inclusive amount as grouped RMB copy, e.g. `¥12,800`.
pub(super) fn format_yuan(amount: &Amount) -> String {
    let raw = amount.to_decimal().normalize().to_string();
    let (int_part, frac) = raw.split_once('.').unwrap_or((raw.as_str(), ""));
    let grouped = group_int(int_part);
    if frac.is_empty() || frac.chars().all(|ch| ch == '0') {
        format!("¥{grouped}")
    } else {
        format!("¥{grouped}.{frac}")
    }
}

/// Build purchase-review impact from line count, gross amount and prepay gate.
pub(super) fn purchase_review_impact_summary(
    line_count: Option<usize>,
    gross_amount: Option<&Amount>,
    prepay_gate: bool,
) -> String {
    let mut summary = "不审核则不能形成应付、不能付款".to_string();
    let mut scale = Vec::new();
    if let Some(count) = line_count.filter(|count| *count > 0) {
        scale.push(format!("{count} 行"));
    }
    if let Some(amount) = gross_amount {
        scale.push(format_yuan(amount));
    }
    if !scale.is_empty() {
        summary.push_str(" · ");
        summary.push_str(&scale.join(" / "));
    }
    if prepay_gate {
        summary.push_str(" · 先款后货");
    }
    summary
}

fn group_int(int_part: &str) -> String {
    let (sign, digits) = int_part
        .strip_prefix('-')
        .map(|digits| ("-", digits))
        .unwrap_or(("", int_part));
    let mut grouped = String::new();
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{sign}{}", grouped.chars().rev().collect::<String>())
}

#[cfg(test)]
mod tests {
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
