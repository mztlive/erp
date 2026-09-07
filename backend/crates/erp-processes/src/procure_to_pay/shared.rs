//! 供应商付款条件提供方的采购命令适配。
use super::PurchaseOrderProcess;
use erp_procurement::entity::purchase_order::PaymentTermSnapshot;
use services::Result;
impl PurchaseOrderProcess {
    /// 解析付款条件并生成门禁快照（金额/比例门槛暂空）。
    pub(super) async fn payment_term_snapshot(&self, payment_term_code: &str) -> Result<PaymentTermSnapshot> {
        let payment_term = super::adapters::payment_term::parse(payment_term_code)?;
        PaymentTermSnapshot::new(
            payment_term.canonical_code,
            payment_term.prepay_gate,
            None,
            None,
            super::adapters::payment_term::parse_snapshot,
        )
        .map_err(Into::into)
    }
}
