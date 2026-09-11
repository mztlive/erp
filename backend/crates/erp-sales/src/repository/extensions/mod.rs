//! 销售集合名与拥有仓储工厂的唯一合同。

mod sales_order;
mod sales_review;
mod sales_selection;

pub use sales_order::SalesOrderExt;
pub use sales_review::SalesReviewExt;
pub use sales_selection::SalesSelectionExt;
