//! `DatabaseExt` 超特征聚合器：每域一个 `extensions/<domain>.rs` 文件。
//!
//! 本文件 P0 后冻结：新增域的访问器一律写在自己的 `extensions/<domain>.rs`，
//! 并通过把 trait 加进 supertrait 列表与本文件里的聚合 trait 生效，聚合 trait 本身不再改。

mod access_control;
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
mod supplier_catalog;
mod supplier_fulfillment;
mod supplier_settlement;
mod warehouse;
mod work_item;

pub use access_control::AccessControlExt;
pub use bulk_job::BulkJobExt;
pub use card_instance::CardInstanceExt;
pub use catalog::CatalogExt;
pub use contract::ContractExt;
pub use cost::CostExt;
pub use customer::CustomerExt;
pub use document_registry::DocumentRegistryExt;
pub use file_asset::FileAssetExt;
pub use fulfillment::FulfillmentExt;
pub use integration_ops::IntegrationOpsExt;
pub use inventory::InventoryExt;
pub use legacy_import::LegacyImportExt;
pub use mall_after_sales::MallAfterSalesExt;
pub use mall_backfill::MallBackfillExt;
pub use mall_order::MallOrderExt;
pub use mall_sync::MallSyncExt;
pub use party::PartyExt;
pub use payable::PayableExt;
pub use projection::ProjectionExt;
pub use publication::PublicationExt;
pub use purchase_order::PurchaseOrderExt;
pub use receivable::ReceivableExt;
pub use returns::ReturnsExt;
pub use sales_order::SalesOrderExt;
pub use sales_review::SalesReviewExt;
pub use source_registry::SourceRegistryExt;
pub use supplier::SupplierExt;
pub use supplier_api::SupplierApiExt;
pub use supplier_catalog::SupplierCatalogExt;
pub use supplier_fulfillment::SupplierFulfillmentExt;
pub use supplier_settlement::SupplierSettlementExt;
pub use warehouse::WarehouseExt;
pub use work_item::WorkItemExt;

/// Database 的统一仓储访问入口：聚合全部 34 个域的访问器 trait。
///
/// 各域在 `extensions/<domain>.rs` 中扩展自己的访问器方法；调用点（`db.accounts()` 等）
/// 签名保持不变。
pub trait DatabaseExt:
    AccessControlExt
    + BulkJobExt
    + CardInstanceExt
    + CatalogExt
    + ContractExt
    + CostExt
    + CustomerExt
    + DocumentRegistryExt
    + FileAssetExt
    + FulfillmentExt
    + IntegrationOpsExt
    + InventoryExt
    + LegacyImportExt
    + MallAfterSalesExt
    + MallBackfillExt
    + MallOrderExt
    + MallSyncExt
    + PartyExt
    + PayableExt
    + ProjectionExt
    + PublicationExt
    + PurchaseOrderExt
    + ReceivableExt
    + ReturnsExt
    + SalesOrderExt
    + SalesReviewExt
    + SourceRegistryExt
    + SupplierExt
    + SupplierApiExt
    + SupplierCatalogExt
    + SupplierFulfillmentExt
    + SupplierSettlementExt
    + WarehouseExt
    + WorkItemExt
{
}

impl<
        T: AccessControlExt
            + BulkJobExt
            + CardInstanceExt
            + CatalogExt
            + ContractExt
            + CostExt
            + CustomerExt
            + DocumentRegistryExt
            + FileAssetExt
            + FulfillmentExt
            + IntegrationOpsExt
            + InventoryExt
            + LegacyImportExt
            + MallAfterSalesExt
            + MallBackfillExt
            + MallOrderExt
            + MallSyncExt
            + PartyExt
            + PayableExt
            + ProjectionExt
            + PublicationExt
            + PurchaseOrderExt
            + ReceivableExt
            + ReturnsExt
            + SalesOrderExt
            + SalesReviewExt
            + SourceRegistryExt
            + SupplierExt
            + SupplierApiExt
            + SupplierCatalogExt
            + SupplierFulfillmentExt
            + SupplierSettlementExt
            + WarehouseExt
            + WorkItemExt,
    > DatabaseExt for T
{
}
