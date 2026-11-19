//! 全库重置的操作资格和最小身份保留合同。

use application_core::AuditActor;
use persistence_core::{Executor, ResetRetention};

use super::RbacService;
use crate::entity::Permission;
use crate::repository::database_reset;
use crate::{Error, ROOT_ROLE_ID, Result};

impl RbacService {
    /// 全库重置预检要求已认证操作人的登录名为 `admin`，且其直接绑定的内建 `role-root` 角色授予 `*:*`。
    /// 资格不符时拒绝；通过时返回策略快照版本供重置事务比对，账号与角色的有效状态由事务内复核。
    pub async fn authorize_database_reset(&self, actor: &AuditActor) -> Result<u64> {
        if actor.account() != "admin" {
            return Err(Error::Forbidden("仅 admin 超级管理员可以清空演示数据库".into()));
        }
        let permission = Permission::parse("*:*")?;
        let snapshot = self
            .role_permission_snapshot(actor.kind(), actor.id(), std::slice::from_ref(&permission))
            .await?;
        if !snapshot.granting_role_ids(&permission).iter().any(|role| role.as_str() == ROOT_ROLE_ID) {
            return Err(Error::Forbidden("清空数据库需要内建超级管理员权限".into()));
        }
        Ok(snapshot.policy_revision())
    }

    /// 在重置事务内、删除前复核发起人是仍有效的 `admin` 本人且内建 `role-root` 角色仍启用。
    /// 按物理文档身份保留账号、角色、直接绑定、`*:*` 规则和全局策略版本；身份不符、记录缺失或读取失败时拒绝重置。
    pub async fn database_reset_retention(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ResetRetention> {
        database_reset::retention(&self.db, actor, executor).await
    }
}
