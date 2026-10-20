//! 销售全流程及任务办理动态；只读归集，不从展示事件反推业务状态。
mod approval;
mod dto;
mod finance;
mod history;
mod query;
mod repository;
mod sources;
mod task;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
pub use dto::{
    TaskTimeline, TimelineCategory, TimelineEvent, TimelinePage, TimelineQuery, TimelineSalesSource,
};
use erp_identity::SharedRbacService;
use mongodb::Database;
use persistence_core::Executor;
pub use sources::TimelineDocument;

use crate::Result;

/// 由组合层接入各拥有领域的详情权限；不以销售单可见替代财务整单权限。
#[async_trait]
pub trait TimelineAccessPort: Send + Sync {
    /// 重验精确单据的详情范围。
    ///
    /// # 参数
    /// `actor` 为认证账号，`document` 为通过正式外键发现的单据，`executor` 为原事务。
    /// # 返回
    /// 单据全部来源可读时返回 true。
    /// # 错误
    /// 授权配置及基础设施故障继续传播。
    async fn readable(
        &self,
        actor: &AuditActor,
        document: &TimelineDocument,
        executor: &mut dyn Executor,
    ) -> Result<bool>;
}

/// 统一时间线读取服务。
#[derive(Clone)]
pub struct SalesTimelineReadService {
    db: Database,
    rbac: SharedRbacService,
    access: Arc<dyn TimelineAccessPort>,
}

impl SalesTimelineReadService {
    /// 装配数据库与各领域授权入口，不缓存授权结论。
    ///
    /// # 参数
    /// `db`、`rbac`、`access` 由组合根提供。
    /// # 返回
    /// 返回只读服务。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn new(db: Database, rbac: SharedRbacService, access: Arc<dyn TimelineAccessPort>) -> Self {
        Self { db, rbac, access }
    }
}
