//! 客户、身份、附件与审计事实的消费方端口。

mod audit;
mod company;
mod customer;
mod file_asset;
mod participant;

pub use audit::{ContractAuditPort, FailClosedAuditPort, PreparedContractAudit};
pub use company::{SigningCompanyFact, SigningCompanyPort};
pub use customer::{
    ContractCreationPort, CustomerAccountFact, CustomerFactsPort, EmptyCustomers, FailClosedCustomerFactsPort,
};
pub use file_asset::{EmptyFileAssetFacts, FailClosedFileAssetFacts, FileAssetFact, FileAssetFactsPort};
pub use participant::{
    ContractParticipantPort, EmptyContractParticipants, FailClosedContractParticipantPort,
};

pub use crate::authorization::CompiledScope as ContractResolvedScope;

pub mod recognition;
