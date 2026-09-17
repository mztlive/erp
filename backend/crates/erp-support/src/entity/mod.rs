//! Support entities and value objects.

pub mod bulk_job;
pub mod file_asset;
pub mod source_registry;

/// 枚举稳定代码与中文展示名的统一声明宏。
///
/// 各枚举的 `serde` 重命名与变体集合保持不变；宏只收敛 `label()`/`as_str()`
/// 两个方法的样板，在模块顶层为指定枚举生成完整 `impl` 块（调用时直接写
/// 在枚举定义之后，不要包在手写 `impl` 里）。显示文案与序列化值保持不变。
///
/// # 示例
/// `enum_str!(JobType { Import => ("import", "导入") })` 生成
/// `label()`（中文展示）与 `as_str()`（稳定代码）两个方法。
macro_rules! enum_str {
    ($name:ident { $($variant:ident => ($code:literal, $label:literal)),* $(,)? }) => {
        impl $name {
            /// 返回枚举的中文展示名。
            ///
            /// # 返回
            /// 返回面向用户的中文标签。
            pub fn label(&self) -> &'static str {
                match self {
                    $(Self::$variant => $label,)*
                }
            }

            /// 返回枚举的稳定代码。
            ///
            /// # 返回
            /// 返回用于持久化与查询的稳定字符串。
            pub fn as_str(&self) -> &'static str {
                match self {
                    $(Self::$variant => $code,)*
                }
            }
        }
    };
}

pub(crate) use enum_str;
