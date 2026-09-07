//! Import domain: legacy import batches, rows, confirmations and apply-result facts.

pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod ports;
pub mod repository;
pub mod service;

pub use application_core::PageView;
pub use dto::{
    optional_text, parse_command_version, parse_receipt_number, required_text, ApplyLegacyImportBatchRequest,
    ApplyRowOutcome, ApplyRowResult, CompleteImportBusinessConfirmationCommand,
    CreateLegacyImportBatchRequest, CreateLegacyImportConfirmationRequest,
    ImportBusinessConfirmationDecision, ImportBusinessConfirmationNextStep,
    ImportBusinessConfirmationResultStatus, ImportExecutionAction, ImportExecutionCommand,
    ImportExecutionNextStep, ImportExecutionResult, ImportExecutionResultStatus, ImportJobStatus,
    ImportRowRequest, LegacyImportBatchListItem, LegacyImportBatchListParams, LegacyImportBatchView,
    LegacyImportConfirmationListParams, LegacyImportRowListParams, LegacyImportRowView, PageParams,
    PreparedConfirmationCompletion, PreparedImportExecution, SortDir, CUSTOMER_NOT_FOUND_ERROR_CODE,
    CUSTOMER_NOT_FOUND_ERROR_DETAIL, CUSTOMER_OBJECT_TYPE,
};
pub use entity::legacy_import::{
    build_import_rows, ApplyResultDraft, ApplyResultItem, ApplyResultOutcome, ApplyResultSet,
    ConfirmationDecision, ConfirmationMatrixDecision, ConfirmationScope, ConfirmationStatus,
    ExternalIdentityMapId, FileAssetId, ImportRowSpec, ImportStatus, LegacyImportBatch,
    LegacyImportBatchData, LegacyImportBatchId, LegacyImportBatchStatus, LegacyImportCommandIdentity,
    LegacyImportConfirmation, LegacyImportConfirmationData, LegacyImportConfirmationId, LegacyImportRow,
    LegacyImportRowData, LegacyImportRowId, MappingStatus, ParseStatus, SourceSystemId, WorkItemId,
};
pub use error::{Error, Result};
pub use ports::{BulkJobFactsPort, FailClosedBulkJobFacts};
pub use repository::{
    LegacyImportApplyScope, LegacyImportBatchFilter, LegacyImportBatchRepository, LegacyImportBatchRow,
    LegacyImportConfirmationFilter, LegacyImportConfirmationRepository, LegacyImportConfirmationRow,
    LegacyImportExt, LegacyImportRepository, LegacyImportRowFilter, LegacyImportRowRepository,
    LegacyImportRowRow,
};
pub use service::LegacyImportService;
