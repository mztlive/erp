//! 退款与冲正提交先恢复精确命令收据，未知结果仅 fresh 回读，不重复写入。

use application_core::{AuditActor, CommandReceipt};
use erp_audit::AuditLog;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_returns::ReturnsCommandReceiptService;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::repository::BusinessDocumentRepositoryExt;
use erp_workflow::service::approval::execution::apply_plan::PlannedWrites;
use erp_workflow::service::approval::execution::{command_recovery_delay, map_receipt_first_write_error};
use erp_workflow::{BpmExt, DocumentRegistryExt};
use mongodb::Database;
use mongodb::error::Error as MongoError;
use persistence_core::{Error as PersistenceError, Executor, Transactional};

use super::super::ReturnsProcess;
use super::super::customer_refund::authorized_customer_refund;
use super::super::supplier_refund::authorized_supplier_refund;
use super::ensure_return_start_replay_authorized;
use crate::{Error, Result, ReverseFlowPorts};

/// 退款及冲正提交回读使用的完整命令身份和授权上下文。
#[derive(Clone)]
pub(in crate::reverse_flow) struct ReversalSubmitReplayInput {
    pub id: String,
    pub document_type: DocumentType,
    pub command_receipt: CommandReceipt,
    pub actor: AuditActor,
}

impl ReturnsProcess {
    /// 在状态及版本检查前，用 fresh 事务恢复同载荷提交结果。
    ///
    /// # 参数
    /// * `input` - 冲正提交的命令身份与授权上下文。
    ///
    /// # 返回
    /// 命中同载荷已提交收据时返回资源 ID；无收据时返回 `None`。
    ///
    /// # 错误
    /// 授权失败、收据资源与原单不一致或仓储读取失败时返回对应错误。
    pub(in crate::reverse_flow) async fn replay_reversal_submit(
        &self,
        input: &ReversalSubmitReplayInput,
    ) -> Result<Option<String>> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let input = input.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { replay_submit_apply(&db, &rbac, &input, executor).await })
            })
            .await
    }

    /// 在原提交事务先重验完整资格和真实来源，再匹配精确请求收据。
    /// # 参数
    /// input 为原命令身份与操作人，executor 为本次提交事务。
    /// # 返回
    /// 已提交同载荷命令返回真实结果 ID；未提交返回 None。
    /// # 错误
    /// 资格、来源、收据不符或数据库故障时拒绝。
    pub(in crate::reverse_flow) async fn replay_reversal_submit_stage(
        &self,
        input: &ReversalSubmitReplayInput,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        replay_submit_apply(&self.db, &self.rbac, input, executor).await
    }

    /// 结果未知或收据竞争后只回读已提交命令，不自动执行第二次写事务。
    ///
    /// # 参数
    /// * `input` - 冲正提交的命令身份与授权上下文。
    /// * `result` - 写事务的结果。
    ///
    /// # 返回
    /// 原结果成功，或有界回读确认已提交时，无返回值。
    ///
    /// # 错误
    /// 命令不可能已提交时返回原错误。回读仍无法确认时返回原错误。
    /// 原错误为 `OutcomeUnknown` 且回读得到确定性失败时仍返回原错误；否则返回该回读错误。
    pub(in crate::reverse_flow) async fn finish_reversal_submit(
        &self,
        input: &ReversalSubmitReplayInput,
        result: Result<()>,
    ) -> Result<()> {
        let Err(original_error) = result else {
            return Ok(());
        };
        if !original_error.command_may_have_committed() {
            return Err(original_error);
        }
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            match self.replay_reversal_submit(input).await {
                Ok(Some(_)) => return Ok(()),
                Ok(None) => {},
                Err(error) if error.command_may_have_committed() => {},
                Err(_) if matches!(original_error, Error::OutcomeUnknown(_)) => return Err(original_error),
                Err(error) => return Err(error),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(original_error)
    }
}

