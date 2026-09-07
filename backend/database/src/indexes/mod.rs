//! 索引聚合器：每域一个 `indexes/<domain>.rs` 文件，各自提供
//! `pub(crate) async fn ensure(db: &Database) -> Result<()>`。
//!
//! 本文件 P0 后冻结：新增域在 `indexes/<domain>.rs` 内实现自己的 `ensure` 并加入下方顺序调用。

mod cost;
mod fulfillment;
mod integration_ops;

mod payable;
mod procurement_responsibility;
mod purchase_order;
mod receivable;
mod returns;
mod sales_order;
mod sales_review;
mod supplier_api;
mod supplier_fulfillment;
mod supplier_offering;
mod supplier_settlement;

/// 创建全部域依赖的幂等命名索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub async fn ensure_indexes(db: &mongodb::Database) -> persistence_core::Result<()> {
    erp_identity::indexes::ensure(db).await?;
    erp_audit::indexes::ensure(db).await?;
    erp_workflow::indexes::ensure(db).await?;
    erp_support::indexes::ensure(db).await?;
    erp_catalog::indexes::ensure(db).await?;
    erp_contract::indexes::ensure(db).await?;
    cost::ensure(db).await?;
    erp_customer::indexes::ensure(db).await?;
    fulfillment::ensure(db).await?;
    integration_ops::ensure(db).await?;
    erp_inventory::indexes::ensure(db).await?;
    erp_import::indexes::ensure(db).await?;
    erp_party::indexes::ensure(db).await?;
    payable::ensure(db).await?;
    procurement_responsibility::ensure(db).await?;
    purchase_order::ensure(db).await?;
    receivable::ensure(db).await?;
    returns::ensure(db).await?;
    sales_order::ensure(db).await?;
    sales_review::ensure(db).await?;
    erp_supplier::indexes::ensure(db).await?;
    supplier_api::ensure(db).await?;
    supplier_offering::ensure(db).await?;
    supplier_fulfillment::ensure(db).await?;
    supplier_settlement::ensure(db).await?;
    erp_warehouse::indexes::ensure(db).await?;
    Ok(())
}
