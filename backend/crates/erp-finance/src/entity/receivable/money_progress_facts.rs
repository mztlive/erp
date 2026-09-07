//! Per-account monetary facts published for consumer progress calculations.

use super::ReceivableAccount;
use erp_core::money::Amount;

/// Preserve each account, including accounts whose four balances are zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReceivableMoneyProgressFact {
    /// Amount not yet settled.
    pub open_total: Amount,
    /// Amount already settled.
    pub settled_total: Amount,
    /// Amount remaining invoiceable.
    pub open_invoiceable_total: Amount,
    /// Amount already invoiced.
    pub invoiced_total: Amount,
}
impl From<&ReceivableAccount> for ReceivableMoneyProgressFact {
    fn from(account: &ReceivableAccount) -> Self {
        Self {
            open_total: account.open_total,
            settled_total: account.settled_total,
            open_invoiceable_total: account.open_invoiceable_total,
            invoiced_total: account.invoiced_total,
        }
    }
}
