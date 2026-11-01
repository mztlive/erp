//! 授权目录池逐页成员证据与完整范围核对；未完成扫描不能推导撤回。

mod indexes;
mod model;
pub mod repository;

pub use indexes::ensure_indexes;
pub use model::{
    CatalogMembershipPage, CatalogMembershipScan, CatalogMembershipState, CatalogPoolMember, CatalogPoolScope,
};
