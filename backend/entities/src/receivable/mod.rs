//! 域 D18 `receivable`：receivable_account、receivable_entry、receivable_funds_review、
//! receivable_entry_offset、customer_receipt、receipt_allocation、invoice、
//! sales_invoice_allocation（页面：W11、W13）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 common 基元。
//! 公共字段归属按 §4.3 判定：
//! - `receivable_account` / `invoice` 是账户与主表类 → 组合 [`crate::common::stable::StableBase`]；
//! - 其余表是正式事实（§4.5 不设业务软删除，冲正用反向事实），按 §6.8 字段字典建模；
//!   §6.8 字典未含 FactBase 全部语义字段（fact_no/occurred_at/recorded_at/
//!   recorded_by/source_type/source_reference/reason_code/reason_text），因此不组合
//!   FactBase，仅用 `BaseModel` 持久化元数据；
//! - 状态机 §7.5 / 合同 §4.4.2 覆盖回款（DRAFT→IN_APPROVAL→POSTED→REVERSED）；发票签署为
//!   `NO_APPROVAL`，不得新增审批绑定字段或审批状态机；发票与账户状态是固定枚举，数据模型
//!   第 7 章未定义其状态机，不发明（§13.3）。

pub mod card_funds_receipt;
pub mod card_funds_review_decision;
pub mod customer_receipt;
pub mod funds_ledger;
pub mod funds_review_chain;
pub mod funds_snapshot;
pub mod invoice;
pub mod receipt_allocation;
pub mod receivable_account;
pub mod receivable_entry;
pub mod receivable_entry_offset;
pub mod receivable_funds_review;
pub mod sales_change_delta;
pub mod sales_invoice_allocation;
pub mod sales_invoice_allocation_plan;

pub use card_funds_receipt::{
    CardFundsCommandFollowUp, CardFundsCommandReceipt, CardFundsCommandReceiptData,
    CardFundsCommandReceiptError, CardFundsCommandReceiptVersion, CardFundsRegistrationKind,
    CardFundsRegistrationReceipt, CardFundsRegistrationReceiptError, CARD_FUNDS_INVOICE_REGISTRATION_ACTION,
    CARD_FUNDS_RECEIPT_REGISTRATION_ACTION, CARD_FUNDS_REVIEW_ACTION,
};
pub use card_funds_review_decision::{
    CardFundsReviewConclusion as EntityCardFundsReviewConclusion, CardFundsReviewEvidence,
    CardFundsReviewResult as EntityCardFundsReviewResult, CardFundsReviewType as EntityCardFundsReviewType,
    ValidatedCardFundsReviewDecision,
};
pub use customer_receipt::*;
pub use funds_ledger::ReceivableFundsLedger;
pub use funds_review_chain::ReceivableFundsReviewChain;
pub use funds_snapshot::ReceivableFundsSnapshot;
pub use invoice::*;
pub use receipt_allocation::*;
pub use receivable_account::*;
pub use receivable_entry::*;
pub use receivable_entry_offset::*;
pub use receivable_funds_review::*;
pub use sales_change_delta::ReceivableDelta;
pub use sales_invoice_allocation::*;
pub use sales_invoice_allocation_plan::*;
