use entities::{Consumer, ConsumerUpdate, FieldUpdate, LoginAccount};
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

use crate::query::{normalized_text, page_or_default, page_size_or_default};

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateConsumerParams {
    #[validate(length(min = 4, max = 64, message = "账号长度必须在4-64个字符之间"))]
    pub account: String,
    #[validate(length(min = 6, max = 64, message = "密码长度必须在6-64个字符之间"))]
    pub password: String,
    #[validate(custom(function = "validate_nickname", message = "昵称长度必须在1-32个字符之间"))]
    pub nickname: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate, Default)]
pub struct UpdateConsumerParams {
    #[serde(default, skip_deserializing)]
    pub id: String,
    #[validate(length(min = 4, max = 64, message = "账号长度必须在4-64个字符之间"))]
    pub account: Option<String>,
    #[validate(length(min = 6, max = 64, message = "密码长度必须在6-64个字符之间"))]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "FieldUpdate::is_unchanged")]
    #[validate(custom(
        function = "validate_nickname_update",
        message = "昵称长度必须在1-32个字符之间"
    ))]
    pub nickname: FieldUpdate<String>,
}

impl UpdateConsumerParams {
    /// 校验更新请求至少包含一个待变更字段。
    ///
    /// # 返回值
    /// 当包含至少一个待变更字段时返回 `Ok(())`
    ///
    /// # 错误
    /// 当没有任何更新字段时返回错误。
    pub fn ensure_has_updates(&self) -> crate::errors::Result<()> {
        if self.account.is_none() && self.password.is_none() && self.nickname.is_unchanged() {
            return Err(crate::errors::Error::ValidationError(
                "至少提供一个待更新字段".into(),
            ));
        }

        Ok(())
    }

    /// 转换为消费者实体更新及其目标 ID。
    ///
    /// # 返回值
    /// 返回目标消费者 ID，以及账号已规范化的领域更新数据。
    ///
    /// # 错误
    /// 当账号无法规范化为合法登录账号时返回错误。
    pub(crate) fn into_update(self) -> entities::Result<(String, ConsumerUpdate)> {
        let account = self.account.map(LoginAccount::new).transpose()?;
        Ok((
            self.id,
            ConsumerUpdate {
                account,
                password: self.password,
                nickname: self.nickname,
            },
        ))
    }
}

#[derive(Debug, Serialize, Deserialize, Validate, Default, Clone)]
pub struct ConsumerListParams {
    #[validate(length(min = 1, max = 64))]
    pub account: Option<String>,
    #[validate(length(min = 1, max = 32))]
    pub nickname: Option<String>,
    #[validate(range(min = 1))]
    pub page: Option<u64>,
    #[validate(range(min = 1, max = 100))]
    pub page_size: Option<u32>,
}

impl ConsumerListParams {
    /// 归一化消费者列表查询参数。
    ///
    /// # 返回值
    /// 返回不依赖仓储类型的规范化查询参数。
    pub(crate) fn normalized(&self) -> NormalizedConsumerListParams {
        NormalizedConsumerListParams {
            account: normalized_text(self.account.as_deref()),
            nickname: normalized_text(self.nickname.as_deref()),
            page: page_or_default(self.page),
            page_size: page_size_or_default(self.page_size),
        }
    }
}

/// Service 内部使用的规范化消费者列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedConsumerListParams {
    pub(crate) account: Option<String>,
    pub(crate) nickname: Option<String>,
    pub(crate) page: u64,
    pub(crate) page_size: u32,
}

#[derive(Debug, Serialize)]
pub struct ConsumerItem {
    pub id: String,
    pub account: String,
    pub nickname: Option<String>,
    pub is_active: bool,
    pub created_at: u64,
}

impl From<Consumer> for ConsumerItem {
    fn from(consumer: Consumer) -> Self {
        Self {
            id: consumer.base.id,
            account: consumer.secret.into_account(),
            nickname: consumer.nickname,
            is_active: consumer.is_active,
            created_at: consumer.base.created_at,
        }
    }
}

/// 将消费者昵称领域规则适配为 DTO 校验错误。
fn validate_nickname(nickname: &str) -> std::result::Result<(), ValidationError> {
    Consumer::validate_nickname(nickname).map_err(|_| ValidationError::new("consumer_nickname"))
}

