//! 销售命令的类型化审计边界；身份回放仅使用销售领域回执。

use application_core::{AuditActor, CommandFingerprint};
use async_trait::async_trait;
use erp_audit::{AuditAction, BusinessEventContent, BusinessEventContext, BusinessEventResult};
use erp_sales::dto::sales_order::SubmissionView;
use erp_sales::entity::command_receipt::{SalesCommandReceipt, SalesCommandResult};
use erp_sales::repository::prelude::*;
use erp_sales::repository::{SalesCommandExt, SalesOrderExt};
use mongodb::Database;
use persistence_core::Executor;

use super::authorization::SalesCommandAccess;
use crate::audit::{AuditedCommand, AuditedWrite, MongoAuditEventSink, execute_audited};
use crate::{Error, Result};

/// 写入前验证的销售动作与独立命令结果。
pub(super) struct SalesCommandEvent {
    pub receipt: SalesCommandReceipt,
    context: BusinessEventContext,
    content: BusinessEventContent,
}

impl SalesCommandEvent {
    /// 校验静态动作与身份，提前固定安全历史快照。
    /// # 参数
    /// 稳定命令 ID、操作人、原键、请求摘要、强类型结果和业务编号。
    /// # 返回
    /// 返回同事务执行包装的输入。
    /// # 错误
    /// 动作身份或结果无效时拒绝构造。
    pub(super) fn new(
        id: String,
        actor: &AuditActor,
        key: &str,
        fingerprint: String,
        result: SalesCommandResult,
        number: String,
    ) -> Result<Self> {
        let (code, resource_type, label, target_id) = match &result {
            SalesCommandResult::Created { sales_order_id } => {
                ("sales_order.create", "sales_order", "创建销售单", sales_order_id.to_string())
            },
            SalesCommandResult::Submitted { submission_id, .. } => {
                ("sales_order.submit", "sales_order_submission", "提交销售单", submission_id.to_string())
            },
            SalesCommandResult::HandedOver { sales_order_id } => {
                ("sales_order.handover", "sales_order", "交接销售责任", sales_order_id.to_string())
            },
        };
        let action = AuditAction { code, resource_type, label, version: 1, allowed_fields: &[] };
        let context = BusinessEventContext::new(actor.clone(), action)?.with_command_id(Some(id.clone()))?;
        let receipt = SalesCommandReceipt::new(
            id,
            actor.id(),
            key,
            fingerprint,
            result,
            context.event_id().to_string(),
        )?;
        let content = BusinessEventContent {
            target_id,
            target_number: Some(number),
            result: BusinessEventResult::Succeeded,
            field_changes: Vec::new(),
            facts: Vec::new(),
        };
        Ok(Self { receipt, context, content })
    }

    /// 给同一外层命令的成功事件登记关联与实际写入序号，领域回执身份保持独立。
    /// # 参数
    /// `command_id` 是外层命令 ID，`sequence` 是从1开始的写入次序。
    /// # 返回
    /// 返回保持原事件 ID 和领域回执的事件输入。
    /// # 错误
    /// 命令 ID 非法或序号为零时拒绝。
    pub(super) fn with_command_sequence(mut self, command_id: &str, sequence: u32) -> Result<Self> {
        self.context =
            self.context.with_command_id(Some(command_id.to_string()))?.with_event_sequence(sequence)?;
        Ok(self)
    }

    /// 复用销售事实写入的执行器记录回执和成功事件。
    /// # 参数
    /// `db` 为目标库；`executor` 为调用方事务。
    /// # 返回
    /// 成功时返回空结果。
    /// # 错误
    /// 回执或审计失败停止事务。
    pub(super) async fn persist(&self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        execute_audited(
            &self.context,
            &MongoAuditEventSink::new(db),
            executor,
            &RecordCommand { db, event: self },
        )
        .await
    }
}

struct RecordCommand<'a> {
    db: &'a Database,
    event: &'a SalesCommandEvent,
}

#[async_trait]
impl AuditedCommand for RecordCommand<'_> {
    type Output = ();
    async fn execute(&self, executor: &mut dyn Executor) -> Result<AuditedWrite<()>> {
        self.db
            .sales_command_receipts()
            .create(&self.event.receipt, executor)
            .await
            .map_err(receipt_write_error)?;
        Ok(AuditedWrite::Fresh { result: (), content: self.event.content.clone() })
    }
}

