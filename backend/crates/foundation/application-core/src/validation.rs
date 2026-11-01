//! 不含输入值的字段校验合同；字段路径与公开原因跨层保留。

use std::collections::BTreeMap;

use validator::{ValidationError, ValidationErrors, ValidationErrorsKind};

/// 可对外展示的字段错误；仅保存规则说明，不保存 validator 的输入参数。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("请修改标记的字段后重新提交")]
pub struct ValidationIssues(BTreeMap<String, String>);

impl ValidationIssues {
    /// 构造单字段错误。
    ///
    /// # 参数
    /// `path` 为接口字段路径；`message` 为不含用户输入或内部细节的规则说明。
    /// # 返回
    /// 包含该字段的错误集合。
    /// # 错误
    /// 无。
    pub fn field(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self(BTreeMap::from([(path.into(), message.into())]))
    }

    /// 获取字段路径和规则说明。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 字段错误映射；数组下标从零开始。
    /// # 错误
    /// 无。
    pub fn fields(&self) -> &BTreeMap<String, String> {
        &self.0
    }

    /// 为嵌套校验补充所属对象或数组行路径。
    ///
    /// # 参数
    /// `prefix` 为父字段路径，例如 `bank_accounts[1]`。
    /// # 返回
    /// 所有字段加上父路径后的错误集合。
    /// # 错误
    /// 无。
    pub fn at(self, prefix: &str) -> Self {
        Self(self.0.into_iter().map(|(path, message)| (format!("{prefix}.{path}"), message)).collect())
    }
}

impl From<ValidationErrors> for ValidationIssues {
    fn from(errors: ValidationErrors) -> Self {
        let mut fields = BTreeMap::new();
        collect(&errors, "", &mut fields);
        Self(fields)
    }
}

fn collect(errors: &ValidationErrors, prefix: &str, fields: &mut BTreeMap<String, String>) {
    for (field, errors) in errors.errors() {
        let path = if prefix.is_empty() { field.to_string() } else { format!("{prefix}.{field}") };
        match errors {
            ValidationErrorsKind::Field(errors) => {
                let messages = errors.iter().map(rule_message).collect::<Vec<_>>();
                fields.insert(path, messages.join("；"));
            },
            ValidationErrorsKind::Struct(errors) => collect(errors, &path, fields),
            ValidationErrorsKind::List(rows) => {
                for (index, errors) in rows {
                    collect(errors, &format!("{path}[{index}]"), fields);
                }
            },
        }
    }
}

fn rule_message(error: &ValidationError) -> String {
    if let Some(message) = &error.message {
        return message.to_string();
    }
    match error.code.as_ref() {
        "required" | "non_blank" | "non_empty" => "不能为空".into(),
        "email" => "请填写有效的邮箱地址".into(),
        "url" => "请填写有效的网址".into(),
        "length" | "range" => {
            let unit = if error.code == "length" { "长度或数量" } else { "数值" };
            let number = |key: &str| error.params.get(key).filter(|v| v.is_number()).map(ToString::to_string);
            match (number("min"), number("max"), number("equal")) {
                (_, _, Some(value)) => format!("{unit}必须为 {value}"),
                (Some(min), Some(max), _) => format!("{unit}必须在 {min} 至 {max} 之间"),
                (Some(min), None, _) => format!("{unit}不能小于 {min}"),
                (None, Some(max), _) => format!("{unit}不能大于 {max}"),
                _ => format!("{unit}不符合要求，请修改"),
            }
        },
        _ => "格式不符合要求，请修改".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_row_path_and_does_not_copy_input_value() {
        let mut error = ValidationError::new("length");
        error.add_param("min".into(), &1);
        error.add_param("value".into(), &"secret-input");
        let mut errors = ValidationErrors::new();
        errors.add("bank_name", error);
        let issues = ValidationIssues::from(errors).at("bank_accounts[1]");
        assert_eq!(issues.fields()["bank_accounts[1].bank_name"], "长度或数量不能小于 1");
        assert!(!format!("{issues:?}").contains("secret-input"));
    }

    #[test]
    fn preserves_nested_validator_lists() {
        let mut child = ValidationErrors::new();
        let mut error = ValidationError::new("required");
        error.message = Some("姓名不能为空".into());
        child.add("name", error);
        let errors = ValidationErrors(std::collections::HashMap::from([(
            "contacts".into(),
            ValidationErrorsKind::List(BTreeMap::from([(2, Box::new(child))])),
        )]));
        assert_eq!(ValidationIssues::from(errors).fields()["contacts[2].name"], "姓名不能为空");
    }
}
