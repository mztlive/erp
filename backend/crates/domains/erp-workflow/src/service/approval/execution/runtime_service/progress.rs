//! 发起人未读进展的个人计数及版本确认。

use application_core::AuditActor;
use bpm::ids::ApprovalProcessInstanceId;
use persistence_core::{Executor, NoTransaction, Transactional};

use super::{ApprovalRuntimeService, hidden_not_found};
use crate::entity::approval_progress::ViewedApprovalProgress;
use crate::error::Result;
use crate::ports::WorkflowAuthorizationPort;
use crate::repository::BpmExt;
use crate::service::approval::{approval_action_roles_with_executor, approval_actor_is_active_with_executor};

impl<A: WorkflowAuthorizationPort> ApprovalRuntimeService<A> {
    /// 查询当前有效账号发起的未读审批数量。
    /// # 参数
    /// `actor` 为认证上下文中的账号。
    /// # 返回
    /// 按审批实例去重后的未读数量。
    /// # 错误
    /// 账号停用、没有读取权限或数据库失败时返回错误。
    pub async fn started_unread_count(&self, actor: &AuditActor) -> Result<u64> {
        self.ensure_active_instance_reader(actor).await?;
        Ok(self.db.bpm_workflow().started_unread_count(actor.id(), &mut NoTransaction).await?)
    }

    /// 确认当前账号实际看过的审批进展。
    /// # 参数
    /// `actor` 为登录账号，`instance_id` 为审批实例，`version` 为展示版本。
    /// # 返回
    /// 确认成功返回空；旧确认不会清除更新的提醒。
    /// # 错误
    /// 无权、实例缺失、版本非法或数据库失败时返回错误。
    pub async fn mark_progress_read(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        version: u64,
    ) -> Result<()> {
        let this = self.clone();
        let actor = actor.clone();
        let instance_id = instance_id.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.confirm_progress(&actor, &instance_id, version, executor).await })
            })
            .await
    }

    async fn confirm_progress(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        version: u64,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if !approval_actor_is_active_with_executor(&self.auth, actor, executor).await?
            || approval_action_roles_with_executor(&self.auth, actor, "approval_instance:read", executor)
                .await?
                .is_empty()
        {
            return Err(hidden_not_found());
        }
        let repository = self.db.bpm_workflow();
        let instance = repository
            .find_instance_by_id(&ApprovalProcessInstanceId::new(instance_id), executor)
            .await?
            .ok_or_else(hidden_not_found)?;
        let viewed = ViewedApprovalProgress::new(
            version,
            instance.base.version,
            instance.started_by.as_str(),
            actor.id(),
        )?;
        repository.mark_progress_read(instance_id, actor.id(), viewed, executor).await?;
        Ok(())
    }
}