/// 在调用方执行器内读取并验证完整命令身份，不将删除的回执当作未执行。
///
/// # 参数
/// 稳定命令 ID、认证账号、固定动作、原销售单作用域；`fingerprint` 同时携带
/// 规范化请求指纹和从本次请求形成的幂等键摘要。
/// # 返回
/// 命中返回已验证的强类型回执，没有回执返回 None。
/// # 错误
/// 异载荷、身份损坏或读取失败时停止回放。
pub(super) async fn load_command_receipt(
    db: &Database,
    command_id: &str,
    actor_id: &str,
    action: &str,
    scope_id: Option<&str>,
    fingerprint: (&str, &CommandFingerprint),
    executor: &mut dyn Executor,
) -> Result<Option<SalesCommandReceipt>> {
    let receipt = db.sales_command_receipts().find_by_id_including_deleted(command_id, executor).await?;
    if let Some(receipt) = receipt.as_ref() {
        validate_command_replay(
            receipt,
            command_id,
            actor_id,
            action,
            scope_id,
            fingerprint.0,
            fingerprint.1,
        )?;
    }
    Ok(receipt)
}

/// 同执行器恢复精确提交快照和原行，查证后重验当前提交资格。
pub(super) async fn replay_submission_with_executor(
    db: &Database,
    command_id: &str,
    fingerprint: (&str, &CommandFingerprint),
    sales_order_id: &str,
    actor_id: &str,
    access: &SalesCommandAccess,
    executor: &mut dyn Executor,
) -> Result<Option<SubmissionView>> {
    let Some(receipt) = load_command_receipt(
        db,
        command_id,
        actor_id,
        "sales_order.submit",
        Some(sales_order_id),
        fingerprint,
        executor,
    )
    .await?
    else {
        return Ok(None);
    };
    let SalesCommandResult::Submitted { submission_id, .. } = receipt.result else {
        return Err(Error::Internal("销售提交回执结果种类无效".to_string()));
    };
    let submission = db
        .sales_order_submissions()
        .find_by_id(submission_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::Internal("销售提交幂等收据对应快照缺失".to_string()))?;
    validate_submission_fact(
        sales_order_id,
        submission_id.as_ref(),
        actor_id,
        submission.sales_order_id.as_ref(),
        &submission.base.id,
        &submission.submitted_by,
    )?;
    let lines =
        db.sales_order_submission_lines().list_lines_by_submissions(&[submission_id], executor).await?;
    access.current(sales_order_id, executor).await?;
    Ok(Some(erp_sales::service::sales_order::mapper::submission_view(submission, lines)))
}

/// 已冻结销售单、快照及提交人三项一致后，才允许返回原快照。
fn validate_submission_fact(
    expected_order: &str,
    expected_submission: &str,
    expected_actor: &str,
    persisted_order: &str,
    persisted_submission: &str,
    persisted_actor: &str,
) -> Result<()> {
    if expected_order != persisted_order
        || expected_submission != persisted_submission
        || expected_actor != persisted_actor
    {
        return Err(Error::Internal("销售提交幂等收据与业务对象不一致".to_string()));
    }
    Ok(())
}

/// 将领域匹配结果映射为原入口的稳定异载荷错误，完整身份检查始终先于载荷。
fn validate_command_replay(
    receipt: &SalesCommandReceipt,
    command_id: &str,
    actor_id: &str,
    action: &str,
    scope_id: Option<&str>,
    fingerprint: &str,
    expected_key_hash: &CommandFingerprint,
) -> Result<()> {
    receipt.matches(command_id, actor_id, action, scope_id, fingerprint, expected_key_hash).map_err(|error| {
        if matches!(error, erp_sales::Error::ConflictError(_)) {
            let message = match action {
                "sales_order.create" => "同一幂等键已用于不同的销售建单命令",
                "sales_order.submit" => "同一幂等键已用于不同的销售提交",
                "sales_order.handover" => "同一幂等键已用于不同的销售交接",
                _ => "同一操作号已用于不同的销售命令",
            };
            Error::ConflictError(message.to_string())
        } else {
            Error::from(error)
        }
    })
}

