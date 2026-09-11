//! 聚合提供方引用各领域的真实集合名与通用排序。
use erp_catalog::repository::CatalogExt;
use erp_supply::repository::SupplierOfferingExt;
use mongodb::bson::{doc, Document};

pub(super) const PRODUCT_REVISIONS: &str = <mongodb::Database as CatalogExt>::PRODUCT_REVISIONS;
pub(super) const SKUS: &str = <mongodb::Database as CatalogExt>::SKUS;
pub(super) const SKU_REVISIONS: &str = <mongodb::Database as CatalogExt>::SKU_REVISIONS;
pub(super) const SUPPLIER_OFFERINGS: &str = <mongodb::Database as SupplierOfferingExt>::SUPPLIER_OFFERINGS;

/// 主排序字段后追加 `id`，避免同一秒批量写入时 skip/limit 分页重叠。
pub(super) fn sort_doc(field: &str, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    doc! { field: direction, "id": direction }
}

#[cfg(test)]
mod tests {
    use erp_supply::entity::supplier_offering::{AvailabilityStatus, OfferingStatus};
    use mongodb::bson::doc;

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

    #[test]
    fn sort_doc_uses_id_as_stable_tiebreaker() {
        assert_eq!(
            super::sort_doc("created_at", false),
            doc! { "created_at": -1, "id": -1 }
        );
        assert_eq!(
            super::sort_doc("product_no", true),
            doc! { "product_no": 1, "id": 1 }
        );
    }
}
