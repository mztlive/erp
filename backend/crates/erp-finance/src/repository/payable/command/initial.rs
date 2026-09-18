//! 应付账户及原始分录的生产写入顺序。
use async_trait::async_trait;
use mongodb::Database;
use persistence_core::{Executor, Result, mongo_ops};

use crate::entity::payable::{PayableAccount, PayableEntry};
use crate::repository::PayableExt;
#[async_trait]
#[allow(async_fn_in_trait)]
trait Store: Send {
    async fn account(&mut self, account: &PayableAccount, ex: &mut dyn Executor) -> Result<()>;
    async fn entry(&mut self, entry: &PayableEntry, ex: &mut dyn Executor) -> Result<()>;
}
async fn execute(
    store: &mut impl Store,
    account: &PayableAccount,
    entry: &PayableEntry,
    ex: &mut dyn Executor,
) -> Result<()> {
    store.account(account, ex).await?;
    store.entry(entry, ex).await
}
struct MongoStore<'a>(&'a Database);
#[async_trait]
impl Store for MongoStore<'_> {
    async fn account(&mut self, account: &PayableAccount, ex: &mut dyn Executor) -> Result<()> {
        mongo_ops::insert_one(
            &self.0.collection::<PayableAccount>(<Database as PayableExt>::PAYABLE_ACCOUNTS),
            account,
            ex,
        )
        .await?;
        Ok(())
    }
    async fn entry(&mut self, entry: &PayableEntry, ex: &mut dyn Executor) -> Result<()> {
        mongo_ops::insert_one(&self.0.collection::<PayableEntry>(super::super::PAYABLE_ENTRIES), entry, ex)
            .await?;
        Ok(())
    }
}
pub(super) async fn create(
    db: &Database,
    account: &PayableAccount,
    entry: &PayableEntry,
    ex: &mut dyn Executor,
) -> Result<()> {
    execute(&mut MongoStore(db), account, entry, ex).await
}
#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::{BusinessDate, Instant};
    use erp_core::ids::SupplierAccountId;
    use erp_core::money::Amount;

    use super::*;
    use crate::service::payable::supplier_settlement::{SettlementPayableSource, build_settlement_payable};
    struct TestExecutor {
        _identity: u8,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    struct Recording {
        calls: Vec<&'static str>,
        executor: usize,
        fail: Option<&'static str>,
    }
    impl Recording {
        fn visit(&mut self, name: &'static str, ex: &mut dyn Executor) -> Result<()> {
            assert_eq!(ex as *mut dyn Executor as *mut () as usize, self.executor);
            self.calls.push(name);
            if self.fail == Some(name) {
                return Err(persistence_core::Error::OptimisticLockingError);
            }
            Ok(())
        }
    }
    #[async_trait]
    impl Store for Recording {
        async fn account(&mut self, _: &PayableAccount, ex: &mut dyn Executor) -> Result<()> {
            self.visit("account", ex)
        }
        async fn entry(&mut self, _: &PayableEntry, ex: &mut dyn Executor) -> Result<()> {
            self.visit("entry", ex)
        }
    }
    #[tokio::test]
    async fn account_then_entry_share_executor_and_stop_at_every_failure() {
        let source = SettlementPayableSource {
            statement_no: "ST-1".into(),
            supplier_id: SupplierAccountId::new("supplier-1"),
            subject_hash: "a".repeat(64),
            period_end: BusinessDate::from_ymd(2026, 7, 31).unwrap(),
        };
        let (account, entry) = build_settlement_payable(
            &source,
            Amount::from_str("100.00").unwrap(),
            "reviewer-1",
            Instant::from_unix_secs(1700000000),
        )
        .unwrap();
        for fail in [None, Some("account"), Some("entry")] {
            let mut ex = TestExecutor { _identity: 1 };
            let mut store =
                Recording { calls: vec![], executor: &mut ex as *mut TestExecutor as usize, fail };
            let result = execute(&mut store, &account, &entry, &mut ex).await;
            if fail.is_some() {
                assert!(matches!(result, Err(persistence_core::Error::OptimisticLockingError)));
            } else {
                result.unwrap();
            }
            assert_eq!(
                store.calls,
                if fail == Some("account") { vec!["account"] } else { vec!["account", "entry"] }
            );
        }
    }
}
