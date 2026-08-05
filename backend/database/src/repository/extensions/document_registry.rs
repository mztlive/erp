//! 域 D02 `document_registry`：business_document、document_relation、document_participant、workflow_action（页面：全部单据页）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D02 仓储访问器（P2 填充）。
pub trait DocumentRegistryExt: Sized {}

impl DocumentRegistryExt for mongodb::Database {}
