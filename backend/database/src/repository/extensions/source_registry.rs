//! 域 D01 `source_registry`：source_system、external_identity_map、external_identity_target（页面：W17、W29）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D01 仓储访问器（P2 填充）。
pub trait SourceRegistryExt: Sized {}

impl SourceRegistryExt for mongodb::Database {}
