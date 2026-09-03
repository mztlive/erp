//! 域 D13 `sales_order`：sales_order(+_line)、sales_order_working_copy(+_line)、
//! sales_order_submission(+_line)、sales_order_revision(+_line)、
//! goods_service_line_revision、voucher_line_revision（页面：W05）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元。
//! 字段字典与唯一约束见数据模型 §6.4；公共字段归属按 §4.3 判定：
//! - `sales_order` / `sales_order_working_copy` / `sales_order_submission` 是可编辑
//!   草稿与提交状态对象 → 组合 [`crate::common::stable::StableBase`]；
//! - `sales_order_revision` 是「不可变修订」→ 组合
//!   [`crate::common::revision::RevisionBase`]；
//! - 各 `*_line` 与子类型修订行是版本的组成部分，只用 `BaseModel` 持久化元数据
//!   （与 P0 `external_identity_map` 先例一致）。
//! - 正式版本与提交内联客户名称、合同编号、结算主体、税务、付款条件、商品名称、
//!   规格、单位等结构化快照（数据模型 §4.4 / P1 §2.2），禁止 JSON blob。
//!
//! 快照值对象与行字段组类型在 D12/D14/D15 有同形副本（`common/**` P0 冻结，
//! P1 §3 跨域约束），待 `chore/erp-p0-amend-*` 地基修订统一收口到
//! `entities/src/common/`。

mod amount_validation;
mod approval_quantity;
mod closure;
mod content_hash;
mod draft_working_copy;
mod entity;
pub(crate) mod formal_revision;
mod procurement;
pub mod revision;
pub mod snapshot;
pub mod submission;
mod submission_from_working_copy;
pub mod types;
pub mod working_copy;
mod working_copy_line;
#[cfg(test)]
mod working_copy_test_support;
mod working_copy_types;

pub use closure::{
    SalesOrderClosureAssessment, SalesOrderClosureFacts, SalesOrderClosureTerminal,
    SalesOrderFulfillmentBlocker,
};
pub use content_hash::SalesContentHash;
pub use entity::{
    CloseStatus, CollectionProgress, CommercialStatus, FulfillmentProgress, InvoiceProgress, LineStatus,
    ReviewStatus, SalesOrder, SalesOrderData, SalesOrderLine, SalesOrderLineData, SalesOrderUpdate,
};
pub use formal_revision::{
    FormalRevisionContext, FormalRevisionIdentities, FormalRevisionLineIdentity,
    FormalRevisionSubtypeIdentity, SalesOrderRevisionAggregate,
};
pub use procurement::{procurement_responsibility_key, ProcurementCoverageSummary};
pub use revision::{
    RevisionSource, SalesOrderGoodsServiceLineRevision, SalesOrderGoodsServiceLineRevisionData,
    SalesOrderRevision, SalesOrderRevisionData, SalesOrderRevisionLine, SalesOrderRevisionLineData,
    SalesOrderVoucherLineRevision, SalesOrderVoucherLineRevisionData,
};
pub use snapshot::{
    ContractSnapshot, CustomerSnapshot, HeaderSnapshotData, HeaderSnapshots, InvoiceRequirementSnapshot,
    PaymentTermSnapshot, SettlementPartySnapshot,
};
pub use submission::{
    SalesOrderSubmission, SalesOrderSubmissionData, SalesOrderSubmissionLine, SalesOrderSubmissionLineData,
    SubmissionStatus,
};
pub use types::{
    BusinessType, CardForm, ExternalIdentityResolution, GoodsLineFields, LineType, OriginSystem,
    VoucherLineDraft, VoucherLineFields, WelfareScenario,
};
pub use working_copy::{
    SalesOrderWorkingCopy, SalesOrderWorkingCopyData, SalesOrderWorkingCopyLine,
    SalesOrderWorkingCopyLineData, SalesOrderWorkingCopyUpdate, WorkingCopyStatus, WorkingPurpose,
};

/// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use crate::ids::{
    SalesOrderGoodsServiceLineRevisionId, SalesOrderId, SalesOrderLineId, SalesOrderRevisionId,
    SalesOrderRevisionLineId, SalesOrderSubmissionId, SalesOrderSubmissionLineId,
    SalesOrderVoucherLineRevisionId, SalesOrderWorkingCopyId, SalesOrderWorkingCopyLineId,
};
