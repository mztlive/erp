use mongodb::bson::{Document, doc};
use serde::{Deserialize, Serialize};

use crate::{Error, Notification, Result};

pub(crate) const INBOX_PAGE_SIZE: usize = 20;

/// 收件箱游标；时间和 ID 同时参与排序，避免同秒消息遗漏。
#[derive(Debug, Default, Deserialize)]
pub struct InboxQuery {
    pub before_created_at: Option<i64>,
    pub before_id: Option<String>,
}

impl InboxQuery {
    pub(crate) fn filter(&self, recipient: &str) -> Result<Document> {
        let mut filter = doc! { "recipient_id": recipient };
        match (self.before_created_at, self.before_id.as_deref()) {
            (None, None) => {},
            (Some(at), Some(id)) if at >= 0 && !id.is_empty() && id.len() <= 128 => {
                filter.insert(
                    "$or",
                    vec![
                        doc! { "created_at": { "$lt": at } },
                        doc! { "created_at": at, "id": { "$lt": id } },
                    ],
                );
            },
            _ => return Err(Error::Invalid("分页条件无效，请重新打开收件箱")),
        }
        Ok(filter)
    }
}

/// 对外消息投影；不公开投递去重键或内部系统字段。
#[derive(Debug, Serialize)]
pub struct NotificationView {
    pub id: String,
    pub title: String,
    pub body: String,
    pub href: Option<String>,
    pub created_at: u64,
    pub read_at: Option<i64>,
}

impl From<Notification> for NotificationView {
    fn from(value: Notification) -> Self {
        Self {
            id: value.base.id,
            title: value.content.title,
            body: value.content.body,
            href: value.content.href,
            created_at: value.base.created_at,
            read_at: value.read_at,
        }
    }
}

/// 每页 20 条，继续读取时携带最后一条的时间及 ID。
#[derive(Debug, Serialize)]
pub struct InboxPage {
    pub items: Vec<NotificationView>,
    pub unread_count: u64,
    pub has_more: bool,
}

impl InboxPage {
    pub(crate) fn from_records(mut items: Vec<Notification>, unread_count: u64) -> Self {
        let has_more = items.len() > INBOX_PAGE_SIZE;
        items.truncate(INBOX_PAGE_SIZE);
        Self { items: items.into_iter().map(NotificationView::from).collect(), unread_count, has_more }
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;

    use super::*;
    use crate::NotificationData;

    #[test]
    fn cursor_is_bounded_and_always_scoped_to_recipient() {
        let query = InboxQuery { before_created_at: Some(42), before_id: Some("b".into()) };
        let filter = query.filter("u1").unwrap();
        assert_eq!(filter.get_str("recipient_id").unwrap(), "u1");
        assert_eq!(
            filter.get_array("$or").unwrap()[1],
            doc! { "created_at": 42_i64, "id": { "$lt": "b" } }.into()
        );
        assert!(InboxQuery { before_created_at: Some(42), before_id: None }.filter("u1").is_err());
        assert!(
            InboxQuery { before_created_at: Some(-1), before_id: Some("b".into()) }.filter("u1").is_err()
        );
        assert_eq!(InboxQuery::default().filter("u2").unwrap(), doc! { "recipient_id": "u2" });
    }

    #[test]
    fn page_preserves_order_and_reports_continuation() {
        let messages = (0..21)
            .map(|id| Notification {
                base: BaseModel { id: id.to_string(), ..BaseModel::fake() },
                content: NotificationData {
                    dedup_key: id.to_string(),
                    recipient_id: "u1".into(),
                    title: "消息".into(),
                    body: "正文".into(),
                    href: None,
                },
                read_at: None,
            })
            .collect();
        let page = InboxPage::from_records(messages, 30);
        assert!(page.has_more);
        assert_eq!(page.items.len(), 20);
        assert_eq!(page.items.last().unwrap().id, "19");
        assert_eq!(page.unread_count, 30);
        assert!(!InboxPage::from_records(Vec::new(), 0).has_more);
    }
}
