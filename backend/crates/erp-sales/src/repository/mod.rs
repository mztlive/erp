//! 销售 MongoDB 仓储、查询事实与集合访问器。

pub mod extensions;
pub(crate) mod filter;
pub mod owned;
pub mod sales_order;
pub mod sales_review;
pub mod sales_selection;

pub use extensions::{SalesOrderExt, SalesReviewExt, SalesSelectionExt};

#[cfg(test)]
mod serialization_contract;

mod fulfillment_facts;
