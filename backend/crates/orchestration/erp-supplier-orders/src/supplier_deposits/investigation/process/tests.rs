use std::collections::HashSet;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};

use erp_core::money::Amount;
use erp_finance::entity::supplier_deposit::{
    DepositAccountIdentity, DepositCurrency, SupplierDepositAccount,
};
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::common::{ConnectorError, ConnectorResult, SourceRevision, SourceStamp};
use erp_supply::ports::connector::funds::{
    SupplierFundsCurrency, SupplierFundsDirection, SupplierFundsEvidence, SupplierFundsObservationResult,
    SupplierFundsTransaction,
};

use super::*;

type Trace = Arc<Mutex<Vec<&'static str>>>;

struct Source {
    trace: Trace,
    fail: bool,
    records: usize,
}
#[async_trait]
impl SupplierFundsSource for Source {
    fn account_reference(&self) -> &str {
        "verified-account"
    }
    async fn balance(&self) -> ConnectorResult<Snapshot<SupplierFundsBalance>> {
        self.trace.lock().unwrap().push("balance");
        Ok(snapshot(SupplierFundsBalance {
            account_reference: "verified-account".into(),
            amount: "1000".parse().unwrap(),
            currency: SupplierFundsCurrency::CNY,
            evidence: archive(),
        }))
    }
    async fn transactions(
        &self,
        _query: &SupplierFundsQuery,
    ) -> ConnectorResult<Page<Snapshot<SupplierFundsObservation>>> {
        self.trace.lock().unwrap().push("read");
        if self.fail {
            return Err(ConnectorError {
                class: SupplierFailureClass::TransientFailure,
                code: "READ_FAILED".into(),
                summary: "读取失败".into(),
                retry_after: None,
            });
        }
        Ok(Page {
            items: (0..self.records)
                .map(|_| {
                    snapshot(SupplierFundsObservation {
                        evidence: archive(),
                        result: SupplierFundsObservationResult::Transaction(SupplierFundsTransaction {
                            stable_transaction_id: "transaction".into(),
                            external_order_id: Some("original-order".into()),
                            direction: SupplierFundsDirection::Debit,
                            amount: "100".parse().unwrap(),
                            currency: SupplierFundsCurrency::CNY,
                            occurred_at: Instant::from_unix_secs(2),
                            evidence_reference: "proof".into(),
                            original_transaction_id: None,
                            bank_reference: None,
                            business_code: "verified-debit".into(),
                        }),
                    })
                })
                .collect(),
            next: None,
        })
    }
}

struct State {
    pending: Vec<Snapshot<SupplierFundsObservation>>,
    applied: HashSet<String>,
    checkpoint: bool,
    account: SupplierDepositAccount,
}
struct Store {
    trace: Trace,
    fail_receive: bool,
    state: Mutex<State>,
}
impl Store {
    fn new(trace: Trace, fail_receive: bool) -> Self {
        let mut account = SupplierDepositAccount::new(
            "local-account".into(),
            DepositAccountIdentity {
                payer_party_id: "payer".into(),
                supplier_id: "supplier".into(),
                external_account_ref: "verified-account".into(),
                currency: DepositCurrency::CNY,
            },
        )
        .unwrap();
        account.credit("1000".parse().unwrap()).unwrap();
        account.reserve("100".parse().unwrap()).unwrap();
        Self {
            trace,
            fail_receive,
            state: Mutex::new(State { pending: vec![], applied: HashSet::new(), checkpoint: false, account }),
        }
    }
}
#[async_trait]
impl FundsInvestigationStore for Store {
    fn account_reference(&self) -> &str {
        "verified-account"
    }
    async fn resume(&self) -> Result<FundsInvestigationRun> {
        self.trace.lock().unwrap().push("resume");
        let mut state = self.state.lock().unwrap();
        let pending = std::mem::take(&mut state.pending);
        let mut result = FundsInvestigationRun::default();
        for item in pending {
            if let SupplierFundsObservationResult::Transaction(transaction) = item.value.result
                && state.applied.insert(transaction.stable_transaction_id)
            {
                state.account.debit(transaction.amount, transaction.amount)?;
                result.applied += 1;
            }
        }
        Ok(result)
    }
    async fn query(&self, _now: Instant) -> Result<SupplierFundsQuery> {
        self.trace.lock().unwrap().push("checkpoint");
        Ok(SupplierFundsQuery {
            since: Instant::from_unix_secs(1),
            through: Instant::from_unix_secs(10),
            cursor: None,
            limit: NonZeroU32::new(1).unwrap(),
        })
    }
    async fn receive_page(
        &self,
        _query: &SupplierFundsQuery,
        page: Page<Snapshot<SupplierFundsObservation>>,
    ) -> Result<usize> {
        self.trace.lock().unwrap().push("receive");
        if self.fail_receive {
            return Err(Error::BusinessLogicError("保存失败".into()));
        }
        let mut state = self.state.lock().unwrap();
        let count = page.items.len();
        state.pending.extend(page.items);
        state.checkpoint = true;
        Ok(count)
    }
    async fn observe_balance(&self, _balance: Snapshot<SupplierFundsBalance>) -> Result<()> {
        self.trace.lock().unwrap().push("save-balance");
        Ok(())
    }
}

