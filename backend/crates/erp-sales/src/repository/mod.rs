//! 销售 MongoDB 仓储、查询事实与集合访问器。

pub mod extensions;
pub mod owned;
pub mod sales_order;
pub mod sales_review;

pub use extensions::{SalesOrderExt, SalesReviewExt};

#[cfg(test)]
mod serialization_contract;
