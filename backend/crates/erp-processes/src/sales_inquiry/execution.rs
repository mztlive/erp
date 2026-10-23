//! 一个命令一个事务；重放不产生新的任务、审计或业务事实。

use application_core::{AuditActor, CommandReceipt};
use erp_audit::AuditActorLogs;
use erp_sales::entity::sales_inquiry::{InquiryCommandReceipt, InquiryCommandResult, SalesInquiry};
use erp_sales::service::sales_inquiry::SalesInquiryService;
use persistence_core::{Executor, NoTransaction};
use serde::Serialize;

use super::SalesInquiryProcess;
use super::dto::*;
use super::fee_allocation::AllocateInquiryFeesCommand;
use super::internal_quote::{SaveInquiryFeeBasisCommand, SaveInquiryQuoteCommand};
use super::quote_import::ImportInquiryQuotesCommand;
use crate::audit::persist_log;
use crate::{Error, Result};

mod apply;
mod create;

#[derive(Clone, Serialize)]
#[serde(tag = "action", content = "request", rename_all = "snake_case")]
pub(super) enum InquiryCommand {
    Create(CreateInquiryCommand),
    Save(SaveInquiryCommand),
    Submit(SubmitInquiryCommand),
    Supplement(InquiryTaskReasonCommand),
    SaveFeedback(SaveInquiryFeedbackCommand),
    Publish(PublishInquiryCommand),
    Renew(InquiryTaskReasonCommand),
    Accept(AcceptInquiryCommand),
    Reinquire(ReinquireInquiryCommand),
    Close(InquiryReasonCommand),
    Reassign(ReassignInquiryCommand),
    AllocateFees(AllocateInquiryFeesCommand),
    InternalQuote(Box<SaveInquiryQuoteCommand>),
    ImportQuotes(Box<ImportInquiryQuotesCommand>),
    FeeBasis(SaveInquiryFeeBasisCommand),
}

impl InquiryCommand {
    fn action(&self) -> &'static str {
        match self {
            Self::Create(_) => "create",
            Self::Save(_) => "save",
            Self::Submit(_) => "submit",
            Self::Supplement(_) => "supplement",
            Self::SaveFeedback(_) => "save_feedback",
            Self::Publish(_) => "publish",
            Self::Renew(_) => "renew",
            Self::Accept(_) => "accept",
            Self::Reinquire(_) => "reinquire",
            Self::Close(_) => "close",
            Self::Reassign(_) => "reassign",
            Self::AllocateFees(_) => "allocate_fees",
            Self::InternalQuote(_) => "internal_quote",
            Self::ImportQuotes(_) => "import_quotes",
            Self::FeeBasis(_) => "fee_basis",
        }
    }
    fn key(&self) -> &str {
        match self {
            Self::Create(r) => &r.command_key,
            Self::Save(r) => &r.command_key,
            Self::Submit(r) => &r.command_key,
            Self::Publish(r) => &r.command_key,
            Self::Renew(r) => &r.command_key,
            Self::Supplement(r) => &r.command_key,
            Self::SaveFeedback(r) => &r.command_key,
            Self::Accept(r) => &r.command_key,
            Self::Reinquire(r) => &r.command_key,
            Self::Close(r) => &r.command_key,
            Self::Reassign(r) => &r.command_key,
            Self::AllocateFees(r) => &r.command_key,
            Self::InternalQuote(r) => &r.command_key,
            Self::ImportQuotes(r) => &r.command_key,
            Self::FeeBasis(r) => &r.command_key,
        }
    }
    fn task(&self) -> Option<(&str, u64)> {
        match self {
            Self::Publish(r) => Some((&r.task_id, r.task_version)),
            Self::Renew(r) => Some((&r.task_id, r.task_version)),
            Self::Supplement(r) => Some((&r.task_id, r.task_version)),
            Self::SaveFeedback(r) => Some((&r.task_id, r.task_version)),
            Self::Reassign(r) => Some((&r.task_id, r.task_version)),
            Self::InternalQuote(r) => Some((&r.task_id, r.task_version)),
            Self::ImportQuotes(r) => Some((&r.task_id, r.task_version)),
            Self::FeeBasis(r) => Some((&r.task_id, r.task_version)),
            _ => None,
        }
    }
    fn sales_action(&self) -> &'static str {
        if matches!(self, Self::Save(_) | Self::AllocateFees(_)) { "update" } else { self.action() }
    }
    fn procurement_action(&self) -> &'static str {
        if matches!(self, Self::Renew(_)) {
            "publish"
        } else if matches!(self, Self::FeeBasis(_) | Self::ImportQuotes(_)) {
            "internal_quote"
        } else {
            self.action()
        }
    }
}

impl SalesInquiryProcess {
    pub(super) async fn execute(
        &self,
        actor: &AuditActor,
        id: &str,
        command: InquiryCommand,
    ) -> Result<InquiryMutationView> {
        let receipt = CommandReceipt::from_payload(
            "sales-inquiry-command:",
            actor.id(),
            command.action(),
            "sales_inquiry",
            command.key(),
            &(id, &command),
        )?;
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let revision = self.rbac.current_policy_revision().await?;
        let result = self
            .rbac
            .run_authorized_policy_transaction(revision, {
                let actor = actor.clone();
                let id = id.clone();
                let command = command.clone();
                let receipt = receipt.clone();
                move |executor| {
                    Box::pin(
                        async move { this.execute_command(&actor, &id, command, receipt, executor).await },
                    )
                }
            })
            .await;
        match result {
            Err(error) if error.command_may_have_committed() => {
                match SalesInquiryService::new(self.db.clone())
                    .receipt(receipt.id(), &mut NoTransaction)
                    .await
                {
                    Ok(Some(prior)) => {
                        self.authorize_replay(&actor, &id, &command, &mut NoTransaction).await?;
                        mutation_from_result(prior.matches(
                            actor.id(),
                            command.key(),
                            command.action(),
                            receipt.fingerprint().digest_hex(),
                        )?)
                    },
                    Ok(None) | Err(_) => Err(error),
                }
            },
            other => other,
        }
    }

