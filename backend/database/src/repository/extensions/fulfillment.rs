//! 域 D16 `fulfillment`：purchase_receipt(+_line)、delivery(+_line)、electronic_delivery、service_fulfillment、customer_acceptance(+_line)、acceptance_fulfillment_allocation（页面：W06、W09）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D16 仓储访问器（P2 填充）。
pub trait FulfillmentExt: Sized {}

impl FulfillmentExt for mongodb::Database {}
