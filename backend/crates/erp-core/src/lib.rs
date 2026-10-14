//! ERP 共享内核：金额、稳定 ID、业务时间、行政区划、配送政策、校验原语与操作人类别。

pub mod common;
pub mod delivery_policy;
mod errors;
pub mod field_update;
pub mod identity;
pub mod ids;
pub mod money;
pub mod region;
pub mod validation;

pub use errors::{Error, Result};
pub use field_update::FieldUpdate;
pub use identity::{AccountKind, InvalidAccountKind};
