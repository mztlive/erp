//! 销售审核域结构化快照值对象（数据模型 §4.4 / P1 §2.2）。
//!
//! 本模块是销售单域快照值对象的同名复用出口：变更提交与销售提交/正式版本内联
//! 相同的客户名称、合同编号、结算主体、税务与付款条件结构化快照，规则与序列化
//! 形状由 [`crate::entity::sales_order::snapshot`] 唯一承载，此处只做复用，
//! 不得另行定义同形副本。

pub use crate::entity::sales_order::snapshot::{
    ContractSnapshot, CustomerSnapshot, HeaderSnapshotData, HeaderSnapshots, InvoiceRequirementSnapshot,
    PaymentTermSnapshot, SettlementPartySnapshot,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_snapshots_reuse_canonical_normalization() {
        let snapshots = HeaderSnapshots::build(&HeaderSnapshotData {
            customer_name: " 东方企业 ".to_string(),
            contract_no: Some(" HT-2026-0088 ".to_string()),
            settlement_party_name: Some(" 集团结算中心 ".to_string()),
            payment_term_code: "NET30".to_string(),
            payment_term_name: " 月结 30 天 ".to_string(),
            invoice_type: " 增值税专用发票 ".to_string(),
            tax_point: " 6 ".to_string(),
        })
        .unwrap();

        assert_eq!(snapshots.customer_snapshot.customer_name, "东方企业");
        assert_eq!(snapshots.contract_snapshot.unwrap().contract_no, "HT-2026-0088");
        assert!(CustomerSnapshot::new("   ".to_string()).is_err());
    }
}
