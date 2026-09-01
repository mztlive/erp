//! 域 D15 `purchase_order`：purchase_order、purchase_order_submission(+line)、
//! purchase_order_revision(+line)、purchase_line_sales_allocation、purchase_change_order、
//! purchase_change_submission(+line)（页面：W08）。
//!
//! 字段字典与唯一约束见数据模型 §6.6；公共字段归属按 §4.3 判定：
//! - `purchase_order` / `purchase_change_order` 是可编辑单据草稿 → 组合
//!   [`crate::common::StableBase`]，主状态机见 §7.4；
//! - `purchase_order_submission` / `purchase_change_submission` 是不可变提交，
//!   字段按 §6.6 字典精确建模（`submission_no`、`submitted_at`/`submitted_by`），
//!   不套用 FactBase（无 `fact_no`/`occurred_at`/`recorded_at` 语义字段）；
//! - `purchase_order_revision`(+line) 是不可变生效版本 → 组合
//!   [`crate::common::RevisionBase`]，并按 §4.4 内联结构化快照
//!   （供应商名称、付款条件门禁、商品名称、规格、单位）；
//! - `purchase_line_sales_allocation` 是分配明细，按 §6.6 字典精确建模。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元；
//! 跨聚合校验（超行量分配、逐行引用采购确认、表头行汇总守恒）留给 P3，条目见各文件注释。

mod allocation;
mod change_order;
mod command_receipt;
mod coverage;
mod creation_basis;
mod draft_edit;
mod line_amounts;
mod line_common;
mod order;
mod purchase_revision;
mod purchase_submission;
mod snapshot;
mod sourcing_plan;
mod types;

pub use allocation::{
    CurrentSalesAllocationLine, CurrentSalesAllocationPlan, CurrentSalesAllocationPlanError,
    PurchaseLineSalesAllocation, PurchaseLineSalesAllocationData,
};
pub use change_order::{
    PurchaseChangeOrder, PurchaseChangeOrderData, PurchaseChangeOrderStatus, PurchaseChangeOrderUpdate,
    PurchaseChangeSubmission, PurchaseChangeSubmissionData, PurchaseChangeSubmissionLine,
    PurchaseChangeSubmissionLineData,
};
pub use command_receipt::{
    digest_parts, payload_fingerprint, LegacyReceiptIdScheme, PurchaseCommandReceipt,
    PurchaseCommandReceiptError, PurchaseCommandReceiptIdentity, PurchaseReceiptWire,
};
pub use coverage::{
    build_procurement_coverage, ProcurementCoverageFacts, SalesProcurementCoverage,
    SalesProcurementCoverageLine,
};
pub use creation_basis::{
    basis_id_for, basis_scope_key, compose_basis_id, fulfillment_options, maximum_create_quantity,
    normalize_requested_lines, purchase_type_from_product_kind, stable_line_id, supply_cost, BasisGroup,
    BasisLine, BasisScope, CreationBasisFacts, LineSupply, RequestedLine,
};
pub use draft_edit::{validate_draft_line_edits, DraftLineEdit, DraftLineEditViolation};
pub use line_amounts::{compute_header_totals, LineAmountViolation, PurchaseLineInput};
pub use order::{
    ProgressStatus, PurchaseOrder, PurchaseOrderData, PurchaseOrderStatus, PurchaseOrderUpdate,
    PurchaseReviewStatus,
};
pub use purchase_revision::{
    PurchaseOrderRevision, PurchaseOrderRevisionData, PurchaseOrderRevisionLine,
    PurchaseOrderRevisionLineData,
};
pub use purchase_submission::{
    PurchaseOrderReviewDecision, PurchaseOrderSubmission, PurchaseOrderSubmissionData,
    PurchaseOrderSubmissionLine, PurchaseOrderSubmissionLineData, PurchaseOrderSubmissionUpdate,
    SubmissionStatus,
};
pub use snapshot::{PaymentTermSnapshot, SupplierSnapshot};
pub use sourcing_plan::{
    stock_basis_id_for, RequestedStockLine, SourcingAssignment, SourcingAssignmentSet, SourcingDraftPlan,
    SourcingPlan, SourcingPlanError, StockAllocationPlan, StockBasisGroup, StockBasisLine, SupplySourceType,
};
pub use types::{FulfillmentResponsibility, PurchaseLineType, PurchaseType};
