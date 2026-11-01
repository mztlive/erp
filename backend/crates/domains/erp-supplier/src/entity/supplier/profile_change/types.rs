use erp_core::field_update::FieldUpdate;

/// 将可空输入映射为明确设置或清空意图。
///
/// # 参数
/// * `value` - 可空输入值
///
/// # 返回
/// `Some(v)` 映射为 `Set(v)`，`None` 映射为 `Clear`，调用方以 `Unchanged` 表达保留。
///
/// # 错误
/// 无；仅做枚举映射。
///
/// # 约束
/// 不触及 I/O，仅做纯映射；与 Service 侧 `option_as_authoritative_update` 保持等价。
pub(super) fn option_as_authoritative_update<T>(value: Option<T>) -> FieldUpdate<T> {
    value.map_or(FieldUpdate::Clear, FieldUpdate::Set)
}
