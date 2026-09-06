//! Foreign supply-collection names and BSON status codes used by sellable queries.
//!
//! Catalog repositories must not name other domains' collections as string
//! literals. These constants are the original `SupplierOfferingExt` values and
//! live outside `repository/` so boundary scans do not treat them as catalog-owned
//! collections. BSON status codes keep the original uppercase wire values.

/// `supplier_offering` collection name (company-pool eligibility).
pub const SUPPLIER_OFFERINGS: &str = "supplier_offerings";
/// `supplier_offering_revision` collection name (current offering revision).
pub const SUPPLIER_OFFERING_REVISIONS: &str = "supplier_offering_revisions";
/// `supplier_offering_availability` collection name (live availability projection).
pub const SUPPLIER_OFFERING_AVAILABILITIES: &str = "supplier_offering_availabilities";

/// Offering status codes consumed by sellable and listing pipelines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferingStatus {
    /// Offering may participate in sourcing.
    Active,
}

impl OfferingStatus {
    /// Return the persisted uppercase status code.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
        }
    }
}

/// Live availability status codes consumed by the sellable pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvailabilityStatus {
    /// Offering is currently available.
    Available,
}

impl AvailabilityStatus {
    /// Return the persisted uppercase availability code.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Available => "AVAILABLE",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AvailabilityStatus, OfferingStatus};

    #[test]
    fn supply_status_wire_values_stay_uppercase() {
        assert_eq!(OfferingStatus::Active.as_str(), "ACTIVE");
        assert_eq!(AvailabilityStatus::Available.as_str(), "AVAILABLE");
        assert_eq!(super::SUPPLIER_OFFERINGS, "supplier_offerings");
        assert_eq!(super::SUPPLIER_OFFERING_REVISIONS, "supplier_offering_revisions");
        assert_eq!(
            super::SUPPLIER_OFFERING_AVAILABILITIES,
            "supplier_offering_availabilities"
        );
    }
}
