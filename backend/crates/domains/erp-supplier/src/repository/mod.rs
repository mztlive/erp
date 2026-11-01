//! 供应商 MongoDB 仓储与访问器。

pub mod extensions;
pub mod owned;
pub mod prelude;
pub mod scope;
pub mod supplier;

pub use extensions::SupplierExt;
pub use owned::{
    SupplierAccountRepository, SupplierCapabilityRepository, SupplierCommercialProfileRevisionRepository,
    SupplierProfileCommandRepository, SupplierQualificationCapabilityRepository,
    SupplierQualificationRepository,
};
pub use scope::{SupplierAccountRepositoryScopeExt, SupplierReadScope, SupplierScopeClause, SupplierVersion};
pub use supplier::{
    SupplierAccountFilter, SupplierAccountRepositoryExt, SupplierAccountRow, SupplierCapabilityFilter,
    SupplierCapabilityRepositoryExt, SupplierCommercialProfileFilter,
    SupplierCommercialProfileRevisionRepositoryExt, SupplierDetailBundle, SupplierListBundle,
    SupplierListSearchInput, SupplierProfileCommandRepositoryExt,
    SupplierQualificationCapabilityRepositoryExt, SupplierQualificationFilter,
    SupplierQualificationHealthFilter, SupplierQualificationRepositoryExt, SupplierRepository,
};

pub mod handover_receipt;
