use std::sync::Mutex;

use entity_core::BaseModel;
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_finance::entity::supplier_deposit::{
    DepositAccountIdentity, DepositCurrency, FundsEvidenceArchive, FundsEvidenceInput, FundsEvidenceResult,
    FundsTransactionDirection, SupplierDepositAccount,
};
use mongodb::error::Error as MongoError;
use persistence_core::{Error as PersistenceError, NoTransaction};

use super::*;

struct State {
    inbox: FundsEvidenceInbox,
    account: SupplierDepositAccount,
    has_debit: bool,
    settled: bool,
    task_updated: bool,
}

struct Steps {
    state: Mutex<State>,
    trace: Mutex<Vec<&'static str>>,
    fail: Option<&'static str>,
    unresolved: bool,
}

impl Steps {
    fn new(fail: Option<&'static str>) -> Self {
        let now = Instant::from_unix_secs(10);
        let input = FundsEvidenceInput {
            source_observed_at: now,
            source_revision: None,
            archive: FundsEvidenceArchive {
                reference: "source-evidence".into(),
                body_encrypted: "encrypted-source".into(),
                content_fingerprint: "source-digest".into(),
            },
            result: FundsEvidenceResult::Transaction {
                stable_transaction_id: "source-transaction".into(),
                external_order_id: Some("source-order".into()),
                direction: FundsTransactionDirection::Debit,
                amount: "100".parse().unwrap(),
                currency: DepositCurrency::CNY,
                occurred_at: now,
                evidence_reference: "source-evidence".into(),
                original_transaction_id: None,
                bank_reference: None,
                business_code: Some("ORDER_DEBIT".into()),
            },
        };
        let inbox = FundsEvidenceInbox {
            base: BaseModel { id: "evidence".into(), ..BaseModel::fake() },
            account_id: "account".into(),
            source_key: input.key().unwrap(),
            semantic_fingerprint: "semantic-digest".into(),
            input,
            state: FundsEvidenceInboxState::Received,
            first_observed_at: now,
            last_observed_at: now,
            applied_resource: None,
            review_reason_code: None,
            review_resolution: None,
        };
        let mut account = SupplierDepositAccount::new(
            "account".into(),
            DepositAccountIdentity {
                payer_party_id: "payer".into(),
                supplier_id: "supplier".into(),
                external_account_ref: "external-account".into(),
                currency: DepositCurrency::CNY,
            },
        )
        .unwrap();
        account.credit("1000".parse().unwrap()).unwrap();
        account.reserve("100".parse().unwrap()).unwrap();
        Self {
            state: Mutex::new(State {
                inbox,
                account,
                has_debit: false,
                settled: false,
                task_updated: false,
            }),
            trace: Mutex::new(vec![]),
            fail,
            unresolved: false,
        }
    }

    fn step(&self, name: &'static str) -> Result<()> {
        self.trace.lock().unwrap().push(name);
        if self.fail == Some(name) {
            return Err(Error::BusinessLogicError("原批准或付款任务需要核对".into()));
        }
        Ok(())
    }

    async fn apply(&self, mode: ApplicationMode) -> Result<FundsInvestigationRun> {
        let inbox = self.state.lock().unwrap().inbox.clone();
        apply_evidence(self, &inbox, mode, &mut NoTransaction).await
    }
}

#[async_trait]
impl FundsApplicationSteps for Steps {
    async fn cash(&self, _: &FundsEvidenceInbox, _: &mut dyn Executor) -> Result<CashApplication> {
        self.step("cash")?;
        if self.unresolved {
            return Ok(CashApplication::Review("UNMAPPED_EXTERNAL_ORDER"));
        }
        let mut state = self.state.lock().unwrap();
        state.account.debit("100".parse().unwrap(), "100".parse().unwrap())?;
        state.has_debit = true;
        Ok(CashApplication::Applied(FundsAppliedResource::DebitUsage { usage_id: "usage".into() }))
    }

    async fn settle(&self, usage: &str, _: &mut dyn Executor) -> Result<()> {
        assert_eq!(usage, "usage");
        self.step("settle")?;
        self.state.lock().unwrap().settled = true;
        Ok(())
    }

    async fn payment_task(&self, usage: &str, _: &mut dyn Executor) -> Result<()> {
        assert_eq!(usage, "usage");
        self.step("task")?;
        self.state.lock().unwrap().task_updated = true;
        Ok(())
    }

