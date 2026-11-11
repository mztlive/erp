//! 采购拥有域提供的对象事实与查询字段映射；授权规则由 identity 登记并执行。

mod facts;
mod mapping;

pub use facts::PurchaseFacts;
pub use mapping::PurchaseMapping;
