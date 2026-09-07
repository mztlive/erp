//! 聚合提供方引用各领域的真实集合名与通用排序。
use erp_catalog::repository::CatalogExt;
use erp_supply::repository::SupplierOfferingExt;
use mongodb::bson::{doc, Document};

pub(super) const PRODUCT_REVISIONS: &str = <mongodb::Database as CatalogExt>::PRODUCT_REVISIONS;
pub(super) const SKUS: &str = <mongodb::Database as CatalogExt>::SKUS;
pub(super) const SKU_REVISIONS: &str = <mongodb::Database as CatalogExt>::SKU_REVISIONS;
pub(super) const SUPPLIER_OFFERINGS: &str = <mongodb::Database as SupplierOfferingExt>::SUPPLIER_OFFERINGS;

/// 保持原单字段排序方向，不添加次键。
pub(super) fn sort_doc(field: &str, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    doc! { field: direction }
}

#[cfg(test)]
mod tests {
    use erp_supply::entity::supplier_offering::{AvailabilityStatus, OfferingStatus};

    #[test]
    fn supply_status_wire_values_stay_uppercase() {
        assert_eq!(OfferingStatus::Active.as_str(), "ACTIVE");
        assert_eq!(AvailabilityStatus::Available.as_str(), "AVAILABLE");
        assert_eq!(super::SUPPLIER_OFFERINGS, "supplier_offerings");
        assert_eq!(
            <mongodb::Database as erp_supply::repository::SupplierOfferingExt>::SUPPLIER_OFFERING_REVISIONS,
            "supplier_offering_revisions"
        );
        assert_eq!(
            <mongodb::Database as erp_supply::repository::SupplierOfferingExt>::SUPPLIER_OFFERING_AVAILABILITIES,
            "supplier_offering_availabilities"
        );
    }
}