/// 将可空昵称更新的领域规则适配为 DTO 校验错误。
fn validate_nickname_update(nickname: &FieldUpdate<String>) -> std::result::Result<(), ValidationError> {
    let FieldUpdate::Set(nickname) = nickname else {
        return Ok(());
    };

    validate_nickname(nickname)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试 `trim_helpers_remove_whitespace` 行为。
    ///
    /// # 返回
    /// 不返回数据，仅表示执行结果。
    #[test]
    fn create_consumer_params_should_deserialize() {
        let params = CreateConsumerParams {
            account: "user1".into(),
            password: "password123".into(),
            nickname: Some("Tester".into()),
        };
        assert_eq!(params.account, "user1");
        assert_eq!(params.password, "password123");
        assert_eq!(params.nickname.as_deref(), Some("Tester"));
    }

    #[test]
    fn update_consumer_params_should_accept_path_id_assignment() {
        let mut params = UpdateConsumerParams {
            id: String::new(),
            account: Some("user2".into()),
            password: Some("newpass".into()),
            nickname: FieldUpdate::Unchanged,
        };
        params.id = "cid".into();
        assert_eq!(params.id, "cid");
        assert_eq!(params.account.as_deref(), Some("user2"));
        assert_eq!(params.password.as_deref(), Some("newpass"));
        assert!(params.nickname.is_unchanged());
    }

    #[test]
    fn ensure_has_updates_rejects_empty_payload() {
        let params = UpdateConsumerParams {
            id: "cid".into(),
            account: None,
            password: None,
            nickname: FieldUpdate::Unchanged,
        };

        assert!(params.ensure_has_updates().is_err());
    }

    #[test]
    fn ensure_has_updates_accepts_non_empty_payload() {
        let params = UpdateConsumerParams {
            id: "cid".into(),
            account: Some("user2".into()),
            password: None,
            nickname: FieldUpdate::Unchanged,
        };

        assert!(params.ensure_has_updates().is_ok());
    }

    #[test]
    fn list_params_normalize_text_and_pagination_defaults() {
        let params = ConsumerListParams {
            account: Some("  consumer01  ".into()),
            nickname: Some("   ".into()),
            page: None,
            page_size: None,
        };

        let normalized = params.normalized();

        assert_eq!(normalized.account.as_deref(), Some("consumer01"));
        assert_eq!(normalized.nickname, None);
        assert_eq!(normalized.page, 1);
        assert_eq!(normalized.page_size, 20);
    }

    #[test]
    fn create_params_should_accept_trimmed_32_character_unicode_nickname() {
        let params = CreateConsumerParams {
            account: "consumer01".to_string(),
            password: "password123".to_string(),
            nickname: Some(format!("  {}  ", "名".repeat(32))),
        };

        assert!(params.validate().is_ok());
    }

    #[test]
    fn create_params_should_reject_blank_nickname() {
        let params = CreateConsumerParams {
            account: "consumer01".to_string(),
            password: "password123".to_string(),
            nickname: Some("   ".to_string()),
        };

        assert!(params.validate().is_err());
    }

    #[test]
    fn update_params_should_reject_nickname_longer_than_32_unicode_characters() {
        let params = UpdateConsumerParams {
            id: "consumer-1".to_string(),
            account: None,
            password: None,
            nickname: FieldUpdate::Set("名".repeat(33)),
        };

        assert!(params.validate().is_err());
    }

    #[test]
    fn update_params_should_normalize_account_in_domain_update() {
        let params = UpdateConsumerParams {
            id: "consumer-1".to_string(),
            account: Some("  Consumer01  ".to_string()),
            password: None,
            nickname: FieldUpdate::Unchanged,
        };

        let (_, update) = params.into_update().unwrap();

        assert_eq!(
            update.account.as_ref().map(LoginAccount::as_str),
            Some("Consumer01")
        );
    }

    #[test]
    fn nullable_nickname_should_distinguish_omitted_null_and_value() {
        let omitted: UpdateConsumerParams =
            serde_json::from_value(serde_json::json!({ "account": "consumer01" })).unwrap();
        assert_eq!(omitted.nickname, FieldUpdate::Unchanged);

        let cleared: UpdateConsumerParams =
            serde_json::from_value(serde_json::json!({ "nickname": null })).unwrap();
        assert_eq!(cleared.nickname, FieldUpdate::Clear);
        assert!(cleared.ensure_has_updates().is_ok());

        let replaced: UpdateConsumerParams =
            serde_json::from_value(serde_json::json!({ "nickname": "新昵称" })).unwrap();
        assert_eq!(replaced.nickname, FieldUpdate::Set("新昵称".to_string()));
    }
}
