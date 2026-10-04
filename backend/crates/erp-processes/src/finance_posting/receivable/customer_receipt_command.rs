//! 客户回款命令的独立财务回执、当前结果恢复与一次结构化业务事件。

use application_core::{AuditActor, CommandReceipt};
use async_trait::async_trait;
use erp_audit::{
    AuditAction, AuditCode, AuditFact, AuditField, AuditFieldKind, AuditLog, AuditValue,
    BusinessEventContent, BusinessEventContext, BusinessEventResult,
};
use erp_finance::entity::receivable::CustomerReceipt;
use erp_finance::service::command_receipt::FinanceCommandReceiptService;
use mongodb::Database;
use persistence_core::Executor;

use crate::audit::{AuditEventSink, AuditedCommand, AuditedWrite, MongoAuditEventSink, execute_audited};
use crate::{Error, Result};

const RECEIPT_FIELDS: &[AuditField] = &[
    AuditField { code: "amount", label: "到账金额", kind: AuditFieldKind::Amount },
    AuditField {
        code: "status",
        label: "回款状态",
        kind: AuditFieldKind::Code(&[
            AuditCode { code: "IN_APPROVAL", label: "审批中" },
            AuditCode { code: "posted", label: "已过账" },
        ]),
    },
];

/// 回款命令结果的窄读取边界，结果形态沿用对应入口。
#[async_trait]
pub(super) trait ReceiptReplayPort: Send + Sync {
    type Output: Send;

    /// 读取领域回执；不存在时返回空结果，异载荷或损坏时失败。
    ///
    /// # 参数
    /// * `command` - 原请求的稳定命令身份。
    /// * `executor` - 调用方执行器。
    /// # 返回
    /// 返回原命令结果资源或空。
    /// # 错误
    /// 身份、载荷、回执损坏或读取失败时返回错误。
    async fn committed_resource_id(
        &self,
        command: &CommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>>;

    /// 沿用入口结果形态回读原业务对象；读取失败时返回原错误。
    ///
    /// # 参数
    /// * `id` - 回执确认的原结果资源。
    /// * `executor` - 当前调用方执行器。
    /// # 返回
    /// 返回当前授权结果。
    /// # 错误
    /// 原业务对象不存在或结果读取失败时返回错误。
    async fn current_result(&self, id: &str, executor: &mut dyn Executor) -> Result<Self::Output>;
}

/// 查证独立回执并回读原结果；命中其他原单时不得读取其业务详情。
///
/// # 参数
/// * `port` - 对应入口的领域回执与结果读取边界。
/// * `command` - 原请求命令身份。
/// * `expected_resource` - 提交已有原单时必须匹配的资源 ID。
/// * `executor` - 调用方读取或事务执行器。
/// # 返回
/// 命中时返回原对象当前结果，否则返回空。
/// # 错误
/// 指纹、原单不一致或回执/结果读取失败时返回错误。
pub(super) async fn replay_receipt_command<P: ReceiptReplayPort>(
    port: &P,
    command: &CommandReceipt,
    expected_resource: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<Option<P::Output>> {
    let Some(id) = port.committed_resource_id(command, executor).await? else {
        return Ok(None);
    };
    ensure_committed_resource(&id, expected_resource)?;
    port.current_result(&id, executor).await.map(Some)
}

/// 提交未知时任何查证失败均保留首次错误；只查证一次，不执行写命令。
///
/// # 参数
/// * `port` - 回执及原结果读取边界。
/// * `command` - 原命令身份。
/// * `original_error` - 事务首次错误。
/// * `executor` - 当前查证执行器。
/// # 返回
/// 查证成功时返回原结果。
/// # 错误
/// 无命中时返回原错误；未知提交查证失败时保留原未知错误。
pub(super) async fn recover_receipt_command<P: ReceiptReplayPort>(
    port: &P,
    command: &CommandReceipt,
    original_error: Error,
    executor: &mut dyn Executor,
) -> Result<P::Output> {
    match replay_receipt_command(port, command, None, executor).await {
        Ok(Some(result)) => Ok(result),
        Ok(None) => Err(original_error),
        Err(_) if matches!(original_error, Error::OutcomeUnknown(_)) => Err(original_error),
        Err(error) => Err(error),
    }
}

/// 原单提交回执只能关联请求中的原单。
///
/// # 参数
/// * `id` - 财务回执记录的结果资源。
/// * `expected_resource` - 原请求资源，可为空。
/// # 返回
/// 资源一致或不限制资源时返回成功。
/// # 错误
/// 关联其他原单时返回冲突。
pub(super) fn ensure_committed_resource(id: &str, expected_resource: Option<&str>) -> Result<()> {
    if expected_resource.is_some_and(|expected| expected != id) {
        return Err(Error::ConflictError("回款提交收据与原单不一致".into()));
    }
    Ok(())
}

/// 首次成功回执与业务事件的事务写入边界。
#[async_trait]
pub(super) trait ReceiptCommandWritePort: AuditEventSink {
    /// 在业务事务中保存首次命令成功回执及其事件关联。
    ///
    /// # 参数
    /// * `command` - 已执行的原命令。
    /// * `resource_id` - 正式结果资源。
    /// * `audit_event_id` - 同事务关联业务事件。
    /// * `executor` - 原业务事务执行器。
    /// # 返回
    /// 写入成功时返回空结果。
    /// # 错误
    /// 回执无效、唯一身份冲突或持久化失败时返回错误。
    async fn save_receipt(
        &self,
        command: &CommandReceipt,
        resource_id: String,
        audit_event_id: String,
        executor: &mut dyn Executor,
    ) -> Result<()>;
}

/// 回款命令真实财务回执与事件存储适配器。
pub(super) struct MongoReceiptCommandWrites<'a> {
    pub(super) db: &'a Database,
}

#[async_trait]
impl AuditEventSink for MongoReceiptCommandWrites<'_> {
    async fn persist(&self, log: &AuditLog, executor: &mut dyn Executor) -> Result<()> {
        MongoAuditEventSink::new(self.db).persist(log, executor).await
    }
}

