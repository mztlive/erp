//! 合同领域：稳定合同、不可变修订与 PDF 关联。

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
pub use error::{Error, Result, known_duplicate_index_message};
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
    ContractAccess, ContractScopePorts, ContractService, PlannedContractArchive, plan_first_archive,
    plan_upload_archive,
};
pub use service::template::ContractTemplateService;
