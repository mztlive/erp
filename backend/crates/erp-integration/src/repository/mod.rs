//! 集成域查询投影与本域持久化。

mod extensions;
pub mod integration_ops;
pub mod owned;

pub use extensions::IntegrationOpsExt;

#[cfg(test)]
mod serialization_contract;
