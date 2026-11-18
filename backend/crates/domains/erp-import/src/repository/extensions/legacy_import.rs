//! 域 D22 `legacy_import` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as LegacyImportExt>::LEGACY_IMPORT_BATCHES` 等值。

use mongodb::Database;

use super::super::legacy_import::LegacyImportRepository;
use crate::repository::owned::{
    LegacyImportBatchRepository, LegacyImportConfirmationRepository, LegacyImportRowRepository,
};

/// 域 D22 仓储访问器。
pub trait LegacyImportExt {
    /// `legacy_import_batch` 集合名。
    const LEGACY_IMPORT_BATCHES: &'static str = "legacy_import_batches";
    /// `legacy_import_row` 集合名。
    const LEGACY_IMPORT_ROWS: &'static str = "legacy_import_rows";
    /// `legacy_import_confirmation` 集合名。
    const LEGACY_IMPORT_CONFIRMATIONS: &'static str = "legacy_import_confirmations";

    /// 取得导入批次集合仓储，供按稳定身份读写批次和查询列表。
    fn legacy_import_batches(&self) -> LegacyImportBatchRepository<'_>;

    /// 取得导入行集合仓储，供处理阶段读写逐行事实。
    fn legacy_import_rows(&self) -> LegacyImportRowRepository<'_>;

    /// 取得导入确认集合仓储，供按任务或试算定位责任范围事实。
    fn legacy_import_confirmations(&self) -> LegacyImportConfirmationRepository<'_>;

    /// 取得协调批次和导入行写入的领域仓储；跨集合写入仍须调用方提供事务。
    fn legacy_import(&self) -> LegacyImportRepository<'_>;
}

impl LegacyImportExt for Database {
    fn legacy_import_batches(&self) -> LegacyImportBatchRepository<'_> {
        LegacyImportBatchRepository::new(self, Self::LEGACY_IMPORT_BATCHES)
    }

    fn legacy_import_rows(&self) -> LegacyImportRowRepository<'_> {
        LegacyImportRowRepository::new(self, Self::LEGACY_IMPORT_ROWS)
    }

    fn legacy_import_confirmations(&self) -> LegacyImportConfirmationRepository<'_> {
        LegacyImportConfirmationRepository::new(self, Self::LEGACY_IMPORT_CONFIRMATIONS)
    }

    fn legacy_import(&self) -> LegacyImportRepository<'_> {
        LegacyImportRepository::new(self)
    }
}
