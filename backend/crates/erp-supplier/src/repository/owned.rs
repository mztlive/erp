//! [`persistence_core::Repository`] 的集合别名。
//!
//! 领域方法是通用仓储上的扩展 trait。

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
