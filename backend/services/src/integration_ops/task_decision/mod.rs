//! W29 强类型任务动作、任务完成与无任务直接对账决定。

mod action;
mod complete;
mod direct;
mod guard;

#[cfg(test)]
mod tests;

use database::AccessControlExt;
use entities::integration_ops::{
    IntegrationCommandIdentity, ReconciliationDifference, ReconciliationDifferenceId,
    ReconciliationDifferenceResolution, ReconciliationDifferenceResolutionId, ResolutionAction,
};
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;
use serde::{Deserialize, Serialize};

use super::{ControlledEvidenceRef, DirectReconciliationStatus, IntegrationActionOutcome};
use crate::audit::AuditActorLogs;
use crate::errors::{Error, Result};
use application_core::AuditActor;

#[cfg(test)]
use self::action::next_allowed_actions;
#[cfg(test)]
use self::guard::command_identity;

const TASK_ACTION_AUDIT: &str = "integration.task_action";
const TASK_COMPLETION_AUDIT: &str = "integration.task_completion";
const DIRECT_DECISION_AUDIT: &str = "integration.direct_reconciliation";

#[derive(Debug, Clone)]
struct DirectFact {
    action: ResolutionAction,
    evidence_reference: Option<String>,
    resulting_status: DirectReconciliationStatus,
    outcome: IntegrationActionOutcome,
    business_result_reference: Option<String>,
    verified_evidence: Vec<ControlledEvidenceRef>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReceiptEnvelope<T> {
    #[serde(rename = "f")]
    fingerprint: String,
    #[serde(rename = "r")]
    result: T,
}

/// 经领域追加工厂形成决定记录（序号递增与状态派生归领域，时间与身份由调用方注入）。
///
/// # 参数
/// * `difference` - 所属对账差异
/// * `latest` - 当前最新决定；`None` 表示首条追加
/// * `fact` - 已验证的决定事实（动作与证据引用）
/// * `record_id` - 记录主键（调用方收据 ID）
/// * `actor_id` - 决定人
///
/// # 返回
/// 返回新建的不可变追加式决定记录。
///
/// # 错误
/// 序号已达上限时返回 `ConflictError`，其余领域约束失败保持领域逻辑错误。
///
/// # 约束
/// 序号上限文案由领域拥有，此处只做错误类别映射，不维护第二份上限规则。
fn append_resolution(
    difference: &ReconciliationDifference,
    latest: Option<&ReconciliationDifferenceResolution>,
    fact: &DirectFact,
    record_id: &str,
    actor_id: &str,
) -> Result<ReconciliationDifferenceResolution> {
    ReconciliationDifferenceResolution::append(
        ReconciliationDifferenceResolutionId::new(record_id.to_string()),
        ReconciliationDifferenceId::new(difference.base.id.clone()),
        latest,
        fact.action,
        fact.evidence_reference.clone(),
        actor_id.to_string(),
        Instant::now(),
    )
    .map_err(|error| {
        let message = error.to_string();
        if message == "差异决定序号已达上限" {
            Error::ConflictError(message)
        } else {
            Error::Logic(error)
        }
    })
}

async fn store_receipt<T: Serialize>(
    db: &Database,
    actor: &AuditActor,
    receipt: &IntegrationCommandIdentity,
    result: T,
    executor: &mut dyn Executor,
) -> Result<()> {
    let message = serde_json::to_string(&ReceiptEnvelope {
        fingerprint: receipt.fingerprint().to_string(),
        result,
    })
    .map_err(|_| Error::Internal("W29 结果无法形成幂等收据".to_string()))?;
    let audit = actor.clone().resource_log_with_id(
        receipt.receipt_id().to_string(),
        receipt.action(),
        receipt.resource_type(),
        receipt.resource_id().to_string(),
        Some(message),
    )?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(())
}
