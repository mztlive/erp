//! 域 D26 `publication`：product_publication(+_revision、_revision_media)、product_publication_delivery（页面：W22）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D26 仓储访问器（P2 填充）。
pub trait PublicationExt: Sized {}

impl PublicationExt for mongodb::Database {}
