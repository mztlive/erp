//! 域 D05 `file_asset` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as FileAssetExt>::FILE_ASSETS` 等值。

use entities::file_asset::{DocumentAttachment, FileAsset};
use mongodb::Database;

use super::super::file_asset::{FileAssetFilter, FileAssetRepository};
use crate::Repository;

/// 域 D05 仓储访问器。
pub trait FileAssetExt {
    /// `file_asset` 集合名。
    const FILE_ASSETS: &'static str = "file_assets";
    /// `document_attachment` 集合名。
    const DOCUMENT_ATTACHMENTS: &'static str = "document_attachments";

    /// 文件资产列表筛选条件类型（定义见 `repository::file_asset`）。
    type FileAssetFilter;

    /// 获取 `file_asset` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::file_asset::FileAsset>`。
    fn file_assets(&self) -> Repository<'_, FileAsset>;

    /// 获取 `document_attachment` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::file_asset::DocumentAttachment>`。
    fn document_attachments(&self) -> Repository<'_, DocumentAttachment>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `FileAssetRepository` 实例。
    fn file_asset(&self) -> FileAssetRepository<'_>;
}

impl FileAssetExt for Database {
    type FileAssetFilter = FileAssetFilter;

    fn file_assets(&self) -> Repository<'_, FileAsset> {
        Repository::new(self, Self::FILE_ASSETS)
    }

    fn document_attachments(&self) -> Repository<'_, DocumentAttachment> {
        Repository::new(self, Self::DOCUMENT_ATTACHMENTS)
    }

    fn file_asset(&self) -> FileAssetRepository<'_> {
        FileAssetRepository::new(self)
    }
}
