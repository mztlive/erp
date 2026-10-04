//! 已结束写事务后的只读命令查证结果；提交未知时不得覆盖原未知分类。

use application_core::{AuditActor, CommandReceipt};
use erp_audit::{AuditActorLogs, AuditLog};
use erp_returns::ReturnsCommandReceiptService;
use mongodb::Database;
use persistence_core::Executor;

use crate::audit::persist_log;
use crate::{Error, Result};

/// 按实际创建、提交顺序生成同一逆向命令的两条业务事件。
///
/// # 参数
/// 操作人、已经规范化的命令收据和实际创建的结果主键。
/// # 返回
/// 返回共用命令编号、序号分别为 1 和 2 的创建与提交事件。
/// # 错误
/// 审计身份、动作目录、命令关联或事件序号不合法时返回原错误。
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

/// 在原事务内先保存创建、提交审计，再保存逆向命令结果。
///
/// # 参数
/// 数据库、命令收据、实际结果主键、两条原业务审计及同一执行器。
/// # 返回
/// 两条业务审计及独立命令收据均持久化成功。
/// # 错误
/// 任一步持久化失败时返回原错误，由外层事务回滚。
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

/// 将只读查证结果与原提交错误合并，不开启写事务。
///
/// # 参数
/// * `original` - 首次事务错误。
/// * `recovered` - 原命令的只读查证结果。
/// # 返回
/// 查证命中时返回原正式结果 ID。
/// # 错误
/// 未命中返回原错误；提交未知且查证失败仍返回原未知错误。
pub(crate) fn recovered_resource(original: Error, recovered: Result<Option<String>>) -> Result<String> {
    match recovered {
        Ok(Some(id)) => Ok(id),
        Ok(None) => Err(original),
        Err(_) if matches!(original, Error::OutcomeUnknown(_)) => Err(original),
        Err(error) => Err(error),
    }
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
