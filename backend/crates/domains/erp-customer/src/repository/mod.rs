//! 客户 MongoDB 仓储与访问器。

pub mod customer;
pub mod customer_shared;
pub mod extensions;
pub mod owned;
pub mod prelude;
pub mod scope;

pub use customer::{
    CustomerAccountFilter, CustomerAccountRepositoryExt, CustomerAccountRow, CustomerAssignmentFilter,
    CustomerAssignmentRepositoryExt, CustomerAssignmentRow, CustomerProfileCommandRepositoryExt,
};
pub use extensions::CustomerExt;
pub use owned::{CustomerAccountRepository, CustomerAssignmentRepository, CustomerProfileCommandRepository};
pub use scope::{
    CustomerAccountRepositoryScopeExt, CustomerAssignmentRepositoryScopeExt, CustomerReadScope,
    CustomerScopeClause, CustomerVersion,
};
