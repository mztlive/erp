//! 财务 MongoDB 仓储、查询事实与集合访问器。

use mongodb::bson::{Document, doc};

pub mod cost;
pub mod extensions;
mod fulfillment_facts;
pub mod owned;
pub mod payable;
pub mod prelude;
mod progress;
pub mod receivable;

pub use cost::{CostAllocationFilter, CostAllocationRow, CostEntryFilter, CostEntryRow, CostRepository};
pub use extensions::{CostExt, PayableExt, ReceivableExt};
pub use owned::{
    CostAllocationRepository, CostEntryRepository, CustomerReceiptRepository, InvoiceRepository,
    PayableAccountRepository, PayableEntryOffsetRepository, PayableEntryRepository,
    PaymentAllocationRepository, PurchaseInvoiceAllocationRepository, ReceiptAllocationRepository,
    ReceivableAccountRepository, ReceivableEntryOffsetRepository, ReceivableEntryRepository,
};
pub use payable::{
    PayableAccountFilter, PayableAccountRow, PayableRepository, PurchaseInvoiceAllocationFilter,
    SupplierPaymentFilter, SupplierPaymentRow,
};
pub use prelude::*;
pub use receivable::customer_center::CustomerCenterReceivableRow;
pub use receivable::{
    CustomerReceiptFilter, CustomerReceiptRow, InvoiceFilter, InvoiceRow, ReceivableAccountFilter,
    ReceivableListScope, ReceivableRepository, ScopedCustomerReceiptQuery, ScopedInvoiceQuery,
};

#[cfg(test)]
mod test_fixture;

#[cfg(test)]
mod serialization_contract;

pub mod keyword;

/// 构建带 `id` 稳定次序的排序文档：字段名经白名单映射，未命中回退 `created_at`。
///
/// # 参数
/// * `sort_by` - 排序字段（白名单内有效）
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
/// * `allowed` - 允许的排序字段名集合（防止透传任意字段名）
///
/// # 返回
/// 返回排序条件文档。
pub(super) fn sort_doc_with_id(sort_by: Option<&str>, sort_ascending: bool, allowed: &[&str]) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by.filter(|name| allowed.contains(name)).unwrap_or("created_at");
    doc! { field: direction, "id": direction }
}
