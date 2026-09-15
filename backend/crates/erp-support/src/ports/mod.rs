//! Consumer ports for audit persistence, business-document facts and pending attachments.

mod audit;
mod business_document;
mod pending;

pub use audit::{FailClosedAuditPort, PreparedSupportAudit, SupportAuditPort};
pub use business_document::{
    BUSINESS_DOCUMENT_TYPE_CODES, BusinessDocumentPort, FailClosedBusinessDocumentPort,
    is_business_document_type,
};
pub use pending::{EmptyPendingAttachments, PendingAttachmentBatch};
