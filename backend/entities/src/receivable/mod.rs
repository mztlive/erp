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
//! - 状态机 §7.5 只覆盖回款（DRAFT→PENDING_REVIEW→POSTED→REVERSED）；发票与账户
//!   状态是固定枚举，数据模型第 7 章未定义其状态机，不发明（§13.3）。

pub mod customer_receipt;
pub mod invoice;
pub mod receipt_allocation;
pub mod receivable_account;
pub mod receivable_entry;
pub mod receivable_entry_offset;
pub mod receivable_funds_review;
pub mod sales_invoice_allocation;

pub use customer_receipt::*;
pub use invoice::*;
pub use receipt_allocation::*;
pub use receivable_account::*;
pub use receivable_entry::*;
pub use receivable_entry_offset::*;
pub use receivable_funds_review::*;
pub use sales_invoice_allocation::*;
