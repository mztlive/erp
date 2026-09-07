//! 域 D33 `supplier_settlement` 仓储：supplier_settlement_statement、
//! supplier_settlement_item、supplier_settlement_difference（页面：W27）。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS）；本文件只补充域特有
//! 查询与跨集合多步骤写入入口。集合名常量统一取 `SupplierSettlementExt` 关联常量
//! （唯一权威来源，indexes 与 Repository 两侧共用）。
//!
//! 结算明细是不可变正式结算事实行（只 `new` 不 `update`，§6.20），本域不为它提供
//! 软删除方法；结算单是正式单据，仍走基类软删除/恢复语义。
//!
//! 筛选/行类型定义在本文件，经 `SupplierSettlementExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use super::extensions::{SupplierFulfillmentExt, SupplierSettlementExt};

pub mod detail_snapshot;

mod command;
mod difference;
mod item;
mod projection;
mod source_scope;
mod statement;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod wire_tests;

pub use command::SupplierSettlementRepository;
pub use detail_snapshot::SupplierSettlementStatementDetailSnapshot;
pub use difference::{SupplierSettlementDifferenceFilter, SupplierSettlementDifferenceRow};
pub use item::{SupplierSettlementItemFilter, SupplierSettlementItemRow};
pub use source_scope::SupplierSettlementSourceScope;
pub use statement::{
    SupplierSettlementStatementFilter, SupplierSettlementStatementRow, SupplierSettlementStatementStatsRow,
};

/// `supplier_settlement_statement` 集合名（单一来源：`SupplierSettlementExt` 关联常量）。
const SUPPLIER_SETTLEMENT_STATEMENTS: &str =
    <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_STATEMENTS;
/// `supplier_settlement_item` 集合名（单一来源：`SupplierSettlementExt` 关联常量）。
const SUPPLIER_SETTLEMENT_ITEMS: &str =
    <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_ITEMS;
const SUPPLIER_SETTLEMENT_DIFFERENCES: &str =
    <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_DIFFERENCES;
const SUPPLIER_SETTLEMENT_DIFFERENCE_EVIDENCE: &str =
    <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_DIFFERENCE_EVIDENCE;
/// `supplier_fulfillment_order` 集合名（FUL-R06 结算来源范围读取）。
const SUPPLIER_FULFILLMENT_ORDERS: &str =
    <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ORDERS;
/// `supplier_fulfillment_item` 集合名（FUL-R06 结算来源范围读取）。
const SUPPLIER_FULFILLMENT_ITEMS: &str =
    <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS;
/// `supplier_refund_fact` 集合名（FUL-R06 结算来源范围读取）。
const SUPPLIER_REFUND_FACTS: &str = <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_FACTS;
/// `supplier_refund_allocation` 集合名（FUL-R06 结算来源范围读取）。
const SUPPLIER_REFUND_ALLOCATIONS: &str =
    <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_ALLOCATIONS;

/// 结算单列表排序白名单（§6.20 查询索引支持的字段；白名单外一律回退 `created_at`）。
const STATEMENT_SORT_FIELDS: &[&str] = &["created_at", "period_start", "period_end", "confirmed_at"];
/// 结算明细列表排序白名单（白名单外一律回退 `created_at`）。
const ITEM_SORT_FIELDS: &[&str] = &["created_at", "erp_calculated_amount", "supplier_billed_amount"];
/// 结算差异列表排序白名单（白名单外一律回退 `created_at`）。
const DIFFERENCE_SORT_FIELDS: &[&str] = &["created_at", "difference_amount", "resolved_at"];
