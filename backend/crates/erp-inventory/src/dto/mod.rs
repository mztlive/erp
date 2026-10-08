//! 供处理器与流程复用的库存 HTTP/应用 DTO。

pub mod inventory;
pub mod list;
pub mod scope;

pub use inventory::*;
pub use list::*;
pub use scope::{InventoryListPage, ensure_scope_version, normalized_scope_version, normalized_user_ids};