/// 账号、静态权限、精确资金来源与原经办职责先于任何收据读取。
async fn replay_submit_apply(
    db: &Database,
    rbac: &ReverseFlowPorts,
    input: &ReversalSubmitReplayInput,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    match input.document_type {
        DocumentType::CustomerRefund => {
            authorized_customer_refund(db, rbac, &input.actor, &input.id, executor).await?;
        },
        DocumentType::SupplierRefund => {
            authorized_supplier_refund(db, rbac, &input.actor, &input.id, executor).await?;
        },
        _ => {
            ensure_return_start_replay_authorized(
                db,
                rbac,
                &input.actor,
                input.document_type,
                &input.id,
                executor,
            )
            .await?
        },
    }
    committed_reversal_submit(db, &input.id, &input.command_receipt, executor).await
}

/// 在已有执行器内匹配精确请求收据，BPM 收据不能替代原请求版本指纹。
///
/// # 参数
/// * `db` - 数据库。
/// * `id` - 冲正单主键。
/// * `command_receipt` - 原请求命令收据。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 收据命中且资源 ID 与 `id` 一致时返回该 ID；无收据时返回 `None`。
///
/// # 错误
/// 收据资源 ID 与 `id` 不一致时返回冲突。收据读取失败时返回对应错误。
pub(super) async fn committed_reversal_submit(
    db: &Database,
    id: &str,
    command_receipt: &CommandReceipt,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let replayed = ReturnsCommandReceiptService::new(db.clone())
        .committed_resource_id(command_receipt, executor)
        .await?;
    if replayed.as_deref().is_some_and(|resource_id| resource_id != id) {
        return Err(Error::ConflictError("退款或冲正提交收据与原单不一致".into()));
    }
    Ok(replayed)
}

/// 先插入审批命令收据，再以注册单据守卫启动审批；任一步失败由原事务回滚。
pub(super) async fn persist_start_guard(
    db: &Database,
    kind: DocumentType,
    writes: &PlannedWrites,
    now: Instant,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.bpm_workflow()
        .insert_command_receipt(&writes.receipt, executor)
        .await
        .map_err(map_receipt_first_write_error)?;
    let guarded = db
        .business_documents()
        .mark_approval_started(
            writes.instance.subject.subject_id(),
            kind,
            &writes.instance.process_definition_id,
            writes.instance.definition_version,
            now,
            executor,
        )
        .await?;
    if guarded.is_none() {
        return Err(Error::ConflictError(format!("{}审批启动守卫冲突，请刷新后重试", kind.label())));
    }
    Ok(())
}

/// 启动已经提交后读取响应失败，必须保留同操作号核对的未知结果语义。
///
/// # 参数
/// * `error` - 已提交后的读模型失败。
///
/// # 返回
/// 返回 `OutcomeUnknown`。
///
/// # 错误
/// 不返回错误。
pub(in crate::reverse_flow) fn reversal_result_read_error(error: Error) -> Error {
    Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(MongoError::custom(format!(
        "退款或冲正已提交但结果读取失败: {error}"
    ))))
}

/// 在原事务内保存业务审计和精确请求提交收据。
/// # 参数
/// * `db` / `command_receipt` / `id` - 数据库、原请求身份及单据主键。
/// * `audit` / `executor` - 业务审计及同一事务执行器。
/// # 返回
/// 保存成功后无返回值。
/// # 错误
/// 收据目录无效、唯一冲突或持久化失败时返回错误。
pub(super) async fn persist_submit_receipt(
    db: &Database,
    command_receipt: &CommandReceipt,
    id: String,
    audit: &AuditLog,
    executor: &mut dyn Executor,
) -> Result<()> {
    persist_log(db, audit, executor).await?;
    ReturnsCommandReceiptService::new(db.clone())
        .save_resource(command_receipt, id, audit.base.id.clone(), executor)
        .await?;
    Ok(())
}
