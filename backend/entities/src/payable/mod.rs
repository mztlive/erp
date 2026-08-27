//! 域 D19 `payable`：payable_account、payable_entry、payable_entry_offset、
//! supplier_payment、payment_allocation、purchase_invoice_allocation（页面：W12）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 common 基元。
//! 公共字段归属按 §4.3 判定：
//! - `payable_account` 是账户主表类 → 组合 [`crate::common::stable::StableBase`]；
//! - 其余表是正式事实（§4.5 不设业务软删除，冲正用反向事实），按 §6.9 字段字典
//!   建模；§6.9 字典未含 FactBase 全部语义字段（fact_no/occurred_at/recorded_at/
//!   recorded_by/source_type/source_reference/reason_code/reason_text），因此不
//!   组合 FactBase，仅用 `BaseModel` 持久化元数据；
//! - 付款执行状态机固定为 `DRAFT→POSTED→REVERSED`，不存在付款审批状态；
//! - `invoice` 表归属 D18，本域仅通过 `entities::ids::InvoiceId` 引用。

pub mod payable_account;
pub mod payable_entry;
pub mod payable_entry_offset;
pub mod payment_allocation;
pub mod purchase_invoice_allocation;
pub mod supplier_payment;

pub use payable_account::*;
pub use payable_entry::*;
pub use payable_entry_offset::*;
pub use payment_allocation::*;
pub use purchase_invoice_allocation::*;
pub use supplier_payment::*;
