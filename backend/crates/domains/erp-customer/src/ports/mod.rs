//! 审计持久化，以及外部主体、账号与范围事实的消费端口。

mod account;
mod audit;
mod data_scope;
mod party;

pub use account::{AccountFactPort, FailClosedAccountFactPort};
pub use audit::{CustomerAuditPort, FailClosedAuditPort, PreparedCustomerAudit, ValidatedAuditSnapshot};
pub use data_scope::{
    CustomerDataScopePort, CustomerResolvedClause, CustomerResolvedScope, CustomerScopeObject,
    FailClosedCustomerDataScopePort,
};
pub use party::{FailClosedPartyFactPort, PartyFactPort, PartyIdentityFact};
