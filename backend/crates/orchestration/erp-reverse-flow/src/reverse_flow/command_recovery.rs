//! 已结束写事务后的只读命令查证结果；提交未知时不得覆盖原未知分类。

use application_core::command_recovery::recover_resource;
use application_core::{AuditActor, CommandReceipt};
use bpm::SubjectRef;
use erp_audit::execution::persist_log;
use erp_audit::{AuditActorLogs, AuditLog};
use erp_core::common::time::Instant;
use erp_returns::ReturnsCommandReceiptService;
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshotPayload;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 创建提交事务使用的冻结审批上下文，不替代各单据的授权与来源校验。
pub(super) struct CommitApproval {
    pub(super) subject: SubjectRef,
    pub(super) organization_id: String,
    pub(super) snapshot: ApprovalSubjectSnapshotPayload,
    pub(super) owner_role: &'static str,
    pub(super) idempotency_key: String,
    pub(super) now: Instant,
}

// 准备逆向建单并提交时的业务审计。两条事件关联原命令和实际结果，先创建（序号 1）、再提交（序号 2）；
// 身份、动作目录、关联或序号不合法时返回原错误。
pub(super) fn commit_audits(
    actor: &AuditActor,
    receipt: &CommandReceipt,
    resource_id: &str,
) -> Result<(AuditLog, AuditLog)> {
    let resource_type = receipt.resource_type();
    let (create_action, submit_action) = match resource_type {
        "customer_refund" => ("customer_refund.create", "customer_refund.submit"),
        "supplier_refund" => ("supplier_refund.create", "supplier_refund.submit"),
        "payment_reversal" => ("payment_reversal.create", "payment_reversal.submit"),
        "receipt_reversal" => ("receipt_reversal.create", "receipt_reversal.submit"),
        _ => return Err(Error::Internal("逆向创建提交审计资源类型不受支持".into())),
    };

    let create = actor
        .clone()
        .resource_log(create_action, resource_type, resource_id.to_string())?
        .with_command_id(Some(receipt.id().to_string()))?
        .with_event_sequence(1)?;
    let submit = actor
        .clone()
        .resource_log(submit_action, resource_type, resource_id.to_string())?
        .with_command_id(Some(receipt.id().to_string()))?
        .with_event_sequence(2)?;
    Ok((create, submit))
}

// 逆向审批启动完成后，在原命令执行器依次写入创建审计、提交审计和独立结果回执，回执关联提交事件。
// 任一步失败时返回原错误，由根事务回滚。
pub(super) async fn save_commit(
    db: &Database,
    receipt: &CommandReceipt,
    resource_id: &str,
    audits: (&AuditLog, &AuditLog),
    executor: &mut dyn Executor,
) -> Result<()> {
    persist_log(db, audits.0, executor).await?;
    persist_log(db, audits.1, executor).await?;
    ReturnsCommandReceiptService::new(db.clone())
        .save_resource(receipt, resource_id.to_string(), audits.1.base.id.clone(), executor)
        .await?;
    Ok(())
}

// 写事务结束后合并原错误与原命令的只读查证；确认已提交则返回正式结果 ID。
// 未命中保留原错误，提交未知且查证失败仍保留首次未知错误，不启动写事务。
pub(crate) fn recovered_resource(original: Error, recovered: Result<Option<String>>) -> Result<String> {
    recover_resource(original, recovered, |error| matches!(error, Error::OutcomeUnknown(_)))
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use mongodb::error::Error as MongoError;
    use persistence_core::Error as PersistenceError;

    use super::*;

    #[test]
    fn commit_factory_links_events_to_one_command_in_write_order() {
        let actor = AuditActor::new("actor-1".into(), "actor".into(), AccountKind::Admin);
        for resource_type in ["customer_refund", "supplier_refund", "payment_reversal", "receipt_reversal"] {
            let receipt = CommandReceipt::from_payload(
                "reverse-commit-",
                actor.id(),
                &format!("{resource_type}.commit"),
                resource_type,
                "operation-1",
                &serde_json::json!({ "source_fact_id": "source-1" }),
            )
            .unwrap();
            let (create, submit) = commit_audits(&actor, &receipt, "result-1").unwrap();
            assert_eq!(create.action, format!("{resource_type}.create"));
            assert_eq!(submit.action, format!("{resource_type}.submit"));
            assert_eq!(create.resource_id.as_deref(), Some("result-1"));
            assert_eq!(submit.resource_id, create.resource_id);
            assert_ne!(create.base.id, submit.base.id);

            let created = create.structured_event.unwrap();
            let submitted = submit.structured_event.unwrap();
            assert_eq!(created.command_id.as_deref(), Some(receipt.id()));
            assert_eq!(submitted.command_id, created.command_id);
            assert_eq!(created.event_sequence.get(), 1);
            assert_eq!(submitted.event_sequence.get(), 2);
        }
    }

    #[test]
    fn commit_factory_rejects_other_resource_catalogs() {
        let actor = AuditActor::new("actor-1".into(), "actor".into(), AccountKind::Admin);
        let receipt = CommandReceipt::from_payload(
            "reverse-commit-",
            actor.id(),
            "supplier.commit",
            "supplier",
            "operation-1",
            &serde_json::json!({ "name": "supplier" }),
        )
        .unwrap();
        assert!(matches!(
            commit_audits(&actor, &receipt, "result-1"),
            Err(Error::Internal(message)) if message == "逆向创建提交审计资源类型不受支持"
        ));
    }

    #[test]
    fn lookup_is_only_result_recovery_and_preserves_unknown_when_it_fails() {
        assert_eq!(
            recovered_resource(Error::ConflictError("race".into()), Ok(Some("original".into()))).unwrap(),
            "original"
        );
        assert!(
            matches!(recovered_resource(Error::ConflictError("race".into()), Ok(None)), Err(Error::ConflictError(message)) if message == "race")
        );

        let original =
            Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(MongoError::custom("first commit")));
        assert!(matches!(
            recovered_resource(original, Err(Error::Internal("secondary lookup".into()))),
            Err(Error::OutcomeUnknown(_))
        ));
    }
}
