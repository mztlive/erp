//! 域 D14 `sales_review`：销售变更单与变更提交。
//!
//! 采购二次确认、低毛利上级确认、卡券专用审批记录与旧变更复核实体已删除。
//! 销售变更走统一审批；选源由采购单创建路径承担。

mod formal_revision;
pub mod sales_change_order;
pub mod sales_change_submission;
pub mod snapshot;
pub mod types;

pub use sales_change_order::{
    SalesChangeOrder, SalesChangeOrderData, SalesChangeOrderStatus, SalesChangeOrderUpdate, SalesChangeType,
};
pub use sales_change_submission::{
    SalesChangeSubmission, SalesChangeSubmissionData, SalesChangeSubmissionLine,
    SalesChangeSubmissionLineData,
};
pub use snapshot::{
    ContractSnapshot, CustomerSnapshot, HeaderSnapshotData, HeaderSnapshots, InvoiceRequirementSnapshot,
    PaymentTermSnapshot, SettlementPartySnapshot,
};
pub use types::{
    BusinessType, CardForm, GoodsLineFields, LineType, VoucherLineDraft, VoucherLineFields, WelfareScenario,
};

/// 域内仍保留的 ID newtype 出口。
pub use crate::ids::{SalesChangeOrderId, SalesChangeSubmissionId, SalesChangeSubmissionLineId};
