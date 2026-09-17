//! Supplier domain: accounts, capabilities, qualifications and commercial profiles.

pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod ports;
pub mod repository;
pub mod service;

pub use application_core::PageView;
pub use dto::handover::{
    HandoverCandidateView, HandoverSupplierCapabilityRequest, HandoverSupplierCapabilityView,
    HandoverSupplierRequest, HandoverSupplierView,
};
pub use dto::supplier::{
    CommercialProfileView, RevealSupplierSensitiveRequest, SaveSupplierProfileRequest,
    SupplierCapabilityView, SupplierDetailView, SupplierListParams, SupplierProfileAddressInput,
    SupplierProfileBankAccountInput, SupplierProfileCapabilityOwnerInput, SupplierProfileContactInput,
    SupplierProfileMutationView, SupplierProfileQualificationInput, SupplierProfileRatingInput,
    SupplierQualificationHealth, SupplierQualificationView, SupplierRatingView, SupplierSensitiveFieldView,
    SupplierSensitiveRevealView, SupplierView,
};
pub use entity::supplier::{
    CapabilityCode, CapabilityStatus, InvoiceType, NewQualificationParams, PlannedQualificationInput,
    QualificationAttachmentSensitivity, QualificationStatus, QualificationType, ReconciliationCycle,
    SettlementMode, SupplierAccount, SupplierAccountData, SupplierAccountId, SupplierAccountStatus,
    SupplierAccountUpdate, SupplierCapability, SupplierCapabilityData, SupplierCapabilityId,
    SupplierCapabilityRevision, SupplierCapabilityRevisionData, SupplierCapabilityRevisionId,
    SupplierCapabilityUpdate, SupplierCommercialProfileRevision, SupplierCommercialProfileRevisionData,
    SupplierCommercialProfileRevisionId, SupplierCreationIds, SupplierCreationInputs, SupplierCreationPlan,
    SupplierCreationQualificationIds, SupplierCreationQualificationInput, SupplierCreationRatingInput,
    SupplierPartySeed, SupplierPaymentTerm, SupplierProfileChangePlan, SupplierProfileCommand,
    SupplierProfileCommandData, SupplierProfileUpdateViolation, SupplierQualification,
    SupplierQualificationCapability, SupplierQualificationCapabilityData, SupplierQualificationData,
    SupplierQualificationId, SupplierQualificationRevision, SupplierQualificationRevisionData,
    SupplierQualificationSelection, SupplierQualificationUpdate, SupplierRating, SupplierRatingRevision,
    SupplierRatingRevisionData, SupplierRatingRevisionId, apply_qualification_input, new_capability,
    new_qualification, next_supplier_revision_no, plan_commercial_profile_revision, plan_supplier_creation,
    profile_change, qualification_identity_key, split_encoded_payment_term_snapshot,
    validate_profile_selection,
};
pub use error::{Error, Result, known_duplicate_index_message};
pub use ports::{
    AccountFactPort, AddressTypeFact, EffectiveRecordStatusFact, EmptyFileAssetFacts, EmptyPartyFacts,
    EmptySensitiveTokens, FailClosedAccountFactPort, FailClosedSupplierDataScopePort, FileAssetFact,
    FileAssetFactsPort, PartyAddressFact, PartyBankAccountFact, PartyContactFact, PartyFactsPort,
    PartyListFact, PartyRevisionFact, PartyStatusFact, PartyTaxProfileFact, SensitiveFieldKindFact,
    SensitiveTokenPort, SupplierDataScopePort, SupplierResolvedClause, SupplierResolvedScope,
    SupplierScopeObject, select_current_default,
};
pub use repository::{
    SupplierAccountFilter, SupplierAccountRepository, SupplierAccountRow, SupplierCapabilityFilter,
    SupplierCapabilityRepository, SupplierCommercialProfileFilter,
    SupplierCommercialProfileRevisionRepository, SupplierDetailBundle, SupplierExt, SupplierListBundle,
    SupplierListSearchInput, SupplierProfileCommandRepository, SupplierQualificationCapabilityRepository,
    SupplierQualificationFilter, SupplierQualificationHealthFilter, SupplierQualificationRepository,
    SupplierReadScope, SupplierRepository, SupplierScopeClause,
};
pub use service::supplier::eligibility::ensure_capability_qualified;
pub use service::supplier::{
    SupplierAccess, SupplierListView, SupplierService, capability_handover_audit_id,
    capability_handover_fingerprint, command_view, supplier_handover_audit_id,
    supplier_handover_audit_message, supplier_handover_fingerprint, supplier_handover_fingerprint_matches,
    supplier_scope,
};
