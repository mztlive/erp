//! 供应商资料幂等读取与当前命令资格检查。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::SupplierAccountId;
use erp_identity::entity::authorization::{FactValue, ObjectFacts};
use erp_identity::{AccessTarget, OrganizationDirectoryService};
use erp_permissions::{Permission, internal, supplier};
use erp_supplier::{SupplierExt, SupplierProfileCommand, SupplierProfileMutationView, command_view};
use persistence_core::{Executor, Transactional};

use super::SupplierProfileProcess;
use crate::{Error, Result};

/// 原写命令的授权输入；创建回放保留请求维护人，修订保留请求供应商。
#[derive(Clone)]
pub(super) enum SupplierCommandTarget {
    Create(String),
    Update(String),
}

impl SupplierCommandTarget {
    /// 原命令操作类型不得由回执内容选择。
    fn operation(&self) -> &str {
        match self {
            Self::Create(_) => "create",
            Self::Update(_) => "update",
        }
    }

    /// 创建未分配供应商 ID，修订必须校验原请求对象。
    fn supplier_id(&self) -> Option<&str> {
        match self {
            Self::Create(_) => None,
            Self::Update(id) => Some(id),
        }
    }

    /// 将原请求与同快照组织事实映射为既有写入口的权限及目标。
    fn authorization(&self, org: Option<String>) -> Result<(Permission, AccessTarget)> {
        match self {
            Self::Create(maintainer) => Ok((
                supplier::PROFILE_CREATE,
                AccessTarget::Candidate(ObjectFacts {
                    resource: "supplier".into(),
                    owner_id: FactValue::Known(Some(maintainer.clone())),
                    org_unit_id: FactValue::Known(Some(
                        org.ok_or_else(|| Error::ValidationError("请先维护负责人的有效主属组织".into()))?,
                    )),
                    ..Default::default()
                }),
            )),
            Self::Update(id) => Ok((supplier::HANDOVER, AccessTarget::record(id)?)),
        }
    }
}

impl SupplierProfileProcess {
    /// 在同一快照检查资料详情功能资格与关联供应商详情范围后查询命令结果。
    /// # 参数
    /// idempotency_key 为命令键，actor 为当前操作人。
    /// # 返回
    /// 已授权的稳定结果，缺少记录时返回 None。
    /// # 错误
    /// 详情功能资格、关联供应商详情范围或数据库读取失败时拒绝。
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

