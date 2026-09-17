//! 供应商枚举展示三件套约定（erp-supplier-002）。
//!
//! 各枚举的 `label()`（中文展示名）/`as_str()`（稳定持久化代码）分支结构
//! 完全同构；本约定统一映射表形状，各枚举只声明变体到文案/代码的映射，
//! 对外文案与持久化字符串保持不变。

/// 枚举展示映射表的一行：变体标签与稳定代码。
pub struct DisplayEntry<T> {
    /// 枚举变体。
    pub variant: T,
    /// 中文展示名。
    pub label: &'static str,
    /// 稳定持久化代码。
    pub code: &'static str,
}

/// 由映射表实现 `label()` 查找；调用方保证变体必在表中。
pub fn label_of<T: Copy + PartialEq>(variant: T, table: &[DisplayEntry<T>]) -> &'static str {
    table.iter().find(|entry| entry.variant == variant).map(|entry| entry.label).unwrap_or("")
}

/// 由映射表实现 `as_str()` 查找；调用方保证变体必在表中。
pub fn code_of<T: Copy + PartialEq>(variant: T, table: &[DisplayEntry<T>]) -> &'static str {
    table.iter().find(|entry| entry.variant == variant).map(|entry| entry.code).unwrap_or("")
}

/// 由稳定代码反查枚举变体；未知代码返回 `None`。
#[allow(dead_code)]
pub fn from_code<T: Copy>(code: &str, table: &[DisplayEntry<T>]) -> Option<T> {
    table.iter().find(|entry| entry.code == code).map(|entry| entry.variant)
}
