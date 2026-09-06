//! Payable-account fact mapping for remaining-domain authorization.

use entities::payable::PayableAccount;

use super::super::amount::format_yuan;
use super::super::ObjectFact;

/// Build a payable-account object fact for payment-execution tasks.
pub(super) fn payable_account_fact(
    account: PayableAccount,
    supplier: Option<String>,
    purchase_no: Option<String>,
) -> ObjectFact {
    let label = purchase_no
        .as_ref()
        .map(|no| format!("采购应付 {no}"))
        .unwrap_or_else(|| "采购应付".to_string());
    let mut fact = ObjectFact::new(account.source_document_id, label, account.stable.created_by);
    fact.counterparty_label = supplier;
    fact.impact_summary = Some(format!("未付金额 {}", format_yuan(&account.open_total)));
    fact
}
