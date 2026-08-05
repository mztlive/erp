//! 域 D14 `sales_review`：sales_order_review、procurement_confirmation(+_line)、
//! sales_change_order、sales_change_submission(+_line)、sales_change_review
//! （页面：W05、W07）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元。
//! 字段字典与唯一约束见数据模型 §6.5；公共字段归属按 §4.3 判定：
//! - 审批/确认/变更对象都有固定状态机（待处理/通过/驳回/失效）→ 组合
//!   [`crate::common::stable::StableBase`]；`procurement_confirmation_line` 与
//!   变更提交行是对象的组成部分，只用 `BaseModel` 持久化元数据。
//! - 变更提交内联与销售提交相同的客户/合同/结算/付款/开票结构化快照（§4.4 /
//!   P1 §2.2），禁止 JSON blob。
//!
//! 快照值对象、行字段组与公共枚举在 D13 有同形副本（`common/**` P0 冻结，
//! P1 §3 跨域约束），待 `chore/erp-p0-amend-*` 地基修订统一收口到
//! `entities/src/common/`。采购二次确认的唯一状态源是本域 `procurement_confirmation`，
//! 不重复写成 `sales_order_review.review_stage`（§6.5）。

pub mod procurement_confirmation;
pub mod sales_change_order;
pub mod sales_change_review;
pub mod sales_change_submission;
pub mod sales_order_review;
pub mod snapshot;
pub mod types;

pub use procurement_confirmation::{
    ProcurementConfirmation, ProcurementConfirmationData, ProcurementConfirmationLine,
    ProcurementConfirmationLineData, ProcurementConfirmationStatus, ProcurementRejectReasonCode,
};
pub use sales_change_order::{
    SalesChangeOrder, SalesChangeOrderData, SalesChangeOrderStatus, SalesChangeOrderUpdate, SalesChangeType,
};
pub use sales_change_review::{SalesChangeReview, SalesChangeReviewData, SalesChangeReviewStage};
pub use sales_change_submission::{
    SalesChangeSubmission, SalesChangeSubmissionData, SalesChangeSubmissionLine,
    SalesChangeSubmissionLineData,
};
pub use sales_order_review::{SalesOrderReview, SalesOrderReviewData, SalesReviewStage, SalesReviewStatus};
pub use snapshot::{
    ContractSnapshot, CustomerSnapshot, HeaderSnapshotData, HeaderSnapshots, InvoiceRequirementSnapshot,
    PaymentTermSnapshot, SettlementPartySnapshot,
};
pub use types::{
    BusinessType, CardForm, FulfillmentMode, GoodsLineFields, LineType, VoucherLineDraft, VoucherLineFields,
    WelfareScenario,
};

/// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use crate::ids::{
    ProcurementConfirmationId, ProcurementConfirmationLineId, SalesChangeOrderId, SalesChangeReviewId,
    SalesChangeSubmissionId, SalesChangeSubmissionLineId, SalesOrderReviewId,
};
