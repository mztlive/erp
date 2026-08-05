//! 域 D22 `legacy_import`：legacy_import_batch、legacy_import_row、legacy_import_confirmation（页面：W18）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D22 仓储访问器（P2 填充）。
pub trait LegacyImportExt: Sized {}

impl LegacyImportExt for mongodb::Database {}
