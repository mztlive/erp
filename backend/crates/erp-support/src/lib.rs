//! Support domain: source registry, bulk jobs and file assets.

pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod ports;
pub mod repository;
pub mod service;

pub use application_core::PageView;
pub use dto::bulk_job::{
    BackgroundJobItemView, BackgroundJobListParams, BackgroundJobView, BulkSelectionItemView,
    BulkSelectionSnapshotListParams, BulkSelectionSnapshotView, CancelAllBackgroundJobsRequest,
    CancelAllBackgroundJobsResponse, CancelBackgroundJobFailure, CancelBackgroundJobRequest,
    ConfirmBulkSelectionSnapshotRequest, CreateBackgroundJobItemRequest, CreateBackgroundJobRequest,
    CreateBulkSelectionItemRequest, CreateBulkSelectionSnapshotRequest, ExpireBulkSelectionSnapshotRequest,
};
pub use dto::file_asset::{
    AttachToDocumentRequest, DestroyFileAssetRequest, DocumentAttachmentView, FileAssetListItemView,
    FileAssetListParams, FileAssetView, MarkScanResultRequest, PendingFileAssetRequest,
    RegisterFileAssetRequest,
};
pub use dto::source_registry::{
    CreateExternalIdentityMapRequest, CreateSourceSystemRequest, ExternalIdentityMapListParams,
    ExternalIdentityMapView, SourceSystemListParams, SourceSystemView, UpdateSourceSystemRequest,
};
pub use entity::bulk_job::{
    BackgroundJob, BackgroundJobAggregate, BackgroundJobAggregateData, BackgroundJobData, BackgroundJobId,
    BackgroundJobItem, BackgroundJobItemData, BackgroundJobItemDraft, BackgroundJobItemId, BulkSelectionItem,
    BulkSelectionItemData, BulkSelectionItemDraft, BulkSelectionSnapshot, BulkSelectionSnapshotAggregate,
    BulkSelectionSnapshotAggregateData, BulkSelectionSnapshotData, BulkSelectionSnapshotId, ItemStatus,
    JobStatus, JobType, JobUpdate, LEGACY_IMPORT_DOMAIN_JOB_TYPE, LEGACY_IMPORT_JOB_NO_PREFIX,
    PRODUCT_IMPORT_DOMAIN_JOB_TYPE, PRODUCT_IMPORT_JOB_NO_PREFIX, SUPPLIER_CATALOG_SYNC_JOB_TYPE,
    SUPPLIER_HEALTH_CHECK_JOB_TYPE, SelectionItemStatus, SelectionStatus, SelectionType,
    SupplierGovernanceJobKind, SupplierGovernanceJobSpec, legacy_import_job_no, product_import_job_no,
};
pub use entity::file_asset::{
    AttachmentUsage, BankReceiptEvidencePolicy, ContentHmac, DocumentAttachment, DocumentAttachmentData,
    DocumentAttachmentId, FileAsset, FileAssetData, FileAssetId, PENDING_FILE_REFERENCE_PREFIX,
    PendingFileReference, PendingFileReferenceSet, RetentionClass, SecurityScanStatus, SensitivityClass,
    content_fingerprint,
};
pub use entity::source_registry::{
    ExternalIdKey, ExternalIdentityMap, ExternalIdentityMapData, ExternalIdentityMapId,
    ExternalIdentityTarget, ExternalIdentityTargetData, ExternalIdentityTargetId, ExternalObjectType,
    MappingStatus, RelationRole, SourceSystem, SourceSystemData, SourceSystemId, SourceSystemStatus,
    SourceSystemType, SourceSystemUpdate, TargetStatus,
};
pub use error::{Error, Result};
pub use ports::{
    BUSINESS_DOCUMENT_TYPE_CODES, BusinessDocumentPort, EmptyPendingAttachments, FailClosedAuditPort,
    FailClosedBusinessDocumentPort, PendingAttachmentBatch, PreparedSupportAudit, SupportAuditPort,
    is_business_document_type,
};
pub use repository::{
    BackgroundJobFilter, BackgroundJobItemRow, BackgroundJobRegistration, BulkJobExt, BulkJobRepository,
    BulkSelectionSnapshotFilter, DocumentAttachmentRepository, ExpireTargetsOutcome,
    ExternalIdentityMapFilter, ExternalIdentityMapRepository, ExternalIdentityTargetRepository, FileAssetExt,
    FileAssetFilter, FileAssetRepository, FileAssetRow, SourceRegistryExt, SourceRegistryRepository,
    SourceSystemFilter, SourceSystemRepository,
};
pub use service::bulk_job::BulkJobService;
pub use service::file_asset::FileAssetService;
pub use service::source_registry::SourceRegistryService;
