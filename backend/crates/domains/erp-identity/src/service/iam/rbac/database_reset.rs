//! 全库重置的操作资格和最小身份保留合同。

use application_core::AuditActor;
use persistence_core::{Executor, ResetRetention};

use super::RbacService;
use crate::entity::Permission;
use crate::repository::database_reset;
use crate::{Error, ROOT_ROLE_ID, Result};

impl RbacService {
    /// 核验直接 root 绑定和全量权限，返回此次授权的策略版本。
    /// # 参数
    /// `actor` 为当前已认证后台账号。
    /// # 返回
    /// 供全库重置事务进行版本比较的快照版本。
    /// # 错误
    /// 操作人不是 admin、缺少 root 绑定或全量权限时返回禁止操作。
    pub async fn authorize_database_reset(&self, actor: &AuditActor) -> Result<u64> {
        if actor.account() != "admin" {
            return Err(Error::Forbidden("仅 admin 超级管理员可以清空演示数据库".into()));
        }
        let permission = Permission::parse("*:*")?;
        let snapshot = self
            .role_permission_snapshot(actor.kind(), actor.id(), std::slice::from_ref(&permission))
            .await?;
        if !snapshot.granting_role_ids(&permission).iter().any(|role| role == ROOT_ROLE_ID) {
            return Err(Error::Forbidden("清空数据库需要内建超级管理员权限".into()));
        }
        Ok(snapshot.policy_revision())
    }

    /// 在重置事务中重新核验账号、root 角色和持久化授权，返回最小保留清单。
    /// # 参数
    /// `actor` 为发起人，`executor` 为重置事务。
    /// # 返回
    /// admin 账号、root 角色、两条 Casbin 策略和全局策略版本的精确保留身份。
    /// # 错误
    /// 账号、角色或策略缺失、停用、删除或读取失败时拒绝重置。
    pub async fn database_reset_retention(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ResetRetention> {
        database_reset::retention(&self.db, actor, executor).await
    }
}