#[async_trait]
impl ReceiptCommandWritePort for MongoReceiptCommandWrites<'_> {
    async fn save_receipt(
        &self,
        command: &CommandReceipt,
        resource_id: String,
        audit_event_id: String,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        FinanceCommandReceiptService::new(self.db.clone())
            .save_resource(command, resource_id, audit_event_id, executor)
            .await?;
        Ok(())
    }
}

/// 正式提交成功后在原执行器写回执和一次中文事件；不保存原请求或机器消息。
///
/// # 参数
/// * `port` - 回执与成功事件写入边界。
/// * `command` - 原命令身份。
/// * `context` - 业务写入前已验证的动作与身份上下文。
/// * `receipt` - 已完成业务提交的回款实体。
/// * `executor` - 正式业务写入使用的原执行器。
/// # 返回
/// 回执与业务事件成功写入时返回空结果。
/// # 错误
/// 回执、结果投影或事件写入失败时停止并返回原错误。
pub(super) async fn persist_receipt_command_success<P: ReceiptCommandWritePort>(
    port: &P,
    command: &CommandReceipt,
    context: &BusinessEventContext,
    receipt: &CustomerReceipt,
    executor: &mut dyn Executor,
) -> Result<()> {
    let write = ReceiptSuccessCommand { port, command, receipt, audit_event_id: context.event_id() };
    execute_audited(context, port, executor, &write).await
}

struct ReceiptSuccessCommand<'a, P> {
    port: &'a P,
    command: &'a CommandReceipt,
    receipt: &'a CustomerReceipt,
    audit_event_id: &'a str,
}

