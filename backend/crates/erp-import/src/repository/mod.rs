//! Import MongoDB repositories and accessors.

pub mod extensions;
pub mod legacy_import;
pub mod owned;
pub mod prelude;

pub use extensions::LegacyImportExt;
pub use legacy_import::{
    LegacyImportApplyScope, LegacyImportBatchFilter, LegacyImportBatchRepositoryExt, LegacyImportBatchRow,
    LegacyImportConfirmationFilter, LegacyImportConfirmationRepositoryExt, LegacyImportConfirmationRow,
    LegacyImportConfirmationSupersedeBatchExt, LegacyImportRepository, LegacyImportRowApplyScopeExt,
    LegacyImportRowFailedRetryExt, LegacyImportRowFilter, LegacyImportRowRepositoryExt, LegacyImportRowRow,
};
pub use owned::{LegacyImportBatchRepository, LegacyImportConfirmationRepository, LegacyImportRowRepository};

#[cfg(test)]
mod bson_roundtrip;
