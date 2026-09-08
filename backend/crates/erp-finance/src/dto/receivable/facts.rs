//! 应收账户关联的回款与发票记录。

use erp_core::money::Amount;
use serde::Serialize;

/// 当前应收账户关联的正式回款事实投影。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReceivableReceiptFactView {
    /// 回款单 ID。
    pub receipt_id: String,
    /// 回款单号。
    pub receipt_no: String,
    /// 实际到账时间（RFC 3339）。
    pub received_at: String,
    /// 回款含税金额。
    pub gross_amount: Amount,
    /// 当前应收账户的净核销金额。
    pub allocated_to_account: Amount,
    /// 分配到其它账户的说明；当前投影无法完整证明时为空。
    pub other_allocation_summary: Option<String>,
    /// 回款单是否已经冲正。
    pub reversed: bool,
}

/// 当前应收账户关联的正式销项发票事实投影。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReceivableInvoiceFactView {
    /// 发票 ID。
    pub invoice_id: String,
    /// 发票号码。
    pub invoice_no: String,
    /// 蓝票或红票稳定代码（`BLUE` / `RED`）。
    pub direction: String,
    /// 开票业务日期（`YYYY-MM-DD`）。
    pub issued_at: String,
    /// 含税金额。
    pub gross_amount: Amount,
    /// 不含税金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 当前应收账户的净分配含税金额。
    pub allocated_to_account: Amount,
    /// 当前发票是否已被红冲。
    pub reversed: bool,
}
