//! 原命令无法执行时以同一唯一回执封存拒绝，阻止迟到请求再次写入。

use std::future::Future;
use std::pin::Pin;

use application_core::CommandReceipt;
use erp_audit::{
    AuditLog, BusinessEventContent, BusinessEventContext, BusinessEventResult, registered_action,
};
use erp_identity::PortalActor;
use erp_supply::portal::PortalOfferingService;
use persistence_core::{Executor, Transactional};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::SupplierPortalProcess;
use super::command::scoped_command;
use crate::audit::persist_log;
use crate::{Error, Result};

type ValidationFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

/// 封存的是原操作失败，不代表申请或供给进入任何新的业务状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CommandRejection {
    kind: RejectionKind,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RejectionKind {
    Validation,
    Business,
    Conflict,
}

impl CommandRejection {
    fn from_error(error: Error) -> Result<Self> {
        let (kind, message) = match error {
            Error::ValidationError(message) => (RejectionKind::Validation, message),
            Error::BusinessLogicError(message) => (RejectionKind::Business, message),
            Error::ConflictError(message) if !message.contains("DATA_SCOPE_CHANGED") => {
                (RejectionKind::Conflict, message)
            },
            error => return Err(error),
        };
        Ok(Self { kind, message })
    }

    pub(super) fn into_error(self) -> Error {
        match self.kind {
            RejectionKind::Validation => Error::ValidationError(self.message),
            RejectionKind::Business => Error::BusinessLogicError(self.message),
            RejectionKind::Conflict => Error::ConflictError(self.message),
        }
    }

    fn receipt_value(&self) -> Value {
        json!({"portal_command_outcome":"rejected","rejection":self})
    }
}

/// 已封存的拒绝在所有原命令入口恢复为同一确定错误。
pub(super) fn rejection(value: &Value) -> Result<Option<CommandRejection>> {
    let Some(outcome) = value.get("portal_command_outcome") else {
        return Ok(None);
    };
    if outcome != "rejected" {
        return Err(Error::Internal("门户命令终态回执类型损坏".into()));
    }
    let value = value.get("rejection").ok_or_else(|| Error::Internal("门户拒绝回执缺失".into()))?;
    let rejection: CommandRejection =
        serde_json::from_value(value.clone()).map_err(|_| Error::Internal("门户拒绝回执内容损坏".into()))?;
    if rejection.message.trim().is_empty() {
        return Err(Error::Internal("门户拒绝回执原因缺失".into()));
    }
    Ok(Some(rejection))
}

enum RecoveryDecision {
    Replay(Value),
    Retry,
    Reject(CommandRejection),
}

/// 决策只读取原回执及只读预检结果，不执行或吞掉任何正式业务写入。
async fn recovery_decision(
    receipt: Option<Value>,
    validate: impl Future<Output = Result<()>>,
) -> Result<RecoveryDecision> {
    if let Some(value) = receipt {
        return Ok(RecoveryDecision::Replay(value));
    }
    match validate.await {
        Ok(()) => Ok(RecoveryDecision::Retry),
        Err(error) => Ok(RecoveryDecision::Reject(CommandRejection::from_error(error)?)),
    }
}

impl SupplierPortalProcess {
    pub(super) async fn recover_unexecutable<P, F>(
        &self,
        actor: &PortalActor,
        action: &'static str,
        key: &str,
        payload: &P,
        validate: F,
    ) -> Result<Option<Value>>
    where
        P: Serialize,
        F: for<'a> FnOnce(SupplierPortalProcess, PortalActor, &'a mut dyn Executor) -> ValidationFuture<'a>
            + Send
            + 'static,
    {
        let command = scoped_command(&actor.account_id, &actor.supplier_id, action, key, payload)?;
        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let actor = this.session_validate(&actor, executor).await?;
                    actor.require_write()?;
                    let receipt = PortalOfferingService::new(this.db.clone())
                        .command_result(&command, executor)
                        .await?;
                    match recovery_decision(receipt, validate(this.clone(), actor.clone(), executor)).await? {
                        RecoveryDecision::Replay(value) => Ok(Some(value)),
                        RecoveryDecision::Retry => Ok(None),
                        RecoveryDecision::Reject(rejection) => {
                            let value = rejection.receipt_value();
                            this.commit_rejection(&command, &actor, &value, executor).await?;
                            Ok(Some(value))
                        },
                    }
                })
            })
            .await
    }

    async fn commit_rejection(
        &self,
        command: &CommandReceipt,
        actor: &PortalActor,
        value: &Value,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let log = rejection_log(command, actor)?;
        persist_log(&self.db, &log, executor).await?;
        PortalOfferingService::new(self.db.clone())
            .command_commit(command, &actor.supplier_id, value, executor)
            .await?;
        Ok(())
    }
}

