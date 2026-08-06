//! 域 D22 `legacy_import` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as LegacyImportExt>::LEGACY_IMPORT_BATCHES` 等值。

use entities::legacy_import::{LegacyImportBatch, LegacyImportConfirmation, LegacyImportRow};
use mongodb::Database;

use super::super::legacy_import::{
    LegacyImportBatchFilter, LegacyImportConfirmationFilter, LegacyImportRepository, LegacyImportRowFilter,
};
use crate::Repository;

/// 域 D22 仓储访问器。
pub trait LegacyImportExt {
    /// `legacy_import_batch` 集合名。
    const LEGACY_IMPORT_BATCHES: &'static str = "legacy_import_batches";
    /// `legacy_import_row` 集合名。
    const LEGACY_IMPORT_ROWS: &'static str = "legacy_import_rows";
    /// `legacy_import_confirmation` 集合名。
    const LEGACY_IMPORT_CONFIRMATIONS: &'static str = "legacy_import_confirmations";

    /// 导入批次列表筛选条件类型（定义见 `repository::legacy_import`）。
    type LegacyImportBatchFilter;

    /// 导入行列表筛选条件类型（定义见 `repository::legacy_import`）。
    type LegacyImportRowFilter;

    /// 导入确认列表筛选条件类型（定义见 `repository::legacy_import`）。
    type LegacyImportConfirmationFilter;

    /// 获取 `legacy_import_batch` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::legacy_import::LegacyImportBatch>`。
    fn legacy_import_batches(&self) -> Repository<'_, LegacyImportBatch>;

    /// 获取 `legacy_import_row` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::legacy_import::LegacyImportRow>`。
    fn legacy_import_rows(&self) -> Repository<'_, LegacyImportRow>;

    /// 获取 `legacy_import_confirmation` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::legacy_import::LegacyImportConfirmation>`。
    fn legacy_import_confirmations(&self) -> Repository<'_, LegacyImportConfirmation>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `LegacyImportRepository` 实例。
    fn legacy_import(&self) -> LegacyImportRepository<'_>;
}

impl LegacyImportExt for Database {
    type LegacyImportBatchFilter = LegacyImportBatchFilter;
    type LegacyImportRowFilter = LegacyImportRowFilter;
    type LegacyImportConfirmationFilter = LegacyImportConfirmationFilter;

    fn legacy_import_batches(&self) -> Repository<'_, LegacyImportBatch> {
        Repository::new(self, Self::LEGACY_IMPORT_BATCHES)
    }

    fn legacy_import_rows(&self) -> Repository<'_, LegacyImportRow> {
        Repository::new(self, Self::LEGACY_IMPORT_ROWS)
    }

    fn legacy_import_confirmations(&self) -> Repository<'_, LegacyImportConfirmation> {
        Repository::new(self, Self::LEGACY_IMPORT_CONFIRMATIONS)
    }

    fn legacy_import(&self) -> LegacyImportRepository<'_> {
        LegacyImportRepository::new(self)
    }
}
