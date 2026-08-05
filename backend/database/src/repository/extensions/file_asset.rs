//! 域 D05 `file_asset`：file_asset、document_attachment（页面：W04、W18）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D05 仓储访问器（P2 填充）。
pub trait FileAssetExt: Sized {}

impl FileAssetExt for mongodb::Database {}