fn rejection_log(command: &CommandReceipt, actor: &PortalActor) -> Result<AuditLog> {
    let context = BusinessEventContext::new(
        actor.audit_actor(),
        registered_action(command.action(), "supplier_portal_request")?,
    )?
    .with_command_id(Some(command.id().to_string()))?;
    Ok(context.log(BusinessEventContent {
        target_id: actor.supplier_id.clone(),
        target_number: None,
        result: BusinessEventResult::Rejected,
        field_changes: Vec::new(),
        facts: Vec::new(),
    })?)
}

#[cfg(test)]
mod tests {
    use erp_supply::portal::PortalCommandReceipt;

    use super::*;

    fn command(key: &str, version: u64) -> CommandReceipt {
        scoped_command(
            "external",
            "supplier",
            "supplier_portal.availability_update",
            key,
            &json!({"expected_version":version,"available_quantity":"2"}),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn existing_success_or_rejection_skips_preflight_and_returns_original_receipt() {
        for receipt in [
            json!({"offering_id":"offering","availability_version":2}),
            CommandRejection::from_error(Error::ConflictError("版本已变化".into())).unwrap().receipt_value(),
        ] {
            let decision = recovery_decision(Some(receipt.clone()), async {
                panic!("原回执已存在，不得预检或执行迟到写入")
            })
            .await
            .unwrap();
            let RecoveryDecision::Replay(value) = decision else { panic!("receipt was not replayed") };
            assert_eq!(value, receipt);
        }
    }

    #[tokio::test]
    async fn only_known_business_preflight_rejections_can_be_sealed() {
        assert!(matches!(recovery_decision(None, async { Ok(()) }).await.unwrap(), RecoveryDecision::Retry));
        for error in [
            Error::ValidationError("数量不能为负".into()),
            Error::BusinessLogicError("当前商品不可供".into()),
            Error::ConflictError("可供版本已变化".into()),
        ] {
            assert!(matches!(
                recovery_decision(None, async { Err(error) }).await.unwrap(),
                RecoveryDecision::Reject(_)
            ));
        }
        for error in [
            Error::Forbidden("资格已撤销".into()),
            Error::Unauthenticated("绑定已失效".into()),
            Error::ConflictError("DATA_SCOPE_CHANGED：权限已变化".into()),
            Error::RepositoryError(persistence_core::Error::OptimisticLockingError),
            Error::Internal("数据库不可用".into()),
        ] {
            assert!(recovery_decision(None, async { Err(error) }).await.is_err());
        }
    }

    #[test]
    fn positive_and_negative_candidates_contend_for_exactly_the_same_immutable_receipt() {
        let original = command("same-key", 1);
        let rejected = CommandRejection::from_error(Error::ConflictError("版本已变化".into())).unwrap();
        let negative = PortalCommandReceipt::new(&original, "supplier", &rejected.receipt_value()).unwrap();
        let positive =
            PortalCommandReceipt::new(&original, "supplier", &json!({"availability_version":2})).unwrap();
        assert_eq!(positive.base.id, negative.base.id);
        assert_eq!(positive.fingerprint, negative.fingerprint);
        let restored = rejection(&negative.replay(&original).unwrap()).unwrap().unwrap().into_error();
        assert!(matches!(restored, Error::ConflictError(message) if message == "版本已变化"));
        assert!(negative.replay(&command("same-key", 2)).is_err());
        assert_ne!(original.id(), command("fresh-key", 2).id());
    }

    #[test]
    fn corrupted_terminal_receipts_fail_closed() {
        assert!(rejection(&json!({"portal_command_outcome":"unknown"})).is_err());
        assert!(rejection(&json!({"portal_command_outcome":"rejected"})).is_err());
        assert!(
            rejection(
                &json!({"portal_command_outcome":"rejected","rejection":{"kind":"conflict","message":""}})
            )
            .is_err()
        );
        assert!(rejection(&json!({"id":"regular-result"})).unwrap().is_none());
    }
}
