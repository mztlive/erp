//! Customer MongoDB repositories and accessors.

pub mod customer;
pub mod extensions;
pub mod owned;
pub mod scope;

pub use customer::{
    CustomerAccountFilter, CustomerAccountRow, CustomerAssignmentFilter, CustomerAssignmentRow,
};
pub use extensions::CustomerExt;
pub use owned::{CustomerAccountRepository, CustomerAssignmentRepository, CustomerProfileCommandRepository};
pub use scope::{CustomerReadScope, CustomerScopeClause, CustomerVersion};