    async fn complete(
        &self,
        inbox: &str,
        resource: FundsAppliedResource,
        _: &mut dyn Executor,
    ) -> Result<()> {
        assert_eq!(inbox, "evidence");
        self.step("complete")?;
        self.state.lock().unwrap().inbox.apply(resource)?;
        Ok(())
    }

    async fn review(&self, inbox: &str, reason: &str, _: &mut dyn Executor) -> Result<()> {
        assert_eq!(inbox, "evidence");
        self.step("review")?;
        let mut state = self.state.lock().unwrap();
        state.inbox.needs_review(reason)?;
        state.account.reconciliation_required = true;
        Ok(())
    }
}

#[tokio::test]
async fn matched_debit_settles_and_updates_task_before_completion_and_terminal_replay_is_zero_write() {
    let steps = Steps::new(None);
    assert_eq!(steps.apply(ApplicationMode::Complete).await.unwrap().applied, 1);
    assert_eq!(*steps.trace.lock().unwrap(), ["cash", "settle", "task", "complete"]);
    {
        let state = steps.state.lock().unwrap();
        assert_eq!(state.account.book_balance, "900".parse::<Amount>().unwrap());
        assert_eq!(state.account.reserved_amount, Amount::zero());
        assert!(state.settled && state.task_updated);
        assert!(state.inbox.applied_resource.is_some());
        assert!(!state.account.reconciliation_required);
    }
    assert_eq!(steps.apply(ApplicationMode::Complete).await.unwrap(), FundsInvestigationRun::default());
    assert_eq!(steps.trace.lock().unwrap().len(), 4);
}

#[tokio::test]
async fn every_failed_step_stops_before_completion_and_keeps_evidence_unapplied() {
    for (failure, expected) in [
        ("cash", vec!["cash"]),
        ("settle", vec!["cash", "settle"]),
        ("task", vec!["cash", "settle", "task"]),
        ("complete", vec!["cash", "settle", "task", "complete"]),
    ] {
        let steps = Steps::new(Some(failure));
        assert!(steps.apply(ApplicationMode::Complete).await.is_err());
        assert_eq!(*steps.trace.lock().unwrap(), expected);
        let state = steps.state.lock().unwrap();
        assert_eq!(state.inbox.state, FundsEvidenceInboxState::Received);
        assert!(state.inbox.applied_resource.is_none());
    }
}

#[tokio::test]
async fn failed_normal_transaction_can_preserve_cash_without_settlement_or_false_completion() {
    // 此步骤模式仅在上次正常事务已明确回滚后调用；不模拟或证明Mongo事务回滚。
    let steps = Steps::new(None);
    let report = steps.apply(ApplicationMode::PreserveDebit).await.unwrap();
    assert_eq!(report.needs_review, 1);
    assert_eq!(report.settlement_pending, 1);
    assert_eq!(report.applied, 0);
    assert_eq!(*steps.trace.lock().unwrap(), ["cash", "review"]);
    {
        let state = steps.state.lock().unwrap();
        assert_eq!(state.account.book_balance, "900".parse::<Amount>().unwrap());
        assert_eq!(state.account.reserved_amount, Amount::zero());
        assert!(state.has_debit && !state.settled && !state.task_updated);
        assert!(state.account.reconciliation_required);
        assert_eq!(state.inbox.state, FundsEvidenceInboxState::NeedsReview);
    }
    assert_eq!(steps.apply(ApplicationMode::PreserveDebit).await.unwrap(), FundsInvestigationRun::default());
    assert_eq!(steps.trace.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn unmapped_cash_only_pauses_account_without_manufacturing_application() {
    let mut unknown = Steps::new(None);
    unknown.unresolved = true;
    assert_eq!(unknown.apply(ApplicationMode::Complete).await.unwrap().needs_review, 1);
    assert_eq!(*unknown.trace.lock().unwrap(), ["cash", "review"]);
    assert_eq!(unknown.state.lock().unwrap().account.book_balance, "1000".parse::<Amount>().unwrap());
}

#[test]
fn unknown_commit_storage_and_transient_failures_never_enable_cash_fallback() {
    for error in [
        Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(MongoError::custom("unknown"))),
        Error::RepositoryError(PersistenceError::DatabaseError(MongoError::custom("storage"))),
        Error::TransientTransaction(PersistenceError::OptimisticLockingError),
        Error::ReceiptDuplicate(PersistenceError::DuplicateKey(MongoError::custom("duplicate"))),
        Error::Forbidden("scope".into()),
    ] {
        assert!(!reviewable(&error));
    }
    assert!(reviewable(&Error::BusinessLogicError("settlement needs review".into())));
}