fn archive() -> SupplierFundsEvidence {
    SupplierFundsEvidence {
        reference: "proof".into(),
        body_encrypted: "ciphertext".into(),
        content_fingerprint: "source-hash".into(),
    }
}
fn snapshot<T>(value: T) -> Snapshot<T> {
    Snapshot {
        stamp: SourceStamp {
            revision: SourceRevision::Unversioned,
            changed_at: None,
            observed_at: Instant::from_unix_secs(10),
        },
        value,
    }
}

#[tokio::test]
async fn persists_before_application_and_replay_never_debits_twice_or_uses_remote_balance() {
    let trace = Arc::new(Mutex::new(vec![]));
    let source = Source { trace: trace.clone(), fail: false, records: 1 };
    let store = Store::new(trace.clone(), false);
    let process = FundsInvestigationProcess { source: &source, store: &store };

    let result = process.run(Instant::from_unix_secs(10)).await.unwrap();
    assert_eq!(result.applied, 1);
    assert_eq!(
        *trace.lock().unwrap(),
        ["resume", "checkpoint", "read", "receive", "resume", "balance", "save-balance"]
    );
    assert_eq!(store.state.lock().unwrap().account.book_balance, "900".parse::<Amount>().unwrap());

    assert_eq!(process.run(Instant::from_unix_secs(10)).await.unwrap().applied, 0);
    assert_eq!(store.state.lock().unwrap().account.book_balance, "900".parse::<Amount>().unwrap());
}

#[tokio::test]
async fn failed_page_never_advances_checkpoint_or_applies_unsaved_funds() {
    let trace = Arc::new(Mutex::new(vec![]));
    let source = Source { trace: trace.clone(), fail: false, records: 1 };
    let store = Store::new(trace.clone(), true);
    assert!(
        FundsInvestigationProcess { source: &source, store: &store }
            .run(Instant::from_unix_secs(10))
            .await
            .is_err()
    );
    assert_eq!(*trace.lock().unwrap(), ["resume", "checkpoint", "read", "receive"]);
    assert!(!store.state.lock().unwrap().checkpoint);
    assert!(store.state.lock().unwrap().applied.is_empty());
}

#[tokio::test]
async fn query_failure_and_empty_page_do_not_manufacture_payment_conclusions() {
    let trace = Arc::new(Mutex::new(vec![]));
    let store = Store::new(trace.clone(), false);
    let source = Source { trace: trace.clone(), fail: true, records: 0 };
    assert!(
        FundsInvestigationProcess { source: &source, store: &store }
            .run(Instant::from_unix_secs(10))
            .await
            .is_err()
    );
    assert!(!store.state.lock().unwrap().checkpoint);

    let source = Source { trace, fail: false, records: 0 };
    let result = FundsInvestigationProcess { source: &source, store: &store }
        .run(Instant::from_unix_secs(10))
        .await
        .unwrap();
    assert_eq!(result.applied, 0);
    assert_eq!(store.state.lock().unwrap().account.reserved_amount, "100".parse::<Amount>().unwrap());
}
