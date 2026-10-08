//! 命令读取的固定生产调度；每个步骤失败立即终止，复用原调用方 Executor。
use std::collections::HashSet;

use async_trait::async_trait;
use erp_workflow::ports::{ObjectFactKey, ObjectFactMap};
use persistence_core::Executor;

use crate::errors::Result;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    Sales,
    Purchase,
    Fulfillment,
    PurchaseChange,
    SalesChange,
    Receivable,
    Payable,
    CustomerReceipt,
    CustomerRefund,
    ReceiptReversal,
    SupplierPayment,
    SupplierRefund,
    PaymentReversal,
    Inventory,
    Settlement,
    LegacyImport,
    IntegrationError,
    Reconciliation,
    SupplierFulfillment,
    SupplierOffering,
    SupplierPortal,
}
const ORDER: [Step; 21] = [
    Step::Sales,
    Step::Purchase,
    Step::Fulfillment,
    Step::PurchaseChange,
    Step::SalesChange,
    Step::Receivable,
    Step::Payable,
    Step::CustomerReceipt,
    Step::CustomerRefund,
    Step::ReceiptReversal,
    Step::SupplierPayment,
    Step::SupplierRefund,
    Step::PaymentReversal,
    Step::Inventory,
    Step::Settlement,
    Step::LegacyImport,
    Step::IntegrationError,
    Step::Reconciliation,
    Step::SupplierFulfillment,
    Step::SupplierOffering,
    Step::SupplierPortal,
];
#[async_trait]
pub(super) trait CommandFactReads: Send + Sync {
    /// 按固定步骤读取一类对象事实并写入 `facts`。
    ///
    /// # 参数
    /// * `step` - 当前调度步骤。
    /// * `keys` - 本批对象键；空键仍执行该步。
    /// * `facts` - 累积写入的权威事实。
    /// * `executor` - 调用方执行器，各步复用同一个。
    ///
    /// # 返回
    /// 该步读取完成。
    ///
    /// # 错误
    /// 该步仓储或映射失败时返回对应错误；调用方应停止后续步骤。
    async fn read(
        &self,
        step: Step,
        keys: &HashSet<ObjectFactKey>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()>;
}
/// 按固定顺序执行全部命令读取步骤。
///
/// # 参数
/// * `port` - 各步的读取实现。
/// * `keys` - 本批对象键；空键仍执行每一步。
/// * `executor` - 全程复用的执行器。
///
/// # 返回
/// 返回各步累积的权威事实表。
///
/// # 错误
/// 任一步失败立即返回该错误，不再执行后续步骤。
pub(super) async fn load(
    port: &impl CommandFactReads,
    keys: &HashSet<ObjectFactKey>,
    executor: &mut dyn Executor,
) -> Result<ObjectFactMap> {
    let mut facts = ObjectFactMap::new();
    for step in ORDER {
        port.read(step, keys, &mut facts, executor).await?;
    }
    Ok(facts)
}
#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::errors::Error;
    struct TestExecutor {
        calls: usize,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            self.calls += 1;
            None
        }
    }
    struct Reads {
        seen: Mutex<Vec<Step>>,
        executor: usize,
        fail: Option<Step>,
    }
    #[async_trait]
    impl CommandFactReads for Reads {
        async fn read(
            &self,
            step: Step,
            keys: &HashSet<ObjectFactKey>,
            facts: &mut ObjectFactMap,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            assert!(keys.is_empty());
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.executor);
            executor.session();
            self.seen.lock().unwrap().push(step);
            if self.fail == Some(step) {
                return Err(Error::ConflictError("first-read-failure".to_string()));
            }
            facts.insert(
                (erp_workflow::ports::ObjectKind::SalesOrder, format!("{step:?}")),
                erp_workflow::ports::ObjectFact::new("root", "title", "creator"),
            );
            Ok(())
        }
    }
    #[tokio::test]
    async fn command_recipe_preserves_every_step_and_same_non_zst_executor_even_for_empty_keys() {
        let mut executor = TestExecutor { calls: 0 };
        let port = Reads {
            seen: Mutex::new(Vec::new()),
            executor: &mut executor as *mut TestExecutor as usize,
            fail: None,
        };
        let facts = load(&port, &HashSet::new(), &mut executor).await.unwrap();
        assert_eq!(
            *port.seen.lock().unwrap(),
            vec![
                Step::Sales,
                Step::Purchase,
                Step::Fulfillment,
                Step::PurchaseChange,
                Step::SalesChange,
                Step::Receivable,
                Step::Payable,
                Step::CustomerReceipt,
                Step::CustomerRefund,
                Step::ReceiptReversal,
                Step::SupplierPayment,
                Step::SupplierRefund,
                Step::PaymentReversal,
                Step::Inventory,
                Step::Settlement,
                Step::LegacyImport,
                Step::IntegrationError,
                Step::Reconciliation,
                Step::SupplierFulfillment,
                Step::SupplierOffering,
                Step::SupplierPortal,
            ]
        );
        assert_eq!(executor.calls, 21);
        assert_eq!(facts.len(), 21);
    }
    #[tokio::test]
    async fn each_first_read_error_preserves_variant_and_stops_later_reads() {
        for (index, step) in ORDER.iter().enumerate() {
            let mut executor = TestExecutor { calls: 0 };
            let port = Reads {
                seen: Mutex::new(Vec::new()),
                executor: &mut executor as *mut TestExecutor as usize,
                fail: Some(*step),
            };
            let error = load(&port, &HashSet::new(), &mut executor).await.unwrap_err();
            assert!(matches!(error,Error::ConflictError(message) if message=="first-read-failure"));
            assert_eq!(&*port.seen.lock().unwrap(), &ORDER[..=index]);
            assert_eq!(executor.calls, index + 1);
        }
    }
}
