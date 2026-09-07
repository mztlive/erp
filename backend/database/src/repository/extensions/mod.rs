//! `DatabaseExt` 超特征聚合器：每域一个 `extensions/<domain>.rs` 文件。
//!
//! 本文件 P0 后冻结：新增域的访问器一律写在自己的 `extensions/<domain>.rs`，
//! 并通过把 trait 加进 supertrait 列表与本文件里的聚合 trait 生效，聚合 trait 本身不再改。

mod fulfillment;
mod integration_ops;

mod returns;

mod supplier_api;
mod supplier_fulfillment;
mod supplier_offering;
mod supplier_settlement;

pub use fulfillment::FulfillmentExt;
pub use integration_ops::IntegrationOpsExt;

pub use returns::ReturnsExt;
pub use supplier_api::SupplierApiExt;
pub use supplier_fulfillment::SupplierFulfillmentExt;
pub use supplier_offering::SupplierOfferingExt;
pub use supplier_settlement::SupplierSettlementExt;

/// Database 的统一仓储访问入口：聚合全部 34 个域的访问器 trait。
///
/// 各域在 `extensions/<domain>.rs` 中扩展自己的访问器方法；调用点（`db.accounts()` 等）
/// 签名保持不变。
pub trait DatabaseExt:
    erp_identity::AccessControlExt
    + erp_audit::AuditExt
    + erp_workflow::ApprovalIntegrationExt
    + erp_workflow::BpmExt
    + erp_support::BulkJobExt
    + erp_catalog::CatalogExt
    + erp_contract::ContractExt
    + erp_finance::repository::CostExt
    + erp_customer::CustomerExt
    + erp_workflow::DocumentRegistryExt
    + erp_support::FileAssetExt
    + FulfillmentExt
    + IntegrationOpsExt
    + erp_inventory::InventoryExt
    + erp_import::LegacyImportExt
    + erp_party::PartyExt
    + erp_finance::repository::PayableExt
    + erp_procurement::repository::ProcurementResponsibilityExt
    + erp_procurement::repository::PurchaseOrderExt
    + erp_finance::repository::ReceivableExt
    + ReturnsExt
    + erp_sales::repository::SalesOrderExt
    + erp_sales::repository::SalesReviewExt
    + erp_support::SourceRegistryExt
    + erp_supplier::SupplierExt
    + SupplierApiExt
    + SupplierOfferingExt
    + SupplierFulfillmentExt
    + SupplierSettlementExt
    + erp_warehouse::WarehouseExt
    + erp_workflow::WorkItemExt
{
}

impl<
        T: erp_identity::AccessControlExt
            + erp_audit::AuditExt
            + erp_workflow::ApprovalIntegrationExt
            + erp_workflow::BpmExt
            + erp_support::BulkJobExt
            + erp_catalog::CatalogExt
            + erp_contract::ContractExt
            + erp_finance::repository::CostExt
            + erp_customer::CustomerExt
            + erp_workflow::DocumentRegistryExt
            + erp_support::FileAssetExt
            + FulfillmentExt
            + IntegrationOpsExt
            + erp_inventory::InventoryExt
            + erp_import::LegacyImportExt
            + erp_party::PartyExt
            + erp_finance::repository::PayableExt
            + erp_procurement::repository::ProcurementResponsibilityExt
            + erp_procurement::repository::PurchaseOrderExt
            + erp_finance::repository::ReceivableExt
            + ReturnsExt
            + erp_sales::repository::SalesOrderExt
            + erp_sales::repository::SalesReviewExt
            + erp_support::SourceRegistryExt
            + erp_supplier::SupplierExt
            + SupplierApiExt
            + SupplierOfferingExt
            + SupplierFulfillmentExt
            + SupplierSettlementExt
            + erp_warehouse::WarehouseExt
            + erp_workflow::WorkItemExt,
    > DatabaseExt for T
{
}
