//! 按来源管理记录读取授权及事件队列；业务检查使用 AuthorizationService。
mod command;
pub use command::append_event;
mod queue;
use std::sync::Arc;

use mongodb::Database;

use crate::AuthorizationService;
pub use crate::entity::resource_access::SourceUpdate;
pub use crate::ports::ResourceGrantFacts;
pub use crate::repository::resource_grants::queue::{
    GrantEventRepository as GrantEventService, GrantQueueHealth, GrantRecompute, GrantRule,
};

/// 人工授权及自动来源发布；可信事实由组合根装配。
#[derive(Clone)]
pub struct ResourceGrantService {
    db: Database,
    authorization: Arc<AuthorizationService>,
    facts: Arc<dyn ResourceGrantFacts>,
}
impl ResourceGrantService {
    /// 装配授权来源管理服务。
    /// 可发布及撤销来源的服务。
    ///
    /// 不执行 I/O。
    ///
    /// db 为身份数据库，authorization 为进程共享授权服务，facts 为可信业务事实端口。
    pub fn new(
        db: Database,
        authorization: Arc<AuthorizationService>,
        facts: Arc<dyn ResourceGrantFacts>,
    ) -> Self {
        Self { db, authorization, facts }
    }
}
