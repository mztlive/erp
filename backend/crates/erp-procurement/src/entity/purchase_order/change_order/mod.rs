//! `purchase_change_order` / `purchase_change_submission`(+line)（数据模型 §6.6）。
//!
//! 本模块同时含变更单、变更提交与变更行。采购变更单只适用于实物与服务销售单
//! （phase-1 §6.3）；已入库、已付款和已形成发票的事实不回退，生效事务把已通过
//! 复核的目标提交原样复制为新采购版本、版本行和销售分配（§6.6 必需约束，P3 编排）。

mod line;
mod order;
mod submission;

pub use line::{PurchaseChangeSubmissionLine, PurchaseChangeSubmissionLineData};
pub use order::{
    PurchaseChangeOrder, PurchaseChangeOrderData, PurchaseChangeOrderStatus, PurchaseChangeOrderUpdate,
};
pub use submission::{PurchaseChangeSubmission, PurchaseChangeSubmissionData};
