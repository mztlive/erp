//! 冲正提交先恢复精确命令收据，未知结果仅 fresh 回读，不重复写入。

use application_core::{AuditActor, CommandReceipt};
use erp_audit::AuditExt;
use erp_audit::entity::AuditLog;
use erp_audit::repository::AuditLogRepositoryExt;
use erp_core::common::time::Instant;
use erp_identity::SharedRbacService;
use erp_read_models::Error as ReadModelError;
use erp_workflow::DocumentRegistryExt;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::repository::BusinessDocumentRepositoryExt;
use erp_workflow::service::approval::execution::apply_plan::PlannedWrites;
use erp_workflow::service::approval::execution::command_recovery_delay;
use mongodb::Database;
use mongodb::error::Error as MongoError;
use persistence_core::{Error as PersistenceError, Executor, Transactional};

use super::super::ReturnsProcess;
use super::prepare::ensure_return_start_replay_authorized;
use crate::{Error, Result};

/// 两类冲正提交回读使用的完整命令身份和授权上下文。
#[derive(Clone)]
pub(in crate::reverse_flow) struct ReversalSubmitReplayInput {
    pub id: String,
    pub document_type: DocumentType,
    pub submit_permission: &'static str,
    pub command_receipt: CommandReceipt,
    pub actor: AuditActor,
}

impl ReturnsProcess {
    /// 在状态及版本检查前，用 fresh 事务恢复同载荷提交结果。
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

    /// 结果未知或收据竞争后只回读已提交命令，不自动执行第二次写事务。
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
    rbac: &SharedRbacService,
    input: &ReversalSubmitReplayInput,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    ensure_return_start_replay_authorized(
        db,
        rbac,
        &input.actor,
        input.document_type,
        input.submit_permission,
        &input.id,
        executor,
    )
    .await?;
    committed_reversal_submit(db, &input.id, &input.command_receipt, executor).await
}

/// 在已有执行器内匹配精确请求收据，BPM 收据不能替代原请求版本指纹。
pub(super) async fn committed_reversal_submit(
    db: &Database,
    id: &str,
    command_receipt: &CommandReceipt,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let candidates = command_receipt.id_candidates();
    let facts = db.audit_logs().find_command_receipts_by_ids(&candidates, executor).await?;
    let replayed = AuditLog::pick_committed_resource_id(command_receipt, &candidates, &facts)?;
    if replayed.as_deref().is_some_and(|resource_id| resource_id != id) {
        return Err(Error::ConflictError("冲正提交收据与原单不一致".into()));
    }
    Ok(replayed)
}

/// 沿原单注册事实原子启动审批，复用两类冲正完全一致的守卫。
pub(super) async fn mark_reversal_started(
    db: &Database,
    kind: DocumentType,
    writes: &PlannedWrites,
    now: Instant,
    executor: &mut dyn Executor,
) -> Result<()> {
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
pub(in crate::reverse_flow) fn reversal_result_read_error(error: ReadModelError) -> Error {
    Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(MongoError::custom(format!(
        "冲正已提交但结果读取失败: {error}"
    ))))
}
