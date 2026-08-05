//! 域 D13 `sales_order`：sales_order(+_line)、sales_order_working_copy、sales_order_submission、sales_order_revision、goods_service_line_revision、voucher_line_revision（页面：W05）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D13 仓储访问器（P2 填充）。
pub trait SalesOrderExt: Sized {}

impl SalesOrderExt for mongodb::Database {}
