//! 支撑领域消费的审计持久化、业务单据事实与待处理附件端口。

mod audit;
mod business_document;
mod pending;

pub use audit::{FailClosedAuditPort, PreparedSupportAudit, SupportAuditPort};
pub use business_document::{
    BUSINESS_DOCUMENT_TYPE_CODES, BusinessDocumentPort, FailClosedBusinessDocumentPort,
    is_business_document_type,
};
pub use pending::{EmptyPendingAttachments, PendingAttachmentBatch, PendingFileContent};
