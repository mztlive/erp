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

/// 由映射表实现 `label()` 查找。
///
/// 调用方应保证变体在表中；表中没有该变体时不 panic。
///
/// # 参数
/// * `variant` - 待查找的枚举变体
/// * `table` - 变体到中文标签与稳定代码的映射表
///
/// # 返回
/// 命中时返回对应中文标签；未命中时返回空字符串。
///
/// # 错误
/// 不返回错误。
pub fn label_of<T: Copy + PartialEq>(variant: T, table: &[DisplayEntry<T>]) -> &'static str {
    table.iter().find(|entry| entry.variant == variant).map(|entry| entry.label).unwrap_or("")
}

/// 由映射表实现 `as_str()` 查找。
///
/// 调用方应保证变体在表中；表中没有该变体时不 panic。
///
/// # 参数
/// * `variant` - 待查找的枚举变体
/// * `table` - 变体到中文标签与稳定代码的映射表
///
/// # 返回
/// 命中时返回对应稳定代码；未命中时返回空字符串。
///
/// # 错误
/// 不返回错误。
pub fn code_of<T: Copy + PartialEq>(variant: T, table: &[DisplayEntry<T>]) -> &'static str {
    table.iter().find(|entry| entry.variant == variant).map(|entry| entry.code).unwrap_or("")
}

/// 由稳定代码反查枚举变体。
///
/// # 参数
/// * `code` - 稳定代码，按表中字符串精确匹配
/// * `table` - 变体到中文标签与稳定代码的映射表
///
/// # 返回
/// 命中时返回对应变体；未知代码返回 `None`。
///
/// # 错误
/// 不返回错误。
#[allow(dead_code)]
pub fn from_code<T: Copy>(code: &str, table: &[DisplayEntry<T>]) -> Option<T> {
    table.iter().find(|entry| entry.code == code).map(|entry| entry.variant)
}
