//! ERP 共享内核：金额、稳定 ID、业务时间、校验原语与操作人类别。

pub mod common;
mod errors;
pub mod field_update;
pub mod identity;
pub mod ids;
pub mod money;
pub mod validation;

pub use errors::{Error, Result};
pub use field_update::FieldUpdate;
pub use identity::{AccountKind, InvalidAccountKind};
