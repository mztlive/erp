//! 导入 MongoDB 仓储与访问器。

mod command_receipt;
pub mod extensions;
pub use command_receipt::ImportCommandReceiptExt;
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
