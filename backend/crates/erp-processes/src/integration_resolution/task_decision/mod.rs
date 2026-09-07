//! W29 强类型任务动作、任务完成与无任务直接对账决定。

mod action;
mod complete;
mod direct;
mod execution;
mod guard;
#[cfg(test)]
mod tests;

#[cfg(test)]
use self::guard::command_identity;
use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_integration::entity::integration_ops::IntegrationCommandIdentity;
#[cfg(test)]
use erp_integration::service::task_decision::action::next_allowed_actions;
use mongodb::Database;
use persistence_core::Executor;
use serde::{Deserialize, Serialize};
use services::{Error, Result};

const TASK_ACTION_AUDIT: &str = "integration.task_action";
const TASK_COMPLETION_AUDIT: &str = "integration.task_completion";
const DIRECT_DECISION_AUDIT: &str = "integration.direct_reconciliation";

#[derive(Debug, Serialize, Deserialize)]
struct ReceiptEnvelope<T> {
    #[serde(rename = "f")]
    fingerprint: String,
    #[serde(rename = "r")]
    result: T,
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
