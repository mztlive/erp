//! 合同 MongoDB 仓储与访问器。

pub mod contract;
pub mod extensions;
pub mod owned;
pub mod prelude;
pub mod scope;
pub mod templates;

pub use contract::{
    ContractDomainRepository, ContractFilter, ContractRepositoryExt, ContractRevisionRepositoryExt,
    ContractRow,
};
pub use extensions::ContractExt;
pub use owned::{ContractRepository, ContractRevisionRepository};
pub use scope::{ContractReadScope, ContractRepositoryScopeExt, ContractScopeClause};

pub mod list_search;

pub mod recognition;
