//! Receivable account and customer receipt read models.

mod account;
pub mod approval_view;
mod customer_receipt;
pub mod snapshot;

/// Read-only receivable projections spanning finance, sales and workflow facts.
///
/// All commands and transaction ownership stay with financial posting processes.
pub struct ReceivableReadService {
    db: mongodb::Database,
}

impl ReceivableReadService {
    /// Construct the read model for the supplied database.
    ///
    /// Construction performs no reads or writes and cannot fail.
    pub fn new(db: mongodb::Database) -> Self {
        Self { db }
    }
}

pub mod invoice_request;