#[async_trait]
impl<P: ReceiptCommandWritePort> AuditedCommand for ReceiptSuccessCommand<'_, P> {
    type Output = ();

    async fn execute(&self, executor: &mut dyn Executor) -> Result<AuditedWrite<()>> {
        self.port
            .save_receipt(self.command, self.receipt.base.id.clone(), self.audit_event_id.into(), executor)
            .await?;
        Ok(AuditedWrite::Fresh { result: (), content: receipt_content(self.receipt) })
    }
}

/// 回款事件只记录到账金额、业务编号与审批结果状态。
fn receipt_content(receipt: &CustomerReceipt) -> BusinessEventContent {
    BusinessEventContent {
        target_id: receipt.base.id.clone(),
        target_number: Some(receipt.receipt_no.clone()),
        result: BusinessEventResult::Succeeded,
        field_changes: vec![],
        facts: vec![
            AuditFact { field: "amount".into(), value: AuditValue::Amount { value: receipt.amount } },
            AuditFact {
                field: "status".into(),
                value: AuditValue::Code {
                    code: receipt.status.as_str().into(),
                    label: receipt.status.label().into(),
                },
            },
        ],
    }
}

/// 在业务写入前校验动作和静态身份，回执与事件共用原稳定命令 ID。
///
/// # 参数
/// * `command` - 稳定命令身份及动作。
/// * `actor` - 认证操作人。
/// # 返回
/// 返回已验证的业务事件上下文。
/// # 错误
/// 动作未登记、元数据或操作人无效时返回错误。
pub(super) fn receipt_command_context(
    command: &CommandReceipt,
    actor: &AuditActor,
) -> Result<BusinessEventContext> {
    Ok(BusinessEventContext::new(actor.clone(), receipt_action(command)?)?
        .with_command_id(Some(command.id().to_string()))?)
}

