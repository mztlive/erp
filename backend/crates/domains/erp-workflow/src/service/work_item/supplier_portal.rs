//! 供应商申请提交、决定及撤回事务使用的工作项步骤。

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::WorkItemId;
use persistence_core::Executor;

use super::WorkItemService;
use crate::entity::work_item::{
    SupplierPortalReviewIdentity, SupplierPortalReviewTaskData, WorkItem, WorkItemStatus,
};
use crate::error::{Error, Result};
use crate::ports::WorkflowAuthorizationPort;
use crate::repository::WorkItemExt;
use crate::repository::prelude::*;

impl<A: WorkflowAuthorizationPort> WorkItemService<A> {
    /// 在申请提交事务内创建指定采购负责人的单人确认任务。
    /// 返回已验证读取范围及商品/供给业务确认资格并持久化的开放任务。
    ///
    /// 账号、权限、对象范围、申请版本或唯一性不满足时返回错误。
    ///
    /// `task_id`：提交过程冻结的稳定任务 ID；`data`：申请提交身份及具体采购责任。
    /// `executor`：保存申请快照的同一事务执行器。
    pub async fn create_supplier_portal_review(
        &self,
        task_id: WorkItemId,
        data: SupplierPortalReviewTaskData,
        executor: &mut dyn Executor,
    ) -> Result<WorkItem> {
        let task = WorkItem::new_supplier_portal_review(task_id, data)?;
        let actor =
            self.supplier_portal_reviewer(task.owner_user_id.as_deref().unwrap_or(""), executor).await?;
        self.db.work_items().create(&task, executor).await?;
        self.ensure_decision_access(&actor, &task, executor).await?;
        Ok(task)
    }

    /// 在决定事务内重读任务并验证申请、任务及当前处理资格。
    /// 返回仍开放、身份匹配且当前操作人具备业务确认资格的任务。
    ///
    /// 任务或申请版本变化、账号失效、非当前负责人或对象越界时拒绝。
    ///
    /// `identity`：请求冻结的申请提交及任务版本；`actor`：本次内部采购确认人。
    /// `executor`：决定写入的事务执行器。
    pub async fn validate_supplier_portal_review(
        &self,
        identity: SupplierPortalReviewIdentity<'_>,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<WorkItem> {
        let task = self.load_supplier_portal_review(identity, executor).await?;
        self.authorization.require_assigned(actor, &task.base.id, executor).await?;
        if task.owner_user_id.as_deref() != Some(actor.id()) {
            return Err(Error::Forbidden("当前任务已指派给其他人".into()));
        }
        self.ensure_domain_decision_access(actor, &task, executor).await?;
        Ok(task)
    }

    /// 随正式申请通过或退回结果完成确认任务。
    /// 返回完成并持久化后的任务及其新版本。
    ///
    /// 处理资格、版本重验或 CAS 写入失败时返回错误并由调用方回滚。
    ///
    /// `identity`：已冻结的申请与任务版本；`executor`：正式事实、申请结果与审计的同一事务执行器。
    pub async fn complete_supplier_portal_review(
        &self,
        identity: SupplierPortalReviewIdentity<'_>,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<WorkItem> {
        let mut task = self.validate_supplier_portal_review(identity, actor, executor).await?;
        task.complete_supplier_portal_review(actor.id(), Instant::now())?;
        self.db.work_items().update(&mut task, executor).await?;
        Ok(task)
    }

    /// 随已授权的供应商撤回申请关闭确认任务。
    /// 返回关闭后的任务及其新版本，不形成采购通过结果。
    ///
    /// 任务已完成、版本不一致、关闭字段非法或 CAS 写入失败时返回错误。
    ///
    /// `identity`：当前申请提交与任务版本；`actor_id`：申请过程已验证归属和撤回资格的供应商账号。
    /// `reason`：供应商可追溯的撤回说明；`executor`：申请撤回、任务与审计的同一事务执行器。
    pub async fn withdraw_supplier_portal_review(
        &self,
        identity: SupplierPortalReviewIdentity<'_>,
        actor_id: &str,
        reason: &str,
        executor: &mut dyn Executor,
    ) -> Result<WorkItem> {
        let mut task = self.load_supplier_portal_review(identity, executor).await?;
        task.withdraw_supplier_portal_review(actor_id, reason, Instant::now())?;
        self.db.work_items().update(&mut task, executor).await?;
        Ok(task)
    }

    // 办理供应商确认时沿调用方事务重新读取任务，要求固定对象身份和开放状态仍然成立。
    async fn load_supplier_portal_review(
        &self,
        identity: SupplierPortalReviewIdentity<'_>,
        executor: &mut dyn Executor,
    ) -> Result<WorkItem> {
        let task = self
            .db
            .work_items()
            .find_work_item(identity.task_id, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("供应商申请确认任务已变化，请刷新后重试".into()))?;
        if !task.matches_supplier_portal_review(identity) || task.status != WorkItemStatus::Open {
            return Err(Error::ConflictError("供应商申请或确认任务版本已变化，请刷新后重试".into()));
        }
        Ok(task)
    }

    // 供应商申请进入确认任务前解析仍启用的内部经办人，不允许外部供应商账号承担采购责任。
    async fn supplier_portal_reviewer(
        &self,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<AuditActor> {
        let account = self
            .identity
            .directory()
            .account(actor_id, executor)
            .await?
            .filter(|account| account.is_active_backoffice())
            .ok_or_else(|| Error::Forbidden("供应商申请确认人不是当前启用的内部账号".into()))?;
        Ok(AuditActor::new(account.id, account.login_account, account.kind))
    }
}