    /// 独立结果查询先核对既有详情功能资格，再核对回执关联供应商范围。
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
        let command = self.command_record(key, executor).await?;
        self.command_result_view(command, actor, executor).await
    }

    /// 回执必须通过关联供应商的当前详情授权后才映射结果。
    async fn command_result_view(
        &self,
        command: Option<SupplierProfileCommand>,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierProfileMutationView>> {
        let Some(command) = command else { return Ok(None) };
        self.authorization
            .access
            .require_with(actor, &supplier::DETAIL, &command.supplier_id, executor)
            .await?;
        Ok(Some(command_view(command)))
    }

    /// 导入重试先按原创建命令授权，再读取同快照回执。
    /// # 参数
    /// key 为导入命令键，actor 为原创建候选的维护人及当前操作人。
    /// # 返回
    /// 导入候选和回执供应商的维护候选均通过原创建命令授权后返回稳定结果，否则返回 None。
    /// # 错误
    /// 原创建候选授权、操作类型核对或回执读取失败时拒绝。
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
                    let target = SupplierCommandTarget::Create(actor.id().to_owned());
                    this.authorize_command(&target, &actor, executor).await?;
                    let Some(command) = this.command_record(&key, executor).await? else {
                        return Ok(None);
                    };
                    if command.operation != "create" {
                        return Err(Error::ConflictError("导入键已用于另一项供应商资料请求".into()));
                    }
                    this.authorize_imported_supplier(&command.supplier_id, &actor, executor).await?;
                    Ok(Some(command_view(command)))
                })
            })
            .await
    }

    /// 导入键按名称共享；禁止用当前导入人的候选资格读取其他责任范围的既有结果。
    async fn authorize_imported_supplier(
        &self,
        supplier_id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let account = self
            .db
            .supplier()
            .account(&SupplierAccountId::new(supplier_id), executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".into()))?;
        let target = SupplierCommandTarget::Create(account.maintainer_user_id);
        self.authorize_command(&target, actor, executor).await
    }

    /// 对原命令使用完整授权服务；候选不得替换成回执关联对象。
    async fn command_qualified(
        &self,
        target: &SupplierCommandTarget,
        org: Option<String>,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (permission, target) = target.authorization(org)?;
        Ok(self.authorization.rbac.require(actor, &permission, target, executor).await?)
    }

    /// 命令记录必须加入当前事务。
    async fn command_record(
        &self,
        key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierProfileCommand>> {
        Ok(self.db.supplier().profile_command(key, executor).await?)
    }

    /// 原创建维护人与其当前有效组织、原修订供应商均在命令读取前授权。
    async fn authorize_command(
        &self,
        target: &SupplierCommandTarget,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let org = match target {
            SupplierCommandTarget::Create(maintainer) => {
                OrganizationDirectoryService::new(self.db.clone())
                    .own_org(maintainer, Instant::now(), executor)
                    .await?
            },
            SupplierCommandTarget::Update(_) => None,
        };
        self.command_qualified(target, org, actor, executor).await
    }

    /// 同执行器核对原请求并授权回放，不执行写入。
    /// # 参数
    /// key、fingerprint 为原请求的幂等身份，target 为原写命令目标；actor、executor 为当前身份及快照。
    /// # 返回
    /// 原命令授权通过且已成功时返回稳定结果，否则返回 None。
    /// # 错误
    /// 原命令授权失败、同键异参或读取失败时拒绝。
    pub(super) async fn replay_command(
        &self,
        key: &str,
        target: &SupplierCommandTarget,
        fingerprint: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierProfileMutationView>> {
        self.authorize_command(target, actor, executor).await?;
        let Some(command) = self.command_record(key, executor).await? else {
            return Ok(None);
        };
        command
            .ensure_replayable(target.operation(), target.supplier_id(), fingerprint)
            .map_err(|error| Error::ConflictError(error.to_string()))?;
        Ok(Some(command_view(command)))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use erp_core::AccountKind;
    use erp_core::common::time::BusinessDate;
    use erp_identity::testing::unavailable_authorization;
    use erp_party::SensitiveDataCodec;
    use erp_supplier::{SupplierAccess, SupplierProfileCommandData};
    use mongodb::Client;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    #[test]
    fn replay_target_preserves_original_create_owner_and_update_record() {
        let create = SupplierCommandTarget::Create("original-maintainer".into());
        let (permission, target) = create.authorization(Some("current-org".into())).unwrap();
        assert_eq!(permission, supplier::PROFILE_CREATE);
        assert_eq!(create.operation(), "create");
        assert_eq!(create.supplier_id(), None);
        let AccessTarget::Candidate(candidate) = target else { panic!("创建必须保留候选目标") };
        assert_eq!(candidate.owner_id.get().unwrap().as_deref(), Some("original-maintainer"));
        assert_eq!(candidate.org_unit_id.get().unwrap().as_deref(), Some("current-org"));
        assert!(candidate.id.is_empty());
        assert!(matches!(create.authorization(None), Err(Error::ValidationError(_))));

        let update = SupplierCommandTarget::Update("original-supplier".into());
        let (permission, target) = update.authorization(None).unwrap();
        assert_eq!(permission, supplier::HANDOVER);
        assert_eq!(update.operation(), "update");
        assert_eq!(update.supplier_id(), Some("original-supplier"));
        let AccessTarget::Records(records) = target else { panic!("修订必须保留记录目标") };
        assert_eq!(records.ids().len(), 1);
        assert!(records.ids().contains("original-supplier"));
    }

    #[tokio::test]
    async fn command_qualification_requires_real_candidate_or_record_authorization_in_snapshot() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("command_guard");
        let authorization = unavailable_authorization();
        let access = SupplierAccess::new(db.clone(), authorization.clone());
        let process = SupplierProfileProcess::new(
            db,
            Arc::new(SensitiveDataCodec::from_secret(b"supplier-command-guard")),
            authorization,
            access,
        );
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        for (target, org) in [
            (SupplierCommandTarget::Create("original-maintainer".into()), Some("org".into())),
            (SupplierCommandTarget::Update("original-supplier".into()), None),
        ] {
            let error = process
                .command_qualified(&target, org.clone(), &actor, &mut NoTransaction)
                .await
                .unwrap_err();
            assert!(
                matches!(error, Error::ValidationError(message) if message == "写入授权必须使用快照事务")
            );
            let error = process
                .command_qualified(&target, org, &actor, &mut SnapshotExecutor::default())
                .await
                .unwrap_err();
            assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
        }
        let error = process
            .replay_command(
                "key",
                &SupplierCommandTarget::Update("original-supplier".into()),
                &"0".repeat(64),
                &actor,
                &mut SnapshotExecutor::default(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }

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
                &SupplierCommandTarget::Create("original-maintainer".into()),
                "fingerprint",
                &actor,
            )
            .await;
        assert!(matches!(result, Err(Error::Forbidden(message)) if message == "revoked"));
    }

    #[tokio::test]
    async fn receipt_result_requires_current_supplier_detail_before_returning_view() {
        let db =
            Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("receipt_detail_guard");
        let authorization = unavailable_authorization();
        let access = SupplierAccess::new(db.clone(), authorization.clone());
        let process = SupplierProfileProcess::new(
            db,
            Arc::new(SensitiveDataCodec::from_secret(b"supplier-receipt-detail-guard")),
            authorization,
            access,
        );
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let command = SupplierProfileCommand::new(
            "command-1",
            SupplierProfileCommandData {
                idempotency_key: "key-1".into(),
                operation: "update".into(),
                request_fingerprint: "0".repeat(64),
                supplier_id: "receipt-supplier".into(),
                supplier_no: "SUP-1".into(),
                revision_id: "revision-1".into(),
                revision_no: 2,
                supplier_version: 3,
                effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                change_reason: "修订".into(),
            },
        )
        .unwrap();
        let error = process
            .command_result_view(Some(command), &actor, &mut SnapshotExecutor::default())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
        let missing =
            process.command_result_view(None, &actor, &mut SnapshotExecutor::default()).await.unwrap();
        assert!(missing.is_none());
    }
}
