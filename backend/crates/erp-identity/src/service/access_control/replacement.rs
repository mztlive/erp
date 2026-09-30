//! 岗位范围原子替换：版本核对、完整授权检查、旧动作剥离、新范围与审计同事务。

use application_core::AuditActor;
use id_generator::next_id;
use persistence_core::Executor;
use validator::Validate;

use super::{AccessControlService, ensure_scope_configuration};
use crate::access_control::{DataScope, DataScopeId, ScopeDimension, ScopeTargetMode};
use crate::dto::{DataScopeView, ReplaceDataScopeRequest};
use crate::entity::access_control::scope_replacement::plan_replacement;
use crate::error::{Error, Result};
use crate::repository::prelude::*;
use crate::{AccessControlExt, MongoCasbinAdapter, SharedRbacService};

impl AccessControlService {
    /// 原子替换岗位所选业务动作的数据范围。
    ///
    /// # 参数
    /// * `req` - 新范围及读取配置时的策略版本。
    /// * `actor` - 当前操作人。
    /// # 返回
    /// 返回保存后的新范围。
    /// # 错误
    /// 版本冲突、授权不足、目标非法或写入失败时整次拒绝。
    pub async fn replace_data_scope(
        &self,
        req: ReplaceDataScopeRequest,
        actor: &AuditActor,
    ) -> Result<DataScopeView> {
        req.scope.validate()?;
        let scope = DataScope::new(DataScopeId::new(next_id()), req.scope.into_data())?;
        plan_replacement(&scope, vec![])?;
        let event = self
            .build_audit_event(
                actor,
                "data_scope.replace",
                "data_scope",
                Some(scope.base.id.clone()),
                vec!["scope".into()],
            )
            .await?;
        let service = Self { db: self.db.clone(), rbac: self.rbac.clone(), targets: self.targets.clone() };
        let actor = actor.clone();
        self.with_audited_write(event, move |executor| {
            Box::pin(async move {
                service.replace_scope(scope, req.expected_policy_version, &actor, executor).await
            })
        })
        .await
    }

    /// 在调用方事务内核对快照并写入替换计划。
    async fn replace_scope(
        &self,
        scope: DataScope,
        expected_version: u64,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<DataScopeView> {
        let rbac = self.rbac.clone().ok_or_else(|| Error::Forbidden("未装配范围配置授权".into()))?;
        rbac.ensure_policy_snapshot_with_executor(expected_version, executor).await?;
        self.authorize_replacement(&scope, rbac, actor, executor).await?;
        let existing =
            self.db.data_scopes().list_by_subject(scope.subject_type, &scope.subject_id, executor).await?;
        for mut change in plan_replacement(&scope, existing)? {
            if change.remaining_actions.is_empty() {
                self.db.data_scopes().soft_delete(&mut change.original, executor).await?;
            } else {
                change.original.binding.actions = change.remaining_actions;
                self.db.data_scopes().update(&mut change.original, executor).await?;
            }
        }
        self.db.data_scopes().create(&scope, executor).await?;
        MongoCasbinAdapter::new(self.db.clone()).bump_policy_revision(executor).await?;
        Ok(scope.into())
    }

    /// 替换必须同时具备新增与删除配置资格，并验证目标身份。
    async fn authorize_replacement(
        &self,
        scope: &DataScope,
        rbac: SharedRbacService,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        ensure_scope_configuration(&self.db, rbac, actor, scope, "replace", executor).await?;
        if scope.binding.target_mode == Some(ScopeTargetMode::Explicit)
            && scope.binding.target_dimension != ScopeDimension::InternalOrg
        {
            self.targets
                .as_ref()
                .ok_or_else(|| Error::ValidationError("外部范围目标校验未装配".into()))?
                .validate_targets(scope.binding.target_dimension, &scope.scope_targets, executor)
                .await?;
        }
        Ok(())
    }
}
