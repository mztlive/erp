use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// 可空字段的更新意图。
///
/// 该类型区分请求未携带字段、显式传入 `null` 与传入具体值，避免使用
/// `Option<T>` 时把“保持不变”和“清除字段”混为一谈。
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
    /// 判断请求是否未携带该字段。
    ///
    /// # 返回值
    /// 更新意图为 [`FieldUpdate::Unchanged`] 时返回 `true`。
    pub fn is_unchanged(&self) -> bool {
        matches!(self, Self::Unchanged)
    }

    /// 借用待设置的具体值。
    ///
    /// # 返回值
    /// `Set` 返回值引用，`Unchanged` 与 `Clear` 返回 `None`。
    pub fn as_set(&self) -> Option<&T> {
        match self {
            Self::Set(value) => Some(value),
            Self::Unchanged | Self::Clear => None,
        }
    }

    /// 将更新意图转换为创建可空字段时使用的值。
    ///
    /// # 返回值
    /// `Set` 返回具体值，`Unchanged` 与 `Clear` 返回 `None`。
    pub fn into_option(self) -> Option<T> {
        match self {
            Self::Set(value) => Some(value),
            Self::Unchanged | Self::Clear => None,
        }
    }

    /// 将更新意图应用到可空字段。
    ///
    /// # 参数
    /// * `target` - 待更新的可空字段
    pub fn apply_to(self, target: &mut Option<T>) {
        match self {
            Self::Unchanged => {}
            Self::Clear => *target = None,
            Self::Set(value) => *target = Some(value),
        }
    }
}

impl<'de, T> Deserialize<'de> for FieldUpdate<T>
where
    T: Deserialize<'de>,
{
    /// 从 JSON 字段值解析更新意图。
    ///
    /// 显式 `null` 解析为 `Clear`，具体值解析为 `Set`；字段缺失由容器字段的
    /// `#[serde(default)]` 解析为 `Unchanged`。
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
    /// 序列化更新意图。
    ///
    /// `Clear` 序列化为 `null`，`Set` 序列化为具体值。容器应通过
    /// `skip_serializing_if` 跳过 `Unchanged`。
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
    use super::FieldUpdate;

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
    fn as_set_only_borrows_concrete_value() {
        assert_eq!(FieldUpdate::<String>::Unchanged.as_set(), None);
        assert_eq!(FieldUpdate::<String>::Clear.as_set(), None);
        assert_eq!(
            FieldUpdate::Set("item-1".to_string())
                .as_set()
                .map(String::as_str),
            Some("item-1")
        );
    }
}
