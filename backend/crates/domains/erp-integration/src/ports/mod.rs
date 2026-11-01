//! 集成领域消费的权威事实端口。

pub mod data_scope;
pub mod evidence;

pub use data_scope::{
    FailClosedIntegrationDataScopePort, IntegrationDataScopePort, IntegrationResolvedClause,
    IntegrationResolvedScope, IntegrationScopeObject,
};
