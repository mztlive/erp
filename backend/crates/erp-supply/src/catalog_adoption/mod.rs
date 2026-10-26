//! 供应商品库选品的冻结预览、逐项命令与原子结果。

mod media;
mod model;
mod repository;

pub use model::*;
pub use repository::{CatalogAdoptionExt, ensure_indexes};
