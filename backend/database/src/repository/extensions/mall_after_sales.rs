//! 域 D30 `mall_after_sales`：mall_after_sales_request(+_line)、mall_refund(+_line)、mall_refund_allocation、mall_balance_restoration(+_allocation)（页面：W25）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D30 仓储访问器（P2 填充）。
pub trait MallAfterSalesExt: Sized {}

impl MallAfterSalesExt for mongodb::Database {}
