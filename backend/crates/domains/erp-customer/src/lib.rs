//! 客户领域：客户角色、归属与资料命令事实。

pub mod authorization;
pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod ports;
pub mod repository;
pub mod service;

pub use application_core::PageView;
pub use authorization::CustomerResolvedScope;
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
    CustomerAuditPort, FailClosedAuditPort, FailClosedPartyFactPort, PartyFactPort, PartyIdentityFact,
    PreparedCustomerAudit, ValidatedAuditSnapshot,
};
pub use repository::{
    CustomerAccountFilter, CustomerAccountRepository, CustomerAccountRow, CustomerAssignmentFilter,
    CustomerAssignmentRepository, CustomerAssignmentRow, CustomerExt, CustomerProfileCommandRepository,
};
pub use service::{CustomerAccess, CustomerAssignmentService, CustomerListView, CustomerService};
