//! Consumer ports for customer, identity, attachment and audit facts.

mod audit;
mod customer;
mod file_asset;
mod identity;

pub use audit::{ContractAuditPort, FailClosedAuditPort, PreparedContractAudit};
pub use customer::{
    CustomerAccountFact, CustomerAssignmentFactsPort, CustomerFactsPort, EmptyAssignments, EmptyCustomers,
    FailClosedAssignmentFactsPort, FailClosedCustomerFactsPort,
};
pub use file_asset::{EmptyFileAssetFacts, FailClosedFileAssetFacts, FileAssetFact, FileAssetFactsPort};
pub use identity::{AccountNamePort, EmptyAccountNames, FailClosedAccountNamePort};