/// 动作目录仅包含本文件拥有的两类成功财务命令。
fn receipt_action(command: &CommandReceipt) -> Result<AuditAction> {
    let (code, label) = match command.action() {
        "customer_receipt.commit" => ("customer_receipt.commit", "登记并提交客户回款"),
        "customer_receipt.submit" => ("customer_receipt.submit", "提交客户回款"),
        _ => return Err(Error::Internal("客户回款命令动作无效".into())),
    };
    Ok(AuditAction {
        code,
        resource_type: "customer_receipt",
        label,
        version: 1,
        allowed_fields: RECEIPT_FIELDS,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_core::ids::{CustomerReceiptId, PartyId};
    use erp_finance::entity::command_receipt::FinanceCommandReceipt;
    use erp_finance::entity::receivable::{CustomerReceiptData, CustomerReceiptStatus};
    use mongodb::error::Error as MongoError;
    use persistence_core::{Error as PersistenceError, NoTransaction};

    use super::*;

    fn command(amount: u32) -> CommandReceipt {
        CommandReceipt::from_payload(
            "customer-receipt-commit-",
            "actor",
            "customer_receipt.commit",
            "customer_receipt",
            "key",
            &amount,
        )
        .unwrap()
    }

    fn actor() -> AuditActor {
        AuditActor::new("actor".into(), "fukuan".into(), AccountKind::Admin)
    }

    fn committed_receipt() -> CustomerReceipt {
        let mut receipt = CustomerReceipt::new(
            CustomerReceiptId::new("receipt"),
            CustomerReceiptData {
                receipt_no: "HK202610040001".into(),
                counterparty_party_id: PartyId::new("party"),
                customer_id: None,
                received_at: Instant::now(),
                amount: "100.00".parse().unwrap(),
                bank_reference: Some("private-bank-reference".into()),
            },
            "actor",
        )
        .unwrap();
        receipt.status = CustomerReceiptStatus::InApproval;
        receipt
    }

    struct Probe {
        identity: usize,
        receipt: Option<FinanceCommandReceipt>,
        fail_read: bool,
        fail_result: bool,
        fail_receipt_write: bool,
        fail_audit_write: bool,
        calls: Mutex<Vec<&'static str>>,
        written_receipts: Mutex<Vec<FinanceCommandReceipt>>,
        logs: Mutex<Vec<AuditLog>>,
    }

    impl Probe {
        fn new(executor: &mut NoTransaction) -> Self {
            Self {
                identity: executor as *mut NoTransaction as usize,
                receipt: Some(
                    FinanceCommandReceipt::resource(&command(100), "receipt".into(), "audit".into()).unwrap(),
                ),
                fail_read: false,
                fail_result: false,
                fail_receipt_write: false,
                fail_audit_write: false,
                calls: Mutex::new(vec![]),
                written_receipts: Mutex::new(vec![]),
                logs: Mutex::new(vec![]),
            }
        }

        fn visit(&self, executor: &mut dyn Executor, step: &'static str) {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.identity);
            self.calls.lock().unwrap().push(step);
        }
    }

    #[async_trait]
    impl ReceiptReplayPort for Probe {
        type Output = u32;

        async fn committed_resource_id(
            &self,
            command: &CommandReceipt,
            executor: &mut dyn Executor,
        ) -> Result<Option<String>> {
            self.visit(executor, "read_receipt");
            if self.fail_read {
                return Err(Error::Internal("回执读取不可用".into()));
            }
            self.receipt.as_ref().map(|receipt| receipt.resource_id(command).map_err(Error::from)).transpose()
        }

        async fn current_result(&self, id: &str, executor: &mut dyn Executor) -> Result<u32> {
            self.visit(executor, "current_result");
            assert_eq!(id, "receipt");
            if self.fail_result {
                return Err(Error::NotFound("原回款视图不可用".into()));
            }
            Ok(42)
        }
    }

    #[async_trait]
    impl ReceiptCommandWritePort for Probe {
        async fn save_receipt(
            &self,
            command: &CommandReceipt,
            resource_id: String,
            audit_event_id: String,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            self.visit(executor, "save_receipt");
            if self.fail_receipt_write {
                return Err(Error::ConflictError("原回执写入错误".into()));
            }
            self.written_receipts.lock().unwrap().push(FinanceCommandReceipt::resource(
                command,
                resource_id,
                audit_event_id,
            )?);
            Ok(())
        }
    }

    #[async_trait]
    impl AuditEventSink for Probe {
        async fn persist(&self, log: &AuditLog, executor: &mut dyn Executor) -> Result<()> {
            self.visit(executor, "save_event");
            if self.fail_audit_write {
                return Err(Error::Internal("原事件写入错误".into()));
            }
            self.logs.lock().unwrap().push(log.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn exact_replay_reads_current_result_with_zero_writes() {
        let mut executor = NoTransaction;
        let probe = Probe::new(&mut executor);
        assert_eq!(
            replay_receipt_command(&probe, &command(100), None, &mut executor).await.unwrap(),
            Some(42)
        );
        assert_eq!(*probe.calls.lock().unwrap(), ["read_receipt", "current_result"]);
        assert!(probe.written_receipts.lock().unwrap().is_empty());
        assert!(probe.logs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn missing_conflicting_corrupt_and_cross_resource_receipts_stop_before_result() {
        for case in 0..4 {
            let mut executor = NoTransaction;
            let mut probe = Probe::new(&mut executor);
            if case == 0 {
                probe.receipt = None;
            }
            if case == 2 {
                probe.receipt.as_mut().unwrap().audit_event_id.clear();
            }
            let request = command(if case == 1 { 50 } else { 100 });
            let expected = if case == 3 { Some("other") } else { None };
            let result = replay_receipt_command(&probe, &request, expected, &mut executor).await;
            if case == 0 {
                assert_eq!(result.unwrap(), None);
            } else {
                assert!(result.is_err());
            }
            assert_eq!(*probe.calls.lock().unwrap(), ["read_receipt"]);
        }
    }

    fn unknown_error() -> Error {
        Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(MongoError::custom(
            "original customer receipt unknown",
        )))
    }

    fn assert_original_unknown(error: Error) {
        match error {
            Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(source)) => {
                assert_eq!(source.get_custom::<&str>(), Some(&"original customer receipt unknown"));
            },
            other => panic!("原未知错误被替换: {other:?}"),
        }
    }

    #[tokio::test]
    async fn unknown_commit_keeps_original_on_any_failed_probe_without_reexecution() {
        for case in 0..5 {
            let mut executor = NoTransaction;
            let mut probe = Probe::new(&mut executor);
            match case {
                0 => probe.receipt = None,
                1 => probe.fail_read = true,
                2 => probe.receipt.as_mut().unwrap().audit_event_id.clear(),
                3 => probe.fail_result = true,
                _ => {},
            }
            let request = command(if case == 4 { 50 } else { 100 });
            let error =
                recover_receipt_command(&probe, &request, unknown_error(), &mut executor).await.unwrap_err();
            assert_original_unknown(error);
            assert!(probe.written_receipts.lock().unwrap().is_empty());
            assert!(probe.logs.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn unknown_commit_can_recover_only_the_current_original_result() {
        let mut executor = NoTransaction;
        let probe = Probe::new(&mut executor);
        assert_eq!(
            recover_receipt_command(&probe, &command(100), unknown_error(), &mut executor).await.unwrap(),
            42
        );
        assert_eq!(*probe.calls.lock().unwrap(), ["read_receipt", "current_result"]);
    }

    #[tokio::test]
    async fn fresh_receipt_and_chinese_event_share_executor_and_stable_command() {
        let mut executor = NoTransaction;
        let probe = Probe::new(&mut executor);
        let command = command(100);
        let context = receipt_command_context(&command, &actor()).unwrap();
        persist_receipt_command_success(&probe, &command, &context, &committed_receipt(), &mut executor)
            .await
            .unwrap();
        assert_eq!(*probe.calls.lock().unwrap(), ["save_receipt", "save_event"]);
        let receipts = probe.written_receipts.lock().unwrap();
        let logs = probe.logs.lock().unwrap();
        assert_eq!(receipts.len(), 1);
        assert_eq!(logs.len(), 1);
        assert_eq!(receipts[0].audit_event_id, logs[0].base.id);
        assert_eq!(receipts[0].resource_id(&command).unwrap(), "receipt");
        let event = logs[0].structured_event.as_ref().unwrap();
        assert_eq!(event.command_id.as_deref(), Some(command.id()));
        assert_eq!(event.resource_number_snapshot.as_deref(), Some("HK202610040001"));
        assert_eq!(event.action_label, "登记并提交客户回款");
        assert_eq!(event.facts.len(), 2);
        let message = logs[0].message.as_ref().unwrap();
        assert!(message.contains("到账金额：100.00元"));
        assert!(message.contains("回款状态：审批中"));
        assert!(!message.contains("private-bank-reference"));
    }

    #[tokio::test]
    async fn receipt_or_event_failure_stops_with_original_error() {
        for fail_receipt in [true, false] {
            let mut executor = NoTransaction;
            let mut probe = Probe::new(&mut executor);
            probe.fail_receipt_write = fail_receipt;
            probe.fail_audit_write = !fail_receipt;
            let command = command(100);
            let context = receipt_command_context(&command, &actor()).unwrap();
            let error = persist_receipt_command_success(
                &probe,
                &command,
                &context,
                &committed_receipt(),
                &mut executor,
            )
            .await
            .unwrap_err();
            if fail_receipt {
                assert!(matches!(error, Error::ConflictError(message) if message == "原回执写入错误"));
                assert_eq!(*probe.calls.lock().unwrap(), ["save_receipt"]);
            } else {
                assert!(matches!(error, Error::Internal(message) if message == "原事件写入错误"));
                assert_eq!(*probe.calls.lock().unwrap(), ["save_receipt", "save_event"]);
            }
            assert!(probe.logs.lock().unwrap().is_empty());
        }
    }
}
