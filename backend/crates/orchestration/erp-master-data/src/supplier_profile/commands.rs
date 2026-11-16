//! 供应商资料幂等读取与当前命令资格检查。
use application_core::AuditActor;
use erp_core::ids::SupplierAccountId;
use erp_identity::AccessTarget;
use erp_permissions::{internal, supplier};
use erp_supplier::{SupplierExt, SupplierProfileCommand, SupplierProfileMutationView, command_view};
use persistence_core::{Executor, Transactional};

use super::SupplierProfileProcess;
use crate::{Error, Result};

impl SupplierProfileProcess {
    /// 在同一快照检查既有资料详情功能资格后查询命令结果。
    /// # 参数
    /// idempotency_key 为命令键，actor 为当前操作人。
    /// # 返回
    /// 已授权的稳定结果，缺少记录时返回 None。
    /// # 错误
    /// 详情功能资格或数据库读取失败时拒绝。
    pub async fn command_result(
        &self,
        idempotency_key: &str,
        actor: &AuditActor,
    ) -> Result<Option<SupplierProfileMutationView>> {
        let this = self.clone();
        let key = idempotency_key.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { this.command_result_snapshot(&key, &actor, executor).await })
            })
            .await
    }

    /// 独立结果查询只使用既有详情功能资格，完整读取留在原快照。
    async fn command_result_snapshot(
        &self,
        key: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierProfileMutationView>> {
        self.authorization
            .rbac
            .require(actor, &internal::SUPPLIER_PROFILE_COMMAND_READ, AccessTarget::Operation, executor)
            .await?;
        Ok(self.command_record(key, executor).await?.map(command_view))
    }

    /// 导入重试沿用原创建资格，不增加独立详情菜单要求。
    pub(super) async fn imported_result(
        &self,
        key: &str,
        actor: &AuditActor,
    ) -> Result<Option<SupplierProfileMutationView>> {
        let this = self.clone();
        let key = key.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    if !this
                        .authorization
                        .rbac
                        .has_permission(&actor, &supplier::PROFILE_CREATE, executor)
                        .await?
                    {
                        return Err(Error::Forbidden("没有创建供应商权限".into()));
                    }
                    let Some(command) = this.command_record(&key, executor).await? else {
                        return Ok(None);
                    };
                    if command.operation != "create" {
                        return Err(Error::ConflictError("导入键已用于另一项供应商资料请求".into()));
                    }
                    this.authorize_command(&command, &actor, executor).await?;
                    Ok(Some(command_view(command)))
                })
            })
            .await
    }

    /// 先检查可能的原功能资格，禁止无权限读取命令内容。
    pub(super) async fn command_qualified(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let create =
            self.authorization.rbac.has_permission(actor, &supplier::PROFILE_CREATE, executor).await?;
        let update = self.authorization.rbac.has_permission(actor, &supplier::HANDOVER, executor).await?;
        if create || update {
            Ok(())
        } else {
            Err(Error::Forbidden("没有供应商资料命令权限".into()))
        }
    }

    /// 命令记录必须加入当前事务。
    async fn command_record(
        &self,
        key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierProfileCommand>> {
        Ok(self.db.supplier().profile_command(key, executor).await?)
    }

    /// 按原操作和真实当前责任检查回包资格。
    async fn authorize_command(
        &self,
        command: &SupplierProfileCommand,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        match command.operation.as_str() {
            "create" => {
                if !self.authorization.rbac.has_permission(actor, &supplier::PROFILE_CREATE, executor).await?
                {
                    return Err(Error::Forbidden("没有创建供应商权限".into()));
                }
                let account = self
                    .db
                    .supplier()
                    .account(&SupplierAccountId::new(&command.supplier_id), executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("供应商不存在".into()))?;
                self.authorization
                    .access
                    .require_create(actor, &account.maintainer_user_id, executor)
                    .await?;
                Ok(())
            },
            "update" => Ok(self
                .authorization
                .access
                .require_with(actor, &supplier::HANDOVER, &command.supplier_id, executor)
                .await?),
            _ => Err(Error::ValidationError("未知供应商资料命令类型".into())),
        }
    }

    /// 同执行器核对原请求并授权回放，不执行写入。
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn replay_command(
        &self,
        key: &str,
        operation: &str,
        supplier_id: Option<&str>,
        fingerprint: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierProfileMutationView>> {
        let Some(command) = self.command_record(key, executor).await? else {
            return Ok(None);
        };
        command
            .ensure_replayable(operation, supplier_id, fingerprint)
            .map_err(|error| Error::ConflictError(error.to_string()))?;
        self.authorize_command(&command, actor, executor).await?;
        Ok(Some(command_view(command)))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use erp_party::SensitiveDataCodec;
    use erp_supplier::SupplierAccess;
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    #[tokio::test]
    async fn lookup_and_recovery_cannot_bypass_current_qualification() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("result_guard");
        let authorization = unavailable_authorization();
        let access = SupplierAccess::new(db.clone(), authorization.clone());
        let process = SupplierProfileProcess::new(
            db,
            Arc::new(SensitiveDataCodec::from_secret(b"supplier-result-guard")),
            authorization,
            access,
        );
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let error = process
            .command_result_snapshot("key", &actor, &mut SnapshotExecutor::default())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
        let result = process
            .resolve_transaction_result_with_assets(
                Err(Error::Forbidden("revoked".into())),
                "key",
                "create",
                None,
                "fingerprint",
                &actor,
            )
            .await;
        assert!(matches!(result, Err(Error::Forbidden(message)) if message == "revoked"));
    }
}
