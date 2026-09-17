//! Customer domain: customer accounts, assignments and profile-command facts.

pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod ports;
pub mod repository;
pub mod service;

pub use application_core::PageView;
pub use dto::{
    AddressType, AssignmentAction, CreateCustomerRequest, CustomerActionBlockerView,
    CustomerAssignmentListParams, CustomerAssignmentRequest, CustomerAssignmentView, CustomerDetailView,
    CustomerListParams, CustomerProfileAddressInput, CustomerProfileBankAccountInput,
    CustomerProfileContactInput, CustomerProfileDetailView, CustomerProfileMutationView, CustomerScope,
    CustomerSensitiveFieldView, CustomerSensitiveRevealView, CustomerView, EffectiveRecordStatus,
    PartyAddressView, PartyBankAccountView, PartyContactView, PartyRevisionView, PartyStatus,
    PartyTaxProfileView, RevealCustomerSensitiveRequest, SaveCustomerProfileRequest, SensitiveFieldKind,
    UpdateCustomerRequest, customer_status_blockers,
};
pub use entity::{
    AssignCustomerAssignment, AssignmentRole, CustomerAccount, CustomerAccountData, CustomerAccountId,
    CustomerAccountStatus, CustomerAccountUpdate, CustomerAssignment, CustomerAssignmentCommand,
    CustomerAssignmentData, CustomerAssignmentId, CustomerAssignmentUpdate, CustomerProfileCommand,
    CustomerProfileCommandData, CustomerProfileCommandResultData, CustomerProfileFactInput,
    CustomerProfileFactKind, CustomerProfileFactSet, CustomerProfileOperation, CustomerProfileReplayContext,
    CustomerProfileRequestFingerprint, CustomerProfileRequestShape, EndCustomerAssignment,
};
pub use error::{Error, Result, known_duplicate_index_message};
pub use ports::{
    AccountFactPort, CustomerAuditPort, CustomerDataScopePort, CustomerResolvedClause, CustomerResolvedScope,
    CustomerScopeObject, FailClosedAccountFactPort, FailClosedAuditPort, FailClosedCustomerDataScopePort,
    FailClosedPartyFactPort, PartyFactPort, PartyIdentityFact, PreparedCustomerAudit, ValidatedAuditSnapshot,
};
pub use repository::{
    CustomerAccountFilter, CustomerAccountRepository, CustomerAccountRow, CustomerAssignmentFilter,
    CustomerAssignmentRepository, CustomerAssignmentRow, CustomerExt, CustomerProfileCommandRepository,
};
pub use service::{CustomerAccess, CustomerAssignmentService, CustomerListView, CustomerService};
