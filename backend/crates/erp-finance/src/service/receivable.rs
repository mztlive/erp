//! Receivable finance operations and deterministic command preparation.
//! Cross-domain transactions and workflow commands are owned by finance posting processes.

pub mod customer_receipt_commit;
pub mod invoice_commit;
pub mod mapping;

pub mod card_funds_register;
pub mod customer_receipt_posting;
pub mod invoice_posting;
pub mod red_invoice_plan;

pub mod card_funds_decision;
pub mod card_funds_receipt;

mod invoice_query;

/// Finance-only receivable queries and transaction-independent application operations.
pub struct ReceivableService {
    db: mongodb::Database,
}

impl ReceivableService {
    /// Construct a finance service over the supplied database handle.
    ///
    /// No reads, writes or transaction boundaries are introduced by construction.
    pub fn new(db: mongodb::Database) -> Self {
        Self { db }
    }
}

pub mod red_invoice_posting;

pub mod initial_account;

pub mod sales_change;

pub mod receipt_reversal;
