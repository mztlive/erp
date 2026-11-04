//! 通知统一入口；站内信由本域持久化，站外投递通过注入的渠道端口执行。
mod dto;
mod entity;
mod error;
pub mod indexes;
pub mod ports;
mod refresh;
mod repository;
mod service;
pub use dto::{InboxPage, InboxQuery, NotificationView};
pub use entity::{Channel, Notification, NotificationData};
pub use error::{Error, Result};
pub use refresh::{RefreshHub, RefreshSignal, RefreshTopic};
pub use service::NotificationService;
