use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// 可空字段的更新意图。
///
/// 该类型区分请求未携带字段、显式传入 `null` 与传入具体值，避免使用
/// `Option<T>` 时把“保持不变”和“清除字段”混为一谈。
///
/// # 容器侧契约（必读）
///
/// `Unchanged` 与 `Clear` 序列化均为 `null`，反序列化时 `null` 只还原为
/// `Clear`；因此容器必须以 `#[serde(skip_serializing_if = "FieldUpdate::is_unchanged")]`
/// 跳过 `Unchanged`，否则一次 serde 往返会把“保持不变”静默变为“清除字段”。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FieldUpdate<T> {
    /// 请求未携带字段，保留原值。
    #[default]
    Unchanged,
    /// 请求显式传入 `null`，清除原值。
    Clear,
    /// 请求传入具体值，替换原值。
    Set(T),
}

impl<T> FieldUpdate<T> {
    /// 识别 [`FieldUpdate::Unchanged`]，供容器序列化跳过未提交字段；否则往返时可能被误解为清空。
    pub fn is_unchanged(&self) -> bool {
        matches!(self, Self::Unchanged)
    }

    /// 创建对象时将更新意图转成初始值：仅 `Set` 有值。修改已有对象时须用 [`Self::apply_to`] 保留缺省与清空的区别。
    pub fn into_option(self) -> Option<T> {
        match self {
            Self::Set(value) => Some(value),
            Self::Unchanged | Self::Clear => None,
        }
    }

    /// 把补丁写入已有可空字段：未提供保持原值，`Clear` 置空，`Set` 覆盖。
    pub fn apply_to(self, target: &mut Option<T>) {
        match self {
            Self::Unchanged => {},
            Self::Clear => *target = None,
            Self::Set(value) => *target = Some(value),
        }
    }
}

impl FieldUpdate<String> {
    /// 把非 serde 入口的可选文本解释为补丁：未提供保留旧值，空串或纯空白清空，其余原样设置。不裁剪或截断 `Set`；
    /// 长度、格式及最终规范化仍由领域实体负责。serde 入口另由容器默认值处理缺失字段，显式 `null` 则清空。
    pub fn from_optional_text(value: Option<String>) -> Self {
        match value {
            Some(value) if value.trim().is_empty() => Self::Clear,
            Some(value) => Self::Set(value),
            None => Self::Unchanged,
        }
    }
}

impl<'de, T> Deserialize<'de> for FieldUpdate<T>
where
    T: Deserialize<'de>,
{
    // 解码补丁字段时仅区分显式 `null` 和具体值：分别为清空和设置；
    // 缺失字段本方法不可见，容器须用 `#[serde(default)]` 保留不变。内部值解码失败时向上传递。
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<T>::deserialize(deserializer).map(|value| match value {
            Some(value) => Self::Set(value),
            None => Self::Clear,
        })
    }
}

impl<T> Serialize for FieldUpdate<T>
where
    T: Serialize,
{
    // 把 `Set` 写为具体值，其他意图都写为 `null`。容器必须用 `FieldUpdate::is_unchanged` 跳过未提供的值，
    // 避免序列化往返把保留旧值变成清空。
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Set(value) => value.serialize(serializer),
            Self::Unchanged | Self::Clear => serializer.serialize_none(),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::FieldUpdate;

    #[derive(Debug, Deserialize)]
    struct SampleUpdate {
        #[serde(default)]
        note: FieldUpdate<String>,
    }

    #[test]
    fn apply_to_should_preserve_clear_and_set_intent() {
        let mut target = Some("old".to_string());
        FieldUpdate::Unchanged.apply_to(&mut target);
        assert_eq!(target.as_deref(), Some("old"));

        FieldUpdate::Clear.apply_to(&mut target);
        assert_eq!(target, None);

        FieldUpdate::Set("new".to_string()).apply_to(&mut target);
        assert_eq!(target.as_deref(), Some("new"));
    }

    #[test]
    fn from_optional_text_maps_missing_blank_and_raw_values() {
        assert_eq!(FieldUpdate::from_optional_text(None), FieldUpdate::Unchanged);
        assert_eq!(FieldUpdate::from_optional_text(Some(String::new())), FieldUpdate::Clear);
        assert_eq!(FieldUpdate::from_optional_text(Some(" \t\n ".to_string())), FieldUpdate::Clear);
        assert_eq!(
            FieldUpdate::from_optional_text(Some("PREPAY_50".to_string())),
            FieldUpdate::Set("PREPAY_50".to_string())
        );
        assert_eq!(
            FieldUpdate::from_optional_text(Some("  PREPAY_50  ".to_string())),
            FieldUpdate::Set("  PREPAY_50  ".to_string()),
            "通用转换不得在保存前 trim"
        );
    }

    #[test]
    fn serde_missing_and_explicit_null_contract_is_unchanged() {
        let missing: SampleUpdate = serde_json::from_str("{}").unwrap();
        assert_eq!(missing.note, FieldUpdate::Unchanged);

        let clear: SampleUpdate = serde_json::from_str(r#"{"note":null}"#).unwrap();
        assert_eq!(clear.note, FieldUpdate::Clear);

        let set: SampleUpdate = serde_json::from_str(r#"{"note":" abc "}"#).unwrap();
        assert_eq!(set.note, FieldUpdate::Set(" abc ".to_string()));
    }
}
