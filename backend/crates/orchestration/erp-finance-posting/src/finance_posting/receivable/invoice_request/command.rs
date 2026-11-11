//! 开票申请命令统一使用独立财务回执与一次中文业务事件。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use application_core::{AuditActor, CommandReceipt};
use erp_audit::{
    AuditAction, AuditFact, AuditField, AuditFieldKind, AuditLog, AuditValue, BusinessEventContent,
    BusinessEventContext, BusinessEventResult,
};
use erp_finance::entity::receivable::SalesInvoiceRequest;
use erp_read_models::finance::receivable::invoice_request::InvoiceRequestView;
use persistence_core::Transactional;

use super::super::ReceivableProcess;
use super::authorization::{MongoAccess, RequestAction};
use crate::{Error, Result};

const FIELDS: &[AuditField] =
    &[AuditField { code: "amount", label: "申请金额", kind: AuditFieldKind::Amount }];
pub(super) const SUBMIT: AuditAction = AuditAction {
    code: "sales_invoice_request.submit",
    resource_type: "sales_invoice_request",
    label: "提交开票申请",
    version: 1,
    allowed_fields: FIELDS,
};
pub(super) const CANCEL: AuditAction = AuditAction {
    code: "sales_invoice_request.cancel",
    resource_type: "sales_invoice_request",
    label: "撤回开票申请",
    version: 1,
    allowed_fields: FIELDS,
};

/// 只投影申请编号与金额，不记录请求体、原因原文或命令机器协议。
/// # 参数
/// 当前操作人、已执行的申请事实、明确动作目录及可选稳定命令身份。
/// # 返回
/// 返回一次结构化中文业务事件。
/// # 错误
/// 动作、身份或安全投影不合法时返回错误。
pub(super) fn event(
    actor: &AuditActor,
    request: &SalesInvoiceRequest,
    action: AuditAction,
    command: Option<&CommandReceipt>,
) -> Result<AuditLog> {
    let context = BusinessEventContext::new(actor.clone(), action)?
        .with_command_id(command.map(|value| value.id().to_string()))?;
    Ok(context.log(BusinessEventContent {
        target_id: request.base.id.clone(),
        target_number: Some(request.request_no.clone()),
        result: BusinessEventResult::Succeeded,
        field_changes: vec![],
        facts: vec![AuditFact {
            field: "amount".to_string(),
            value: AuditValue::Amount { value: request.data.amount },
        }],
    })?)
}

impl ReceivableProcess {
    /// 错误恢复只查证原命令；无法读回原对象时保持首次未知提交错误。
    /// # 参数
    /// * `result` - 首次事务结果。
    /// * `command` - 本次请求的精确命令身份。
    /// # 返回
    /// 返回首次结果对象当前视图。
    /// # 错误
    /// 未保存回执时保留原错误；未知提交恢复失败时保留原未知错误。
    pub(super) async fn finish_invoice_request_command(
        &self,
        result: Result<String>,
        command: &CommandReceipt,
        actor: &AuditActor,
    ) -> Result<InvoiceRequestView> {
        match result {
            Ok(id) => committed_result(
                self.authorized_invoice_request_result(&id, actor, RequestAction::from_command(command)?)
                    .await,
            ),
            Err(error) => recover_replay(error, self.replay_invoice_request_command(command, actor).await),
        }
    }

    /// 首次命中与未知提交查证共用当前权限及同快照详情读取，保持零业务写入。
    pub(super) async fn replay_invoice_request_command(
        &self,
        command: &CommandReceipt,
        actor: &AuditActor,
    ) -> Result<Option<InvoiceRequestView>> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        let command = command.clone();
        let evidence = ReplayEvidence::default();
        let observed = evidence.clone();
        let result = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let view =
                        MongoAccess { db: &db, rbac: &rbac }.replay(&actor, &command, executor).await?;
                    observed.observe(&view);
                    Ok::<_, Error>(view)
                })
            })
            .await;
        let view = evidence.finish(result)?;
        Ok(view)
    }

    /// 已完成命令的响应仍须证明当前操作人、动作资格和完整来源读取权。
    pub(super) async fn authorized_invoice_request_result(
        &self,
        id: &str,
        actor: &AuditActor,
        action: RequestAction,
    ) -> Result<InvoiceRequestView> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        let view = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let view =
                        MongoAccess { db: &db, rbac: &rbac }.result(&actor, &id, action, executor).await?;
                    Ok::<_, Error>(view)
                })
            })
            .await?;
        Ok(view)
    }
}

/// 已命中原命令的写事务必须回滚，并把已提交证据带入独立授权回读。
pub(super) fn replay_requires_readback() -> Error {
    Error::CommittedResultUnavailable("开票申请命令已提交，请恢复原结果".into())
}

