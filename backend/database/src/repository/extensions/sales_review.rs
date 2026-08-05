//! 域 D14 `sales_review`：sales_order_review、procurement_confirmation(+_line)、sales_change_order、sales_change_submission、sales_change_review（页面：W05、W07）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D14 仓储访问器（P2 填充）。
pub trait SalesReviewExt: Sized {}

impl SalesReviewExt for mongodb::Database {}
