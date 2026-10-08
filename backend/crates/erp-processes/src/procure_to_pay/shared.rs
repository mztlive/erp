//! 供应商付款条件提供方的采购命令适配。
use erp_procurement::entity::purchase_order::PaymentTermSnapshot;

use super::PurchaseOrderProcess;
use crate::Result;
impl PurchaseOrderProcess {
    /// 解析付款条件并由供应商受控规则冻结门禁比例。
    ///
    /// # 参数
    /// * `payment_term_code` - 采购单上的付款条件代码。
    ///
    /// # 返回
    /// 返回按解析结果冻结预付门禁的付款条件快照。
    ///
    /// # 错误
    /// 付款条件无法解析或快照构造失败时返回对应错误。
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
