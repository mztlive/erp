//! 客户资料命令幂等记录、事务恢复与返回视图映射。

use application_core::AuditActor;
use erp_audit::execution::recover_command;
use erp_customer::repository::prelude::*;
use erp_customer::{
    CustomerExt, CustomerProfileCommand, CustomerProfileMutationView, CustomerProfileOperation,
    CustomerProfileReplayContext,
};
use erp_identity::AccessTarget;
use erp_permissions::{Permission, customer, internal};
use persistence_core::{Executor, Transactional};

use super::CustomerProfileProcess;
use crate::{Error, Result};

impl CustomerProfileProcess {
    /// 按原幂等键查证客户资料命令时，在同一快照验证详情功能资格。命中时返回稳定结果，无记录时返回 `None`。
    /// 原发起人保留回读例外；他人必须通过关联客户的详情范围。资格、范围或数据库读取失败时不返回结果。
    pub async fn command_result(
        &self,
        idempotency_key: &str,
        actor: &AuditActor,
    ) -> Result<Option<CustomerProfileMutationView>> {
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

    // 原发起人保留既有回读例外；他人回读必须通过真实客户详情范围。
    async fn command_result_snapshot(
        &self,
        key: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerProfileMutationView>> {
        self.authorization
            .require(actor, &internal::CUSTOMER_PROFILE_COMMAND_READ, AccessTarget::Operation, executor)
            .await?;

        let Some(command) = self.command_record(key, executor).await? else {
            return Ok(None);
        };

        if command.initiated_by != actor.id() {
            self.access
                .require_with(actor.clone(), &customer::DETAIL, &command.customer_id, executor)
                .await?;
        }
        Ok(Some(command_view(command)))
    }

    // 首次请求及恢复都使用原事务按幂等键查找客户资料命令，避免回执与授权事实来自不同快照。
    pub(super) async fn command_record(
        &self,
        key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerProfileCommand>> {
        Ok(self.db.customer_profile_commands().find_by_idempotency_key(key, executor).await?)
    }

    // 建档或修订命令重试时，在原事务按幂等键读取回执；先重验回执对象的当前资格，再核对原请求。
    pub(super) async fn replay_command(
        &self,
        context: &CustomerProfileReplayContext,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerProfileMutationView>> {
        let Some(command) = self.command_record(context.idempotency_key(), executor).await? else {
            return Ok(None);
        };
        self.authorize_command(&command, actor, executor).await?;
        Ok(Some(checked_command_view(command, context)?))
    }

    // 创建回放按回执中的真实客户检查原创建资格与当前归属，修订回放沿用更新资格。
    async fn authorize_command(
        &self,
        command: &CustomerProfileCommand,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        match command.operation.as_str() {
            "create" => Ok(self
                .authorization
                .require(
                    actor,
                    &internal::CUSTOMER_CREATED_RESULT_READ,
                    AccessTarget::record(&command.customer_id)?,
                    executor,
                )
                .await?),
            "update" => Ok(self
                .access
                .require_with(actor.clone(), &customer::UPDATE, &command.customer_id, executor)
                .await?),
            _ => Err(Error::ValidationError("客户资料不支持此操作，请从客户详情重新选择".into())),
        }
    }

    // 客户资料写入可能已提交时，才用原请求和幂等键开启新的只读查证，重新检查入口、字段与回执权限。
    // 不重执行写入；不能确认原结果时保留原错误。
    pub(super) async fn resolve_transaction(
        &self,
        transaction: Result<CustomerProfileMutationView>,
        context: &CustomerProfileReplayContext,
        actor: &AuditActor,
        fields: &[Permission],
    ) -> Result<CustomerProfileMutationView> {
        match transaction {
            Err(error) if error.command_may_have_committed() => {
                let fields = fields.to_vec();
                let this = self.clone();
                let context = context.clone();
                let actor = actor.clone();
                let recovered = self
                    .db
                    .client()
                    .with_transaction(move |executor| {
                        Box::pin(async move {
                            match context.operation() {
                                CustomerProfileOperation::Create => {
                                    this.access.require_create(&actor, executor).await?
                                },
                                CustomerProfileOperation::Update => {
                                    this.access
                                        .require_with(
                                            actor.clone(),
                                            &customer::UPDATE,
                                            context.customer_id().ok_or_else(|| {
                                                Error::ValidationError(
                                                    "未选择要修订的客户，请返回客户列表重新选择".into(),
                                                )
                                            })?,
                                            executor,
                                        )
                                        .await?
                                },
                            }
                            this.require_fields(&actor, &fields, executor).await?;

                            this.replay_command(&context, &actor, executor).await
                        })
                    })
                    .await;
                recover_command(error, recovered)
            },
            other => other,
        }
    }
}

// 核对命令与本次请求上下文后，映射为稳定返回视图。
//
// `command`：已提交的客户资料命令；`context`：本次请求的幂等核对字段。
//
// 上下文一致时返回稳定视图。
//
// 幂等键已用于另一项请求时返回冲突。
pub(super) fn checked_command_view(
    command: CustomerProfileCommand,
    context: &CustomerProfileReplayContext,
) -> Result<CustomerProfileMutationView> {
    command.ensure_replay_matches(context).map_err(|_| {
        Error::ConflictError("本次操作编号已用于另一项客户资料办理，请核对原操作记录".to_string())
    })?;
    Ok(command_view(command))
}

// 把命令实体转换为稳定返回视图，不重新核对幂等上下文。
//
// `command`：已提交的客户资料命令。
//
// 返回由命令字段组成的稳定视图。
pub(super) fn command_view(command: CustomerProfileCommand) -> CustomerProfileMutationView {
    CustomerProfileMutationView {
        initiated_by: command.initiated_by,
        customer_id: command.customer_id,
        customer_no: command.customer_no,
        party_id: command.party_id,
        revision_id: command.revision_id,
        revision_no: command.revision_no,
        customer_version: command.customer_version,
        party_version: command.party_version,
        effective_from: command.effective_from.to_string(),
        recorded_at: command.base.created_at,
        change_reason: command.change_reason,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use erp_core::AccountKind;
    use erp_core::common::time::BusinessDate;
    use erp_customer::{CustomerAccess, CustomerProfileCommandData};
    use erp_identity::testing::unavailable_authorization;
    use erp_party::SensitiveDataCodec;
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    #[tokio::test]
    async fn created_receipt_uses_record_authorization_before_any_candidate_organization_read() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("created_customer_receipt_guard");
        let authorization = unavailable_authorization();
        let access = CustomerAccess::new(db.clone(), authorization.clone());
        let process = CustomerProfileProcess::new(
            db,
            Arc::new(SensitiveDataCodec::from_secret(b"customer-created-receipt-guard")),
            access,
            authorization,
        );
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let command = CustomerProfileCommand::new(
            "command",
            CustomerProfileCommandData {
                idempotency_key: "key".into(),
                operation: "create".into(),
                initiated_by: actor.id().into(),
                request_fingerprint: "0".repeat(64),
                customer_id: "persisted-customer".into(),
                customer_no: "KH-1".into(),
                party_id: "party".into(),
                revision_id: "revision".into(),
                revision_no: 1,
                customer_version: 1,
                party_version: 1,
                effective_from: BusinessDate::from_ymd(2026, 11, 16).unwrap(),
                change_reason: "create".into(),
            },
        )
        .unwrap();

        let error = process
            .authorize_command(&command, &actor, &mut SnapshotExecutor::default())
            .await
            .expect_err("回放必须通过记录授权，不能先把创建人组织当作已有客户事实");
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }

    #[tokio::test]
    async fn result_lookup_rejects_unavailable_qualification_before_reading_receipt() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("result_guard");
        let authorization = unavailable_authorization();
        let access = CustomerAccess::new(db.clone(), authorization.clone());
        let process = CustomerProfileProcess::new(
            db,
            Arc::new(SensitiveDataCodec::from_secret(b"customer-result-guard")),
            access,
            authorization,
        );
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);

        let error = process
            .command_result_snapshot("key", &actor, &mut SnapshotExecutor::default())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }

    #[tokio::test]
    async fn rejected_command_is_not_recovered_as_success() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("recovery_guard");
        let authorization = unavailable_authorization();
        let access = CustomerAccess::new(db.clone(), authorization.clone());
        let process = CustomerProfileProcess::new(
            db,
            Arc::new(SensitiveDataCodec::from_secret(b"customer-recovery-guard")),
            access,
            authorization,
        );
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let context = CustomerProfileReplayContext::new(
            "key",
            CustomerProfileOperation::Create,
            None,
            "actor",
            erp_customer::CustomerProfileRequestFingerprint::parse_compatible("0".repeat(64)).unwrap(),
        )
        .unwrap();

        let result =
            process.resolve_transaction(Err(Error::Forbidden("revoked".into())), &context, &actor, &[]).await;
        assert!(matches!(result, Err(Error::Forbidden(message)) if message == "revoked"));
    }
}
