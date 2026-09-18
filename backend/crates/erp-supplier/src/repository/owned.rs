//! Collection-scoped aliases for [`persistence_core::Repository`].
//!
//! Domain-specific methods are extension traits on the generic repository.

pub type SupplierAccountRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier::SupplierAccount>;
pub type SupplierCapabilityRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier::SupplierCapability>;
pub type SupplierCommercialProfileRevisionRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier::SupplierCommercialProfileRevision>;
pub type SupplierProfileCommandRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier::SupplierProfileCommand>;
pub type SupplierQualificationRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier::SupplierQualification>;
pub type SupplierQualificationCapabilityRepository<'a> =
    persistence_core::Repository<'a, crate::entity::supplier::SupplierQualificationCapability>;