/// 回调与事务包装层共享已提交证据，避免只读事务结束失败丢失原命令结果。
#[derive(Clone, Default)]
struct ReplayEvidence(Arc<AtomicBool>);

impl ReplayEvidence {
    /// 只记录实际授权回放命中的回执，不把未命中或初始化失败视为已提交。
    fn observe<T>(&self, view: &Option<T>) {
        if view.is_some() {
            self.0.store(true, Ordering::Release);
        }
    }

    /// 原业务命令已提交时，只读事务自身失败不能降级为业务未执行。
    fn finish<T>(&self, result: Result<T>) -> Result<T> {
        if self.0.load(Ordering::Acquire) { committed_result(result) } else { result }
    }
}

/// 已确定提交后的响应失败必须保留原操作号；不把结果不可读伪装为事务未知。
pub(super) fn committed_result<T>(result: Result<T>) -> Result<T> {
    result.map_err(|error| match error {
        Error::CommittedResultUnavailable(_) => error,
        other => Error::CommittedResultUnavailable(other.to_string()),
    })
}

/// 授权回读失败不得把首次未知提交改写为确定未成功，也不得返回无授权视图。
fn recover_replay<T>(original: Error, replay: Result<Option<T>>) -> Result<T> {
    match replay {
        Ok(Some(view)) => Ok(view),
        Ok(None) => Err(original),
        Err(_) if matches!(original, Error::OutcomeUnknown(_) | Error::CommittedResultUnavailable(_)) => {
            Err(original)
        },
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod recovery_tests {
    use mongodb::error::Error as MongoError;
    use persistence_core::Error as PersistenceError;

    use super::*;

    #[test]
    fn raced_receipt_retains_commit_evidence_when_readback_fails() {
        for failure in [
            Error::Forbidden("action revoked".into()),
            Error::RepositoryError(PersistenceError::EntityMetadataOutOfRange("policy unavailable")),
        ] {
            assert!(matches!(
                recover_replay::<String>(replay_requires_readback(), Err(failure)),
                Err(Error::CommittedResultUnavailable(_))
            ));
        }
        assert_eq!(recover_replay(replay_requires_readback(), Ok(Some("authorized"))).unwrap(), "authorized");
    }

    #[test]
    fn readonly_transaction_failure_preserves_only_observed_receipt_evidence() {
        let evidence = ReplayEvidence::default();
        evidence.clone().observe(&Some("authorized"));
        let failure = Error::RepositoryError(PersistenceError::EntityMetadataOutOfRange("commit failed"));
        assert!(matches!(evidence.finish::<String>(Err(failure)), Err(Error::CommittedResultUnavailable(_))));
        assert_eq!(evidence.finish(Ok("authorized")).unwrap(), "authorized");

        let no_receipt = ReplayEvidence::default();
        no_receipt.observe(&None::<String>);
        let failure = Error::RepositoryError(PersistenceError::EntityMetadataOutOfRange("commit failed"));
        assert!(matches!(no_receipt.finish::<String>(Err(failure)), Err(Error::RepositoryError(_))));
    }

    #[test]
    fn known_commit_with_unreadable_result_is_distinct_from_unknown_or_unexecuted() {
        for error in [
            Error::Forbidden("scope changed".into()),
            Error::RepositoryError(PersistenceError::EntityMetadataOutOfRange("read failed")),
        ] {
            let error = committed_result::<String>(Err(error)).unwrap_err();
            assert!(matches!(error, Error::CommittedResultUnavailable(_)));
            assert!(error.command_may_have_committed());
            assert!(std::error::Error::source(&error).is_none());
        }
        assert_eq!(committed_result(Ok("authorized")).unwrap(), "authorized");
    }

    #[test]
    fn unknown_recovery_never_returns_view_after_authorization_failure() {
        let unknown = Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(MongoError::custom(
            "original unknown",
        )));
        let error =
            recover_replay::<String>(unknown, Err(Error::Forbidden("来源范围已变化".into()))).unwrap_err();
        assert!(matches!(error, Error::OutcomeUnknown(_)));
        let committed = Error::CommittedResultUnavailable("original result unreadable".into());
        assert!(
            matches!(recover_replay::<String>(committed, Err(Error::Forbidden("后来撤权".into()))), Err(Error::CommittedResultUnavailable(message)) if message == "original result unreadable")
        );
        let known = Error::ConflictError("提交冲突".into());
        assert!(matches!(
            recover_replay::<String>(known, Err(Error::Forbidden("权限已撤销".into()))),
            Err(Error::Forbidden(_))
        ));
        assert_eq!(
            recover_replay(Error::ConflictError("已提交".into()), Ok(Some("authorized"))).unwrap(),
            "authorized"
        );
    }
}
