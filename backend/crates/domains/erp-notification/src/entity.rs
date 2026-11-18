use entity_core::BaseModel;
use entity_macros::{Entity, id_type};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Error, Result};

id_type!(NotificationId);

/// 投递渠道；站外渠道名称由组合根注册。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Channel {
    Inbox,
    External(String),
}

/// 一位收件人的不可变通知内容；同一事件必须复用相同 dedup_key。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationData {
    pub dedup_key: String,
    pub recipient_id: String,
    pub title: String,
    pub body: String,
    pub href: Option<String>,
}

impl NotificationData {
    /// 规范化通知并拒绝无效或外部跳转地址。
    /// # 参数
    /// self 为生产者构造的内容。
    /// # 返回
    /// 可交给仓储或渠道的不可变内容。
    /// # 错误
    /// 空白、超长或非站内跳转返回 Invalid。
    pub fn normalized(mut self) -> Result<Self> {
        self.dedup_key = required(&self.dedup_key, 256)?;
        self.recipient_id = required(&self.recipient_id, 128)?;
        self.title = required(&self.title, 128)?;
        self.body = required(&self.body, 2048)?;

        if let Some(href) = &self.href
            && (!href.starts_with('/')
                || href.starts_with("//")
                || href.len() > 1024
                || href.contains('\\')
                || href.chars().any(char::is_control))
        {
            return Err(Error::Invalid("跳转地址必须为站内路径"));
        }
        Ok(self)
    }
}

fn required(value: &str, max: usize) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max {
        return Err(Error::Invalid("必填内容为空或超长"));
    }
    Ok(value.to_owned())
}

/// 持久化个人站内信；已读状态不得覆盖通知正文。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct Notification {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub content: NotificationData,
    pub read_at: Option<i64>,
}

impl Notification {
    pub(crate) fn new(content: NotificationData) -> Self {
        Self {
            base: BaseModel::new(NotificationId::new(Uuid::new_v4().to_string()).to_string()),
            content,
            read_at: None,
        }
    }

    pub(crate) fn replay(&self, content: &NotificationData) -> Result<()> {
        if self.content == *content { Ok(()) } else { Err(Error::Conflict) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> NotificationData {
        NotificationData {
            dedup_key: "event-1".into(),
            recipient_id: "u1".into(),
            title: "待审批".into(),
            body: "请处理审批".into(),
            href: Some("/workspace/tasks".into()),
        }
    }

    #[test]
    fn validates_content_and_internal_navigation() {
        assert!(content().normalized().is_ok());
        for href in ["//evil.example", "/\\evil.example", "javascript:alert(1)", "/\n/evil"] {
            let mut data = content();
            data.href = Some(href.into());
            assert!(data.normalized().is_err());
        }

        let mut data = content();
        data.recipient_id = " ".into();
        assert!(data.normalized().is_err());
        let mut data = content();
        data.title = "字".repeat(129);
        assert!(data.normalized().is_err());
    }

    #[test]
    fn replay_preserves_read_state_and_rejects_different_payload() {
        let mut message = Notification { base: BaseModel::fake(), content: content(), read_at: Some(42) };
        assert!(message.replay(&content()).is_ok());
        assert_eq!(message.read_at, Some(42));

        message.content.body = "changed".into();
        assert!(matches!(message.replay(&content()), Err(Error::Conflict)));
    }
}
