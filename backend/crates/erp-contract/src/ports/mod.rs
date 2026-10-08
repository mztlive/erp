//! 客户、身份、附件与审计事实的消费方端口。

mod audit;
mod company;
mod customer;
mod data_scope;
mod file_asset;
mod identity;
mod participant;

pub use audit::{ContractAuditPort, FailClosedAuditPort, PreparedContractAudit};
pub use company::{SigningCompanyFact, SigningCompanyPort};
pub use customer::{
    ContractAssignmentFact, CustomerAccountFact, CustomerAssignmentFactsPort, CustomerFactsPort,
    EmptyAssignments, EmptyCustomers, FailClosedAssignmentFactsPort, FailClosedCustomerFactsPort,
};
pub use data_scope::{
    ContractDataScopePort, ContractResolvedClause, ContractResolvedScope, ContractScopeObject,
    FailClosedContractDataScopePort,
};
pub use file_asset::{EmptyFileAssetFacts, FailClosedFileAssetFacts, FileAssetFact, FileAssetFactsPort};
pub use identity::{AccountNamePort, EmptyAccountNames, FailClosedAccountNamePort};
pub use participant::{
    ContractParticipantPort, EmptyContractParticipants, FailClosedContractParticipantPort,
};

pub mod recognition;
