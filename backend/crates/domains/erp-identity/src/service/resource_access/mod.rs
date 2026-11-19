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

/// 管理记录读取授权来源，供管理员维护人工授权及后台处理器发布自动来源。
#[derive(Clone)]
pub struct ResourceGrantService {
    // 账号、授权来源、授权行及事件所在的身份数据库。
    db: Database,
    // 维护入口用来统一检查管理资格的进程共享授权服务。
    authorization: Arc<AuthorizationService>,
    // 由组合根装配的可信业务事实端口，供来源发布和读取目标校验使用。
    facts: Arc<dyn ResourceGrantFacts>,
}
impl ResourceGrantService {
    /// 装配时构造授权来源管理服务，仅保存身份数据库、进程共享授权服务和可信业务事实端口。
    pub fn new(
        db: Database,
        authorization: Arc<AuthorizationService>,
        facts: Arc<dyn ResourceGrantFacts>,
    ) -> Self {
        Self { db, authorization, facts }
    }
}
