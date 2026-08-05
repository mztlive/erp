//! 域 D29 `mall_order`：mall_order_fact、mall_order_cancel_fact、mall_order_completion_fact、mall_order、mall_order_item、mall_payment_source、mall_consumption_entry 等（页面：W25、W28）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D29 仓储访问器（P2 填充）。
pub trait MallOrderExt: Sized {}

impl MallOrderExt for mongodb::Database {}
