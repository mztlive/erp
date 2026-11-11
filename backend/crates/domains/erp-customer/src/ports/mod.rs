//! 审计持久化与外部主体事实的消费端口。

mod audit;
mod party;

pub use audit::{CustomerAuditPort, FailClosedAuditPort, PreparedCustomerAudit, ValidatedAuditSnapshot};
pub use party::{FailClosedPartyFactPort, PartyFactPort, PartyIdentityFact};