    async fn execute_command(
        &self,
        actor: &AuditActor,
        id: &str,
        command: InquiryCommand,
        receipt: CommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<InquiryMutationView> {
        let service = SalesInquiryService::new(self.db.clone());
        if let Some(prior) = service.receipt(receipt.id(), executor).await? {
            self.authorize_replay(actor, id, &command, executor).await?;
            return mutation_from_result(prior.matches(
                actor.id(),
                command.key(),
                command.action(),
                receipt.fingerprint().digest_hex(),
            )?);
        }
        let (mut inquiry, tasks) = self.apply_command(actor, id, &command, executor).await?;
        if !matches!(command, InquiryCommand::Create(_)) {
            service.save(&mut inquiry, executor).await?;
        }
        let result = mutation_view(&inquiry, tasks);
        self.persist_command(actor, &command, &receipt, &result, executor).await?;
        Ok(result)
    }

    async fn persist_command(
        &self,
        actor: &AuditActor,
        command: &InquiryCommand,
        receipt: &CommandReceipt,
        result: &InquiryMutationView,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let audit = actor.clone().resource_log(
            &format!("sales_inquiry.{}", command.action()),
            "sales_inquiry",
            result.inquiry_id.clone(),
        )?;
        persist_log(&self.db, &audit, executor).await?;
        let record = InquiryCommandReceipt::new(
            receipt.id().to_string(),
            actor.id(),
            command.key(),
            command.action(),
            receipt.fingerprint().digest_hex().to_string(),
            mutation_result(result),
        )?;
        SalesInquiryService::new(self.db.clone()).save_receipt(&record, executor).await?;
        Ok(())
    }

    async fn authorize_replay(
        &self,
        actor: &AuditActor,
        id: &str,
        command: &InquiryCommand,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if command.task().is_some() {
            self.access()
                .ensure_procurement_permission(actor, command.procurement_action(), executor)
                .await?;
        } else if matches!(command, InquiryCommand::Create(_)) {
            self.access().resolve_sales(actor, "create", executor).await?;
        } else {
            self.access().require_sales(actor, command.sales_action(), id, executor).await?;
        }
        Ok(())
    }
}

fn mutation_view(inquiry: &SalesInquiry, task_ids: Vec<String>) -> InquiryMutationView {
    InquiryMutationView {
        inquiry_id: inquiry.base.id.clone(),
        version: inquiry.base.version,
        requirement_version: inquiry.requirement_version,
        feedback_round: inquiry.feedback_round,
        status: inquiry.status,
        task_ids,
    }
}

fn mutation_result(view: &InquiryMutationView) -> InquiryCommandResult {
    InquiryCommandResult::Mutation {
        inquiry_id: view.inquiry_id.clone(),
        version: view.version,
        requirement_version: view.requirement_version,
        feedback_round: view.feedback_round,
        status: view.status,
        task_ids: view.task_ids.clone(),
    }
}

fn mutation_from_result(result: &InquiryCommandResult) -> Result<InquiryMutationView> {
    match result {
        InquiryCommandResult::Mutation {
            inquiry_id,
            version,
            requirement_version,
            feedback_round,
            status,
            task_ids,
        } => Ok(InquiryMutationView {
            inquiry_id: inquiry_id.clone(),
            version: *version,
            requirement_version: *requirement_version,
            feedback_round: *feedback_round,
            status: *status,
            task_ids: task_ids.clone(),
        }),
        InquiryCommandResult::Created { .. } => Err(Error::Internal("询价命令回执缺少冻结状态".into())),
    }
}

#[cfg(test)]
mod tests {
    use erp_sales::Error as SalesError;
    use erp_sales::entity::sales_inquiry::InquiryStatus;

    use super::*;

    #[test]
    fn command_identity_replays_exact_payload_and_rejects_cross_inquiry_key_reuse() {
        let command = InquiryCommand::Publish(PublishInquiryCommand {
            command_key: "same-key".into(),
            expected_version: 2,
            task_id: "task".into(),
            task_version: 1,
        });
        let make = |id: &str| {
            CommandReceipt::from_payload(
                "sales-inquiry-command:",
                "buyer",
                command.action(),
                "sales_inquiry",
                command.key(),
                &(id, &command),
            )
            .unwrap()
        };
        let receipt = make("a");
        let result = InquiryMutationView {
            inquiry_id: "a".into(),
            version: 3,
            requirement_version: 1,
            feedback_round: 0,
            status: InquiryStatus::ProcurementProcessing,
            task_ids: vec!["task".into()],
        };
        let record = InquiryCommandReceipt::new(
            receipt.id().into(),
            "buyer",
            "same-key",
            "publish",
            receipt.fingerprint().digest_hex().into(),
            mutation_result(&result),
        )
        .unwrap();
        let replay = make("a");
        assert_eq!(receipt.id(), replay.id());
        assert_eq!(
            mutation_from_result(
                record.matches("buyer", "same-key", "publish", replay.fingerprint().digest_hex()).unwrap()
            )
            .unwrap(),
            result
        );
        let other = make("b");
        assert_eq!(receipt.id(), other.id());
        assert!(matches!(
            record.matches("buyer", "same-key", "publish", other.fingerprint().digest_hex()),
            Err(SalesError::ConflictError(_))
        ));
        let serialized = serde_json::to_string(&record).unwrap();
        assert!(!serialized.contains("same-key"));
    }
}
