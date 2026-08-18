//! 索引聚合器：每域一个 `indexes/<domain>.rs` 文件，各自提供
//! `pub(crate) async fn ensure(db: &Database) -> Result<()>`。
//!
//! 本文件 P0 后冻结：新增域在 `indexes/<domain>.rs` 内实现自己的 `ensure` 并加入下方顺序调用。

mod access_control;
mod approval;
mod approval_integration;
mod bpm;
mod bulk_job;
mod card_instance;
mod catalog;
mod contract;
mod cost;
mod customer;
mod document_registry;
mod file_asset;
mod fulfillment;
mod integration_ops;
mod inventory;
mod legacy_import;
mod mall_after_sales;
mod mall_backfill;
mod mall_order;
mod mall_sync;
mod party;
mod payable;
mod projection;
mod publication;
mod purchase_order;
mod receivable;
mod returns;
mod sales_order;
mod sales_review;
mod source_registry;
mod supplier;
mod supplier_api;
mod supplier_fulfillment;
mod supplier_offering;
mod supplier_settlement;
mod warehouse;
mod work_item;

/// 创建全部域依赖的幂等命名索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub async fn ensure_indexes(db: &mongodb::Database) -> crate::Result<()> {
    access_control::ensure(db).await?;
    approval::ensure(db).await?;
    approval_integration::ensure(db).await?;
    bpm::ensure(db).await?;
    bulk_job::ensure(db).await?;
    card_instance::ensure(db).await?;
    catalog::ensure(db).await?;
    contract::ensure(db).await?;
    cost::ensure(db).await?;
    customer::ensure(db).await?;
    document_registry::ensure(db).await?;
    file_asset::ensure(db).await?;
    fulfillment::ensure(db).await?;
    integration_ops::ensure(db).await?;
    inventory::ensure(db).await?;
    legacy_import::ensure(db).await?;
    mall_after_sales::ensure(db).await?;
    mall_backfill::ensure(db).await?;
    mall_order::ensure(db).await?;
    mall_sync::ensure(db).await?;
    party::ensure(db).await?;
    payable::ensure(db).await?;
    projection::ensure(db).await?;
    publication::ensure(db).await?;
    purchase_order::ensure(db).await?;
    receivable::ensure(db).await?;
    returns::ensure(db).await?;
    sales_order::ensure(db).await?;
    sales_review::ensure(db).await?;
    source_registry::ensure(db).await?;
    supplier::ensure(db).await?;
    supplier_api::ensure(db).await?;
    supplier_offering::ensure(db).await?;
    supplier_fulfillment::ensure(db).await?;
    supplier_settlement::ensure(db).await?;
    warehouse::ensure(db).await?;
    work_item::ensure(db).await?;
    Ok(())
}
