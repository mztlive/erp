//! 34 个域的 ID newtype（P0-1.1 共享基元任务）。
//!
//! 稳定主表 → `<Entity>Id`；修订表 → `<Entity>RevisionId`；行表 → `<Entity>LineId`。
//! 值由 `id_generator::next_id()` 产生（UUID v4，32 位十六进制），ID 不承载业务含义。
//! P0 冻结后禁止在域内自定义 ID 类型。
