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

/// 校验两个可选字段必须同时提供或同时省略。
///
/// 成对出现是本域多个实体的共有不变式（映射时间/责任人、确认时间/确认人、
/// 对象类型/对象 ID、预期版本/内容摘要等）；调用方只传入存在性布尔值，
/// 避免移动原值，错误文案由调用方指定以保持既有合同。
///
/// # 参数
/// * `first_present` - 第一个字段是否存在（`is_some()` 结果）
/// * `second_present` - 第二个字段是否存在（`is_some()` 结果）
/// * `message` - 不成对时返回的错误文案（保持既有合同文案不变）
///
/// # 返回
/// 成对时返回 `Ok(())`。
///
/// # 错误
/// 一个存在另一个缺失时返回 `LogicError`。
pub(crate) fn ensure_paired(
    first_present: bool,
    second_present: bool,
    message: &str,
) -> erp_core::Result<()> {
    if first_present != second_present {
        return Err(erp_core::Error::from(message));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ensure_paired;

    #[test]
    fn paired_options_accept_both_present_or_both_absent() {
        assert!(ensure_paired(true, true, "不成对").is_ok());
        assert!(ensure_paired(false, false, "不成对").is_ok());
    }

    #[test]
    fn paired_options_reject_half_present() {
        assert!(ensure_paired(true, false, "必须成对").is_err());
        assert!(ensure_paired(false, true, "必须成对").is_err());
    }
}
