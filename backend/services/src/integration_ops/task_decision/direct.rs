use database::IntegrationOpsExt;
use entities::integration_ops::{
    DirectConclusion, IntegrationCommandIdentity, ReconciliationDifference,
    ReconciliationDifferenceResolution,
};
use erp_workflow::WorkItemExt;
use mongodb::Database;
use persistence_core::Executor;
use serde::{Deserialize, Serialize};

use super::super::evidence::{
    ensure_direct_reason, verified_reference, verify_evidence_refs, EvidenceSubject,
};
use super::super::{
    DirectReconciliationCommand, DirectReconciliationConclusion, DirectReconciliationDecision,
    DirectReconciliationResult, DirectReconciliationStatus, IntegrationActionOutcome, IntegrationItemType,
    IntegrationNonTerminalTaskAction, IntegrationOpsService, PreparedDirectDecisionTarget,
};
use super::action::difference_action_fact;
use super::guard::{command_identity, ensure_difference_open, latest_resolution, load_difference};
use super::{append_resolution, store_receipt, DirectFact, DIRECT_DECISION_AUDIT};
use crate::errors::{Error, Result};
use application_core::AuditActor;

#[derive(Debug, Serialize, Deserialize)]
struct DirectReceiptMessage {
    #[serde(rename = "s")]
    resulting_status: DirectReconciliationStatus,
    #[serde(rename = "t")]
    is_terminal: bool,
    #[serde(rename = "o")]
    outcome: IntegrationActionOutcome,
    #[serde(rename = "b", skip_serializing_if = "Option::is_none")]
    business_result_reference: Option<String>,
}

impl IntegrationOpsService {
    /// 对未关联任何正式任务的差异提交 decision-only 命令。
    ///
    /// # 错误
    /// 路径身份、差异版本、任务关联、终态证据或幂等校验失败时返回错误。
    pub async fn decide_difference(
        &self,
        path_id: &str,
        command: DirectReconciliationCommand,
        actor: &AuditActor,
    ) -> Result<DirectReconciliationResult> {
        command.validate()?;
        if command.difference_id != path_id {
            return Err(Error::ValidationError("路径差异 ID 与命令不一致".to_string()));
        }
        let receipt = command_identity(
            actor.id(),
            DIRECT_DECISION_AUDIT,
            "reconciliation_difference",
            path_id,
            &command.idempotency_key,
            &command,
        )?;
        if let Some(result) = self.replay_direct_decision(&receipt, &command, actor).await? {
            return Ok(result);
        }
        let result = self
            .transact_direct_decision(command.clone(), actor.clone(), receipt.clone())
            .await;
        self.recover_direct_decision(result, &receipt, &command, actor)
            .await
    }

    async fn transact_direct_decision(
        &self,
        command: DirectReconciliationCommand,
        actor: AuditActor,
        receipt: IntegrationCommandIdentity,
    ) -> Result<DirectReconciliationResult> {
        self.run_audited(move |db, session| {
            Box::pin(async move {
                let prepared = PreparedDirectDecisionTarget::try_from(&command)?;
                ensure_no_work_item(db, &prepared.difference_id, session).await?;
                let difference = load_difference(db, &prepared.difference_id, session).await?;
                let latest = latest_resolution(db, &prepared.difference_id, session).await?;
                ensure_direct_version(prepared.difference_version, latest.as_ref())?;
                ensure_difference_open(latest.as_ref())?;
                let fact = direct_decision_fact(
                    db,
                    &difference,
                    &command,
                    receipt.receipt_id(),
                    actor.id(),
                    session,
                )
                .await?;
                let record = append_resolution(
                    &difference,
                    latest.as_ref(),
                    &fact,
                    receipt.receipt_id(),
                    actor.id(),
                )?;
                db.reconciliation_difference_resolutions()
                    .create(&record, session)
                    .await?;
                store_direct_receipt(db, &actor, &receipt, &fact, session).await?;
                Ok(direct_result(&command, receipt.receipt_id(), fact))
            })
        })
        .await
    }

    async fn replay_direct_decision(
        &self,
        receipt: &IntegrationCommandIdentity,
        command: &DirectReconciliationCommand,
        actor: &AuditActor,
    ) -> Result<Option<DirectReconciliationResult>> {
        let Some(message) = self
            .replay_receipt::<DirectReceiptMessage>(receipt, actor)
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(DirectReconciliationResult {
            difference_id: command.difference_id.clone(),
            operation_id: command.operation_id.clone(),
            resolution_record_id: receipt.receipt_id().to_string(),
            resulting_status: message.resulting_status,
            is_terminal: message.is_terminal,
            outcome: message.outcome,
            business_result_reference: message.business_result_reference,
        }))
    }

    async fn recover_direct_decision(
        &self,
        result: Result<DirectReconciliationResult>,
        receipt: &IntegrationCommandIdentity,
        command: &DirectReconciliationCommand,
        actor: &AuditActor,
    ) -> Result<DirectReconciliationResult> {
        match result {
            Ok(result) => Ok(result),
            Err(error) => match self.replay_direct_decision(receipt, command, actor).await? {
                Some(result) => Ok(result),
                None => Err(error),
            },
        }
    }
}

