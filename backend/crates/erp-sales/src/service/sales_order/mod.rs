//! Sales-owned snapshot construction, rules and transaction-local persistence.

pub mod command;
pub mod draft_working_copy;
pub mod formalize;
pub mod lifecycle;
pub mod mapper;
pub mod procurement;
pub mod progress;
mod query;
mod sellable;

/// Sales operations using only sales repositories and consumer-owned facts.
#[derive(Clone)]
pub struct SalesOrderService {
    pub(crate) db: mongodb::Database,
}
impl SalesOrderService {
    /// Construct the sales service without performing reads or writes.
    ///
    /// The caller supplies transaction executors and any required provider ports per operation.
    pub fn new(db: mongodb::Database) -> Self {
        Self { db }
    }
}
