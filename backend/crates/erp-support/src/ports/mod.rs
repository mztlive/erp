//! Consumer ports for audit persistence, business-document facts and pending attachments.

mod audit;
mod business_document;
mod pending;

pub use audit::{FailClosedAuditPort, PreparedSupportAudit, SupportAuditPort};
pub use business_document::{
    is_business_document_type, BusinessDocumentPort, FailClosedBusinessDocumentPort,
    BUSINESS_DOCUMENT_TYPE_CODES,
};
pub use pending::{EmptyPendingAttachments, PendingAttachmentBatch};