async fn ensure_no_work_item(db: &Database, difference_id: &str, executor: &mut dyn Executor) -> Result<()> {
    let items = db
        .work_items()
        .find_unique_for_reconciliation_difference(difference_id, executor)
        .await?;
    if items.is_empty() {
        return Ok(());
    }
    Err(Error::ConflictError(
        "差异已关联正式任务，必须通过 W29 任务强命令处理".to_string(),
    ))
}

/// 校验直接对账差异版本与当前决定序号一致（typed 比较，无二次解析）。
///
/// 非十进制输入已由 Prepared 目标在 DTO 层拒绝；此处只区分当前与陈旧。
fn ensure_direct_version(expected: u64, latest: Option<&ReconciliationDifferenceResolution>) -> Result<()> {
    if ReconciliationDifferenceResolution::current_version(latest) == expected {
        Ok(())
    } else {
        Err(Error::ConflictError(
            "差异决定版本已变化，请刷新后重试".to_string(),
        ))
    }
}

async fn direct_decision_fact(
    db: &Database,
    difference: &ReconciliationDifference,
    command: &DirectReconciliationCommand,
    receipt_id: &str,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<DirectFact> {
    match &command.decision {
        DirectReconciliationDecision::NonTerminalAction {
            action,
            evidence_refs,
            comment: _,
        } => {
            difference_action_fact(
                db,
                difference,
                &IntegrationNonTerminalTaskAction {
                    item_type: IntegrationItemType::ReconciliationDifference,
                    item_id: command.difference_id.clone(),
                    kind: *action,
                    operation_id: command.operation_id.clone(),
                    reason_code: None,
                    comment: None,
                    evidence_refs: evidence_refs.clone(),
                },
                receipt_id,
                actor_id,
                executor,
            )
            .await
        }
        DirectReconciliationDecision::TerminalConclusion {
            conclusion,
            reason_code,
            reason_registry_id,
            reason_registry_version,
            registered_reason_id,
            evidence_refs,
            comment: _,
        } => {
            ensure_direct_reason(
                reason_registry_id,
                *reason_registry_version,
                registered_reason_id,
                *reason_code,
                *conclusion,
                evidence_refs,
            )?;
            let subject = EvidenceSubject::difference(difference);
            let verified = verify_evidence_refs(db, &subject, evidence_refs, actor_id, executor).await?;
            let reference = verified_reference(&verified)?;
            let typed = DirectConclusion::from(*conclusion);
            let (resulting_status, outcome) = match conclusion {
                DirectReconciliationConclusion::ConfirmNoError => (
                    DirectReconciliationStatus::ConfirmedNoError,
                    IntegrationActionOutcome::ConfirmedNoError,
                ),
                DirectReconciliationConclusion::ConfirmValidDifference => (
                    DirectReconciliationStatus::ConfirmedValidDifference,
                    IntegrationActionOutcome::ConfirmedValidDifference,
                ),
            };
            Ok(DirectFact {
                action: typed.resolution_action(),
                evidence_reference: Some(reference.clone()),
                resulting_status,
                outcome,
                business_result_reference: Some(reference),
                verified_evidence: verified.into_iter().map(|evidence| evidence.reference).collect(),
            })
        }
    }
}

fn direct_result(
    command: &DirectReconciliationCommand,
    receipt_id: &str,
    fact: DirectFact,
) -> DirectReconciliationResult {
    let is_terminal = matches!(
        fact.resulting_status,
        DirectReconciliationStatus::ConfirmedNoError | DirectReconciliationStatus::ConfirmedValidDifference
    );
    DirectReconciliationResult {
        difference_id: command.difference_id.clone(),
        operation_id: command.operation_id.clone(),
        resolution_record_id: receipt_id.to_string(),
        resulting_status: fact.resulting_status,
        is_terminal,
        outcome: fact.outcome,
        business_result_reference: fact.business_result_reference,
    }
}

async fn store_direct_receipt(
    db: &Database,
    actor: &AuditActor,
    receipt: &IntegrationCommandIdentity,
    fact: &DirectFact,
    executor: &mut dyn Executor,
) -> Result<()> {
    store_receipt(
        db,
        actor,
        receipt,
        DirectReceiptMessage {
            resulting_status: fact.resulting_status,
            is_terminal: fact.resulting_status == DirectReconciliationStatus::ConfirmedNoError
                || fact.resulting_status == DirectReconciliationStatus::ConfirmedValidDifference,
            outcome: fact.outcome,
            business_result_reference: fact.business_result_reference.clone(),
        },
        executor,
    )
    .await
}
