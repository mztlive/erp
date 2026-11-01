//! 领域错误的机械适配；错误枚举及专属分类仍由拥有领域声明。

/// 为领域错误实现应用合同错误和 validator 字段错误转换。
///
/// # 参数
/// * `$error` - 包含 `Internal`、`ValidationError` 和 `InvalidInput` 变体的领域错误类型。
///
/// # 返回
/// 生成两个 `From` 实现，保留原消息和结构化字段错误。
///
/// # 错误
/// 不执行 I/O；消费方须已依赖 validator，且目标变体类型须与应用合同一致。
#[macro_export]
macro_rules! impl_application_error_conversions {
    ($error:ty) => {
        impl From<$crate::Error> for $error {
            /// 保留应用合同错误的分类、消息和字段明细。
            ///
            /// # 参数
            /// * `error` - 应用合同错误。
            /// # 返回
            /// 对应的领域错误。
            /// # 错误
            /// 不返回错误。
            fn from(error: $crate::Error) -> Self {
                match error {
                    $crate::Error::Internal(message) => Self::Internal(message),
                    $crate::Error::ValidationError(message) => Self::ValidationError(message),
                    $crate::Error::InvalidInput(issues) => Self::InvalidInput(issues),
                }
            }
        }

        impl From<::validator::ValidationErrors> for $error {
            /// 将字段校验失败转换为保留字段路径的领域错误。
            ///
            /// # 参数
            /// * `errors` - validator 字段错误。
            /// # 返回
            /// 包含安全字段明细的 `InvalidInput`。
            /// # 错误
            /// 不返回错误。
            fn from(errors: ::validator::ValidationErrors) -> Self {
                Self::InvalidInput(errors.into())
            }
        }
    };
}

/// 为采用标准变体集合的领域错误实现稳定分类。
///
/// # 参数
/// * `$error` - 仅包含本宏穷举的标准变体的领域错误类型。
///
/// # 返回
/// 生成固有方法 `class`；分类依据变体，不解析错误文案。
///
/// # 错误
/// 不返回错误；含专属变体或按 source 分类的领域须自行实现 `class`。
#[macro_export]
macro_rules! impl_domain_error_class {
    ($error:ty) => {
        impl $error {
            /// 返回协议层使用的稳定错误类别。
            ///
            /// # 参数
            /// 无。
            /// # 返回
            /// 通用冲突为 `Conflict`，业务与字段错误为 `BusinessRule`，
            /// 权限错误为 `Forbidden`，内部失败及提交结果未知为 `Internal`。
            /// # 错误
            /// 不返回错误；`OutcomeUnknown` 不归为可重试冲突。
            pub fn class(&self) -> $crate::ErrorClass {
                match self {
                    Self::Internal(_)
                    | Self::Logic(_)
                    | Self::RepositoryError(_)
                    | Self::OutcomeUnknown(_) => $crate::ErrorClass::Internal,
                    Self::ConflictError(_) | Self::ReceiptDuplicate(_) | Self::TransientTransaction(_) => {
                        $crate::ErrorClass::Conflict
                    },
                    Self::BusinessLogicError(_)
                    | Self::ValidationError(_)
                    | Self::InvalidInput(_)
                    | Self::NotFound(_) => $crate::ErrorClass::BusinessRule,
                    Self::Forbidden(_) | Self::Unauthenticated(_) => $crate::ErrorClass::Forbidden,
                }
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use validator::{ValidationError, ValidationErrors};

    use crate::{Error, ValidationIssues};

    #[derive(Debug)]
    enum ConsumerError {
        Internal(String),
        ValidationError(String),
        InvalidInput(ValidationIssues),
    }

    impl_application_error_conversions!(ConsumerError);

    #[test]
    fn application_errors_keep_the_original_payload() {
        assert!(matches!(
            ConsumerError::from(Error::Internal("internal".into())),
            ConsumerError::Internal(message) if message == "internal"
        ));
        assert!(matches!(
            ConsumerError::from(Error::ValidationError("required".into())),
            ConsumerError::ValidationError(message) if message == "required"
        ));
        let issues = ValidationIssues::field("lines[1].quantity", "数量必须大于零");
        let ConsumerError::InvalidInput(actual) = ConsumerError::from(Error::InvalidInput(issues.clone()))
        else {
            panic!("字段错误必须保留为 InvalidInput");
        };
        assert_eq!(actual, issues);
    }

    #[test]
    fn validator_errors_keep_field_paths_and_safe_messages() {
        let mut errors = ValidationErrors::new();
        let mut issue = ValidationError::new("required");
        issue.message = Some("编号不能为空".into());
        issue.add_param("value".into(), &"private-input");
        errors.add("code", issue);
        let ConsumerError::InvalidInput(issues) = ConsumerError::from(errors) else {
            panic!("validator 错误必须保留为 InvalidInput");
        };
        assert_eq!(issues.fields().get("code").map(String::as_str), Some("编号不能为空"));
        assert_eq!(issues.to_string(), "请修改标记的字段后重新提交");
    }
}
