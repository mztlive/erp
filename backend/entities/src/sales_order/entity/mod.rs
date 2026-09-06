//! `sales_order` 与 `sales_order_line`（数据模型 §6.4 / W05）。
//!
//! `sales_order` 保存当前状态与当前版本指针，生效内容写不可变 `sales_order_revision`
//! （§4.4）；正式主状态 `commercial_status` 仅保存 `DRAFT / PENDING_REVIEW /
//! EFFECTIVE / VOIDED` 4 值（§7.1），审核环节值一律在 `review_status` 审核轨。
//! 卡券销售单与实物及服务销售单必须使用本表，不得增加平行销售单主表。

mod line;
mod order;
mod status;

pub use line::{SalesOrderLine, SalesOrderLineData};
pub use order::{SalesOrder, SalesOrderData, SalesOrderUpdate};
pub use status::{
    CloseStatus, CollectionProgress, CommercialStatus, FulfillmentProgress, InvoiceProgress, LineStatus,
    ReviewStatus,
};

#[cfg(test)]
mod tests;
