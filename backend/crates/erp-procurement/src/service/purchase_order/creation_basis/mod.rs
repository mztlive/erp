//! 采购依据资格、库存分组和创建规则。
pub mod create;
mod mapping;
mod stock;
mod submission;
pub use create::{
    ComputedSelection, SelectedLine, build_submission_line, compute_selected_lines,
    ensure_expected_delivery_within_sales_due, ensure_request_scope, find_requested_group,
    parse_basis_sales_order_id, procurement_quantity_changed, validate_requested_quantities,
};
pub use mapping::{basis_groups_from_facts, business_date_of, zero_quantity};
pub use stock::{physical_stock_lines, stock_groups_from_facts};
pub use submission::build_draft_submission;

pub(crate) use crate::entity::purchase_order::zero_amount;

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::Instant;

    use super::{business_date_of, ensure_expected_delivery_within_sales_due, parse_basis_sales_order_id};
    /// 上海零点对应前一日 UTC 时仍还原业务自然日。
    #[test]
    fn business_date_uses_shanghai_timezone() {
        let unix_secs = chrono::DateTime::parse_from_rfc3339("2026-08-23T00:00:00+08:00")
            .expect("测试时间合法")
            .timestamp();

        let date = business_date_of(Instant::from_unix_secs(unix_secs)).expect("业务日期合法");

        assert_eq!(date.to_string(), "2026-08-23");
    }
    /// 新依据 ID 只接受销售单加 SHA-256，不兼容旧供应商拼接形式。
    #[test]
    fn basis_id_parser_rejects_legacy_shape() {
        let digest = crate::entity::purchase_order::digest_parts(["scope".to_string()]);
        assert_eq!(parse_basis_sales_order_id(&format!("so-1:{digest}")).unwrap().to_string(), "so-1");
        assert!(parse_basis_sales_order_id("so-1:supplier-1").is_err());
    }
    /// 采购预计交付日可以早于或等于销售承诺期限，但不得晚于该期限。
    #[test]
    fn expected_delivery_must_not_exceed_sales_due() {
        let sales_due = erp_core::common::time::BusinessDate::from_str("2026-09-10").unwrap();
        let earlier = erp_core::common::time::BusinessDate::from_str("2026-09-09").unwrap();
        let later = erp_core::common::time::BusinessDate::from_str("2026-09-11").unwrap();

        assert!(ensure_expected_delivery_within_sales_due(earlier, sales_due).is_ok());
        assert!(ensure_expected_delivery_within_sales_due(sales_due, sales_due).is_ok());
        assert!(ensure_expected_delivery_within_sales_due(later, sales_due).is_err());
    }
}