/// 只读查证命中才返回原结果；查证失败不得覆盖首次提交未知分类。
pub(super) fn finish_receipt_recovery<T>(original: Error, recovered: Result<Option<T>>) -> Result<T> {
    crate::audit::recover_command(original, recovered)
}

/// 独立销售回执的唯一竞争进入原只读恢复路径，其他写入错误按原口径返回。
fn receipt_write_error(error: persistence_core::Error) -> Error {
    if error.duplicate_index_name() == Some("uk_sales_command_receipts_id") {
        Error::ReceiptDuplicate(error)
    } else {
        Error::from(error)
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_core::ids::{SalesOrderId, SalesOrderSubmissionId};
    use mongodb::error::Error as MongoError;
    use persistence_core::Error as PersistenceError;

    use super::*;

    /// 创建并提交使用同一外层命令关联，原事件与回执 ID 各自稳定。
    #[test]
    fn sales_events_keep_command_correlation_and_original_write_order() {
        let actor = AuditActor::new("actor".into(), "sales".into(), AccountKind::Admin);
        let results = [
            SalesCommandResult::Created { sales_order_id: SalesOrderId::new("order") },
            SalesCommandResult::Submitted {
                sales_order_id: SalesOrderId::new("order"),
                submission_id: SalesOrderSubmissionId::new("snapshot"),
            },
        ];
        let mut event_ids = Vec::new();
        for (sequence, result) in [1, 2].into_iter().zip(results) {
            let event = SalesCommandEvent::new(
                format!("receipt-{sequence}"),
                &actor,
                "key",
                "a".repeat(64),
                result,
                "XS001".into(),
            )
            .unwrap()
            .with_command_sequence("outer-command", sequence)
            .unwrap();
            let log = event.context.log(event.content.clone()).unwrap();
            let structured = log.structured_event.unwrap();
            assert_eq!(structured.command_id.as_deref(), Some("outer-command"));
            assert_eq!(structured.event_sequence.get(), sequence);
            assert_eq!(event.receipt.base.id, format!("receipt-{sequence}"));
            assert_eq!(log.base.id, event.receipt.audit_event_id);
            event_ids.push(log.base.id);
        }
        assert_ne!(event_ids[0], event_ids[1]);
    }

    /// 生产回放 helper 对创建、提交与交接的固定身份和原结果种类逐项查证。
    #[test]
    fn replay_guard_checks_complete_identity_before_payload() {
        for (action, scope, result) in [
            (
                "sales_order.create",
                None,
                SalesCommandResult::Created { sales_order_id: SalesOrderId::new("order") },
            ),
            (
                "sales_order.submit",
                Some("order"),
                SalesCommandResult::Submitted {
                    sales_order_id: SalesOrderId::new("order"),
                    submission_id: SalesOrderSubmissionId::new("snapshot"),
                },
            ),
            (
                "sales_order.handover",
                Some("order"),
                SalesCommandResult::HandedOver { sales_order_id: SalesOrderId::new("order") },
            ),
        ] {
            let receipt = SalesCommandReceipt::new(
                "command".into(),
                "actor",
                "key",
                "a".repeat(64),
                result,
                "event".into(),
            )
            .unwrap();
            validate_command_replay(
                &receipt,
                "command",
                "actor",
                action,
                scope,
                &"a".repeat(64),
                &CommandFingerprint::from_parts(["key".into()]),
            )
            .unwrap();
            assert!(matches!(
                validate_command_replay(
                    &receipt,
                    "command",
                    "actor",
                    action,
                    scope,
                    &"b".repeat(64),
                    &CommandFingerprint::from_parts(["key".into()])
                ),
                Err(Error::ConflictError(_))
            ));
            assert!(matches!(
                validate_command_replay(
                    &receipt,
                    "command",
                    "other",
                    action,
                    scope,
                    &"b".repeat(64),
                    &CommandFingerprint::from_parts(["key".into()])
                ),
                Err(Error::Internal(_))
            ));
            assert!(
                validate_command_replay(
                    &receipt,
                    "wrong-command",
                    "actor",
                    action,
                    scope,
                    &"a".repeat(64),
                    &CommandFingerprint::from_parts(["key".into()])
                )
                .is_err()
            );
            assert!(
                validate_command_replay(
                    &receipt,
                    "command",
                    "actor",
                    "other.action",
                    scope,
                    &"a".repeat(64),
                    &CommandFingerprint::from_parts(["key".into()])
                )
                .is_err()
            );
        }
    }

    /// 查证只恢复原结果；没有结果和二次查证失败均保留首次提交未知错误。
    #[test]
    fn recovery_preserves_original_unknown_error_after_lookup_failure() {
        let unknown = || {
            Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(MongoError::custom("first commit")))
        };
        assert_eq!(finish_receipt_recovery(unknown(), Ok(Some("original"))).unwrap(), "original");
        assert!(matches!(
            finish_receipt_recovery::<String>(unknown(), Ok(None)),
            Err(Error::OutcomeUnknown(_))
        ));
        let recovered =
            finish_receipt_recovery::<String>(unknown(), Err(Error::Internal("secondary".into())))
                .unwrap_err();
        let Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(source)) = recovered else {
            panic!("不得覆盖首次未知提交错误");
        };
        assert_eq!(source.get_custom::<&str>(), Some(&"first commit"));
        assert!(
            matches!(finish_receipt_recovery::<String>(Error::ConflictError("first".into()), Err(Error::Forbidden("current access".into()))), Err(Error::Forbidden(message)) if message == "current access")
        );
    }

    /// 生产快照查证必须拒绝替代快照、错销售单及其他提交人。
    #[test]
    fn submission_replay_keeps_the_original_snapshot_order_and_submitter() {
        validate_submission_fact("order", "snapshot", "actor", "order", "snapshot", "actor").unwrap();
        for (order, submission, actor) in [
            ("other", "snapshot", "actor"),
            ("order", "newer-snapshot", "actor"),
            ("order", "snapshot", "other"),
        ] {
            assert!(
                validate_submission_fact("order", "snapshot", "actor", order, submission, actor).is_err()
            );
        }
    }

    /// 只有命令身份唯一竞争可只读恢复，其他业务唯一冲突保留普通冲突。
    #[test]
    fn only_the_owned_receipt_unique_constraint_enters_recovery() {
        use mongodb::error::{ErrorKind, WriteError, WriteFailure};
        let duplicate = |index| {
            let write: WriteError = serde_json::from_value(serde_json::json!({
                "code": 11000,
                "codeName": "DuplicateKey",
                "errmsg": format!("E11000 duplicate key error index: {index} dup key: {{}}"),
                "errInfo": null,
            }))
            .unwrap();
            PersistenceError::from(MongoError::from(ErrorKind::Write(WriteFailure::WriteError(write))))
        };
        assert!(matches!(
            receipt_write_error(duplicate("uk_sales_command_receipts_id")),
            Error::ReceiptDuplicate(_)
        ));
        assert!(matches!(
            receipt_write_error(duplicate("uk_sales_orders_order_no")),
            Error::ConflictError(_)
        ));
    }

    /// 实际 Process 匹配边界校验本次键摘要，合法格式无法绕过损坏检查。
    #[test]
    fn replay_guard_rejects_validly_encoded_key_digest_damage_before_payload() {
        let expected_key_hash = CommandFingerprint::from_parts([" key ".trim().into()]);
        let mut receipt = SalesCommandReceipt::new(
            "command".into(),
            "actor",
            " key ",
            "a".repeat(64),
            SalesCommandResult::HandedOver { sales_order_id: SalesOrderId::new("order") },
            "event".into(),
        )
        .unwrap();
        validate_command_replay(
            &receipt,
            "command",
            "actor",
            "sales_order.handover",
            Some("order"),
            &"a".repeat(64),
            &expected_key_hash,
        )
        .unwrap();
        receipt.idempotency_key_hash = CommandFingerprint::from_parts(["other-key".into()]);
        receipt.validate().unwrap();
        for fingerprint in ["a".repeat(64), "b".repeat(64)] {
            assert!(matches!(
                validate_command_replay(
                    &receipt,
                    "command",
                    "actor",
                    "sales_order.handover",
                    Some("order"),
                    &fingerprint,
                    &expected_key_hash
                ),
                Err(Error::Internal(_))
            ));
        }
    }
}
