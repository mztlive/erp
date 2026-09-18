//! Collection-scoped aliases for [`persistence_core::Repository`].
//!
//! Domain-specific methods are extension traits on the generic repository.

pub type LegacyImportBatchRepository<'a> =
    persistence_core::Repository<'a, crate::entity::legacy_import::LegacyImportBatch>;

pub type LegacyImportRowRepository<'a> =
    persistence_core::Repository<'a, crate::entity::legacy_import::LegacyImportRow>;

pub type LegacyImportConfirmationRepository<'a> =
    persistence_core::Repository<'a, crate::entity::legacy_import::LegacyImportConfirmation>;
