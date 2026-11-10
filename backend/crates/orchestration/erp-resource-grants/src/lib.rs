//! 独立自动读取授权编排；业务事实经拥有领域仓储读取。
mod facts;
mod process;
mod rules;
pub use process::{GrantReadiness, ResourceGrantProcess};

mod backfill;
pub mod management;
pub use management::{
    BackfillReport, BackfillRequest, ManualGrantRequest, RecomputeRequest, RevokeRequest, SourcesQuery,
};
