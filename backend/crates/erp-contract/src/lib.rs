//! Contract domain: stable contracts, immutable revisions and PDF associations.

pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod ports;
pub mod repository;
pub mod service;

pub use application_core::PageView;
pub use dto::contract::{
    ArchiveContractRevisionRequest, ContractDetailView, ContractListParams, ContractListScope,
    ContractListView, ContractRevisionView, ContractView, CreateContractRequest, TerminateContractRequest,
    UploadContractRequest, UploadContractView,
};
pub use entity::contract::snapshot::ContractSnapshot;
pub use entity::contract::{
    ArchiveSource, Contract, ContractData, ContractId, ContractRevision, ContractRevisionData,
    ContractRevisionId, ContractStatus, ContractUpdate, CustomerSnapshot, InvoiceRequirementSnapshot,
    PaymentTermSnapshot, SettlementPartySnapshot,
};
pub use error::{known_duplicate_index_message, Error, Result};
pub use ports::{
    AccountNamePort, ContractAssignmentFact, ContractAuditPort, ContractDataScopePort,
    ContractParticipantPort, ContractResolvedClause, ContractResolvedScope, ContractScopeObject,
    CustomerAccountFact, CustomerAssignmentFactsPort, CustomerFactsPort, EmptyAccountNames, EmptyAssignments,
    EmptyContractParticipants, EmptyCustomers, EmptyFileAssetFacts, FailClosedAccountNamePort,
    FailClosedAssignmentFactsPort, FailClosedAuditPort, FailClosedContractDataScopePort,
    FailClosedContractParticipantPort, FailClosedCustomerFactsPort, FailClosedFileAssetFacts, FileAssetFact,
    FileAssetFactsPort, PreparedContractAudit,
};
pub use repository::{
    ContractDomainRepository, ContractExt, ContractFilter, ContractRepository, ContractRevisionRepository,
    ContractRow,
};
pub use service::contract::{
    plan_first_archive, plan_upload_archive, ContractAccess, ContractScopePorts, ContractService,
    PlannedContractArchive,
};
