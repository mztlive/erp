//! 域 D05 `file_asset` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as FileAssetExt>::FILE_ASSETS` 等值。

use crate::repository::owned::{DocumentAttachmentRepository, FileAssetRepository};
use mongodb::Database;

use super::super::file_asset::FileAssetFilter;

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
    /// 返回 `FileAssetRepository<'_>`。
    fn file_assets(&self) -> FileAssetRepository<'_>;

    /// 获取 `document_attachment` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `DocumentAttachmentRepository<'_>`。
    fn document_attachments(&self) -> DocumentAttachmentRepository<'_>;
}

impl FileAssetExt for Database {
    type FileAssetFilter = FileAssetFilter;

    fn file_assets(&self) -> FileAssetRepository<'_> {
        FileAssetRepository::new(self, Self::FILE_ASSETS)
    }

    fn document_attachments(&self) -> DocumentAttachmentRepository<'_> {
        DocumentAttachmentRepository::new(self, Self::DOCUMENT_ATTACHMENTS)
    }
}
