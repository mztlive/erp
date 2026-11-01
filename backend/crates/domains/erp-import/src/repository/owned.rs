//! [`persistence_core::Repository`] 的集合作用域别名。
//!
//! 领域专用方法是通用仓储上的扩展 trait。

pub type LegacyImportBatchRepository<'a> =
    persistence_core::Repository<'a, crate::entity::legacy_import::LegacyImportBatch>;

pub type LegacyImportRowRepository<'a> =
    persistence_core::Repository<'a, crate::entity::legacy_import::LegacyImportRow>;

pub type LegacyImportConfirmationRepository<'a> =
    persistence_core::Repository<'a, crate::entity::legacy_import::LegacyImportConfirmation>;
