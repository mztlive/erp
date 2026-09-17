//! 集成域查询投影与本域持久化。

mod extensions;
pub mod integration_ops;
pub mod owned;
pub mod scope;

pub use extensions::IntegrationOpsExt;
pub use scope::{IntegrationReadScope, IntegrationScopeClause};

#[cfg(test)]
mod serialization_contract;
