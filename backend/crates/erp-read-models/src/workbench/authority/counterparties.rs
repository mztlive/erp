//! 往来编号与来源映射存在性；保留原逐个读取和未知种类语义。
use super::WorkItemFactsReader;
impl WorkItemFactsReader {
    /// Whether a customer or supplier counterparty is currently active.
    pub async fn counterparty_is_active(
        &self,
        kind: &str,
        id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> crate::errors::Result<bool> {
        Ok(self.counterparty_numbers(kind, &[id.to_string()], executor).await?.contains_key(id))
    }

    /// Display numbers for counterparties of one kind.
    pub async fn counterparty_numbers(
        &self,
        kind: &str,
        ids: &[String],
        executor: &mut dyn persistence_core::Executor,
    ) -> crate::errors::Result<std::collections::HashMap<String, String>> {
        numbers(self, kind, ids, executor).await
    }

    /// 检查外部身份映射是否存在，保留调用方的读取边界。
    /// 返回原持久化错误，由原 workflow 边界决定错误分类和文案。
    pub async fn external_identity_map_exists(
        &self,
        id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> persistence_core::Result<bool> {
        use erp_support::SourceRegistryExt;
        Ok(self.db.external_identity_maps().find_by_id(id, executor).await?.is_some())
    }
}

#[async_trait::async_trait]
trait CounterpartyReads: Send + Sync {
    async fn number(
        &self,
        kind: Kind,
        id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> crate::errors::Result<Option<String>>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Supplier,
    Customer,
}
#[async_trait::async_trait]
impl CounterpartyReads for WorkItemFactsReader {
    async fn number(
        &self,
        kind: Kind,
        id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> crate::errors::Result<Option<String>> {
        use erp_core::ids::{CustomerAccountId, SupplierAccountId};
        use erp_customer::CustomerExt;
        use erp_supplier::SupplierExt;
        Ok(match kind {
            Kind::Supplier => self
                .db
                .supplier_accounts()
                .find_by_id(&SupplierAccountId::new(id), executor)
                .await?
                .map(|account| account.supplier_no),
            Kind::Customer => self
                .db
                .customer_accounts()
                .find_by_id(&CustomerAccountId::new(id), executor)
                .await?
                .map(|account| account.customer_no),
        })
    }
}
async fn numbers(
    source: &impl CounterpartyReads,
    kind: &str,
    ids: &[String],
    executor: &mut dyn persistence_core::Executor,
) -> crate::errors::Result<std::collections::HashMap<String, String>> {
    let mut numbers = std::collections::HashMap::new();
    let kind = match kind {
        "supplier" => Kind::Supplier,
        "customer" => Kind::Customer,
        _ => return Ok(numbers),
    };
    for id in ids {
        if let Some(number) = source.number(kind, id, executor).await? {
            numbers.insert(id.clone(), number);
        }
    }
    Ok(numbers)
}
#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::errors::Error;
    struct TestExecutor {
        reads: usize,
    }
    impl persistence_core::Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            self.reads += 1;
            None
        }
    }
    struct Source {
        seen: Mutex<Vec<(Kind, String)>>,
        executor: usize,
    }
    #[async_trait::async_trait]
    impl CounterpartyReads for Source {
        async fn number(
            &self,
            kind: Kind,
            id: &str,
            executor: &mut dyn persistence_core::Executor,
        ) -> crate::errors::Result<Option<String>> {
            assert_eq!(executor as *mut dyn persistence_core::Executor as *mut () as usize, self.executor);
            executor.session();
            self.seen.lock().unwrap().push((kind, id.to_string()));
            match id {
                "missing" => Ok(None),
                "error" => Err(Error::ValidationError("number-read".to_string())),
                _ => Ok(Some(format!("{id}-number"))),
            }
        }
    }
    #[tokio::test]
    async fn counterparty_recipe_preserves_duplicate_reads_input_order_and_same_executor() {
        let mut ex = TestExecutor { reads: 0 };
        let source = Source { seen: Mutex::new(Vec::new()), executor: &mut ex as *mut TestExecutor as usize };
        let ids =
            vec!["second".to_string(), "missing".to_string(), "second".to_string(), "first".to_string()];
        let result = numbers(&source, "supplier", &ids, &mut ex).await.unwrap();
        assert_eq!(source.seen.lock().unwrap().iter().map(|x| x.1.clone()).collect::<Vec<_>>(), ids);
        assert_eq!(result.len(), 2);
        assert_eq!(result["second"], "second-number");
        assert_eq!(ex.reads, 4);
    }
    #[tokio::test]
    async fn unknown_kind_performs_no_reads_and_first_error_stops_later_ids() {
        let mut ex = TestExecutor { reads: 0 };
        let source = Source { seen: Mutex::new(Vec::new()), executor: &mut ex as *mut TestExecutor as usize };
        let ids = vec!["first".to_string(), "error".to_string(), "never".to_string()];
        assert!(numbers(&source, "SUPPLIER", &ids, &mut ex).await.unwrap().is_empty());
        assert_eq!(ex.reads, 0);
        let error = numbers(&source, "customer", &ids, &mut ex).await.unwrap_err();
        assert!(matches!(error,Error::ValidationError(message) if message=="number-read"));
        assert_eq!(
            *source.seen.lock().unwrap(),
            vec![(Kind::Customer, "first".to_string()), (Kind::Customer, "error".to_string())]
        );
        assert_eq!(ex.reads, 2);
    }
}
