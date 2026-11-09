//! 只读入口不具备发布来源能力。
use std::sync::Arc;

use async_trait::async_trait;
use mongodb::Database;
use persistence_core::Executor;

use super::{ResourceAccessService, ResourceGrantFacts};
use crate::entity::resource_access::{GrantTarget, SourceUpdate};
use crate::{Error, Result, SharedRbacService};
struct ReadOnlyFacts;
#[async_trait]
impl ResourceGrantFacts for ReadOnlyFacts {
    async fn validate_targets(&self, _: &[GrantTarget], _: &mut dyn Executor) -> Result<()> {
        Err(Error::Forbidden("只读授权服务不能发布授权".into()))
    }
    async fn validate_source(&self, _: &SourceUpdate, _: &mut dyn Executor) -> Result<()> {
        Err(Error::Forbidden("只读授权服务不能发布授权".into()))
    }
}
impl ResourceAccessService {
    /// 装配仅用于查询和目标判定的服务。
    /// # 参数
    /// db、rbac 为共享身份设施。
    /// # 返回
    /// 不执行 I/O 的只读授权服务。
    /// # 错误
    /// 不返回错误；发布来源时失败关闭。
    pub fn reader(db: Database, rbac: SharedRbacService) -> Self {
        Self::new(db, rbac, Arc::new(ReadOnlyFacts))
    }
}
