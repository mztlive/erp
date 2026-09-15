//! 审计后重新读取核销关联，再依原顺序逐销售单刷新回款进度。

use async_trait::async_trait;
use erp_core::ids::SalesOrderId;
use persistence_core::Executor;

use crate::Result;

/// 仅暴露审计后的新读取和单张销售单刷新；生产与替身共用相同 runner。
#[async_trait]
pub(super) trait AffectedSales: Send {
    async fn load_sales(&mut self, executor: &mut dyn Executor) -> Result<Vec<SalesOrderId>>;
    async fn refresh_sale(&mut self, id: &SalesOrderId, executor: &mut dyn Executor) -> Result<()>;
}

/// 任何读取或单销售刷新失败立即停止，不预先访问后续销售单。
pub(super) async fn refresh_affected_sales(
    port: &mut impl AffectedSales,
    executor: &mut dyn Executor,
) -> Result<()> {
    // 冲正后刷新销售单回款进度与关闭状态（已结清可能退回部分回款）。
    // 必须重新查询审计写入后的全部分配，不复用逆转准备阶段的分配快照。
    let sales_order_ids = port.load_sales(executor).await?;
    for sales_order_id in sales_order_ids {
        port.refresh_sale(&sales_order_id, executor).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{ReceiptReversalPosting, post};
    use super::*;
    use crate::Error;

    struct TestExecutor {
        _identity: u8,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }

    #[derive(Default)]
    struct RecordingPosting {
        events: Vec<String>,
        executors: Vec<usize>,
        allocations: Vec<SalesOrderId>,
        fail: Option<&'static str>,
    }
    impl RecordingPosting {
        fn record(&mut self, step: &str, executor: &mut dyn Executor) -> Result<()> {
            self.events.push(step.to_string());
            self.executors.push(executor as *mut dyn Executor as *mut () as usize);
            if self.fail == Some(step) {
                return Err(Error::ConflictError(step.to_string()));
            }
            Ok(())
        }
    }

    #[async_trait]
    impl ReceiptReversalPosting for RecordingPosting {
        async fn reverse_finance(&mut self, executor: &mut dyn Executor) -> Result<()> {
            self.record("finance", executor)
        }
        async fn post_reversal(&mut self, executor: &mut dyn Executor) -> Result<()> {
            self.record("returns", executor)
        }
        async fn audit(&mut self, executor: &mut dyn Executor) -> Result<()> {
            self.record("audit", executor)?;
            self.allocations = ["sales-1", "sales-2", "sales-3"].into_iter().map(SalesOrderId::new).collect();
            Ok(())
        }
        async fn refresh_sales(&mut self, executor: &mut dyn Executor) -> Result<()> {
            refresh_affected_sales(self, executor).await
        }
    }

    #[async_trait]
    impl AffectedSales for RecordingPosting {
        async fn load_sales(&mut self, executor: &mut dyn Executor) -> Result<Vec<SalesOrderId>> {
            self.record("load_allocations", executor)?;
            Ok(self.allocations.clone())
        }
        async fn refresh_sale(&mut self, id: &SalesOrderId, executor: &mut dyn Executor) -> Result<()> {
            self.record(id.as_ref(), executor)
        }
    }

    #[tokio::test]
    async fn final_post_reloads_after_audit_and_refreshes_each_new_sale_with_same_executor() {
        let mut port =
            RecordingPosting { allocations: vec![SalesOrderId::new("stale-sales")], ..Default::default() };
        let mut executor = TestExecutor { _identity: 1 };
        let expected = &mut executor as *mut TestExecutor as usize;
        post(&mut port, &mut executor).await.unwrap();
        assert_eq!(
            port.events,
            ["finance", "returns", "audit", "load_allocations", "sales-1", "sales-2", "sales-3"]
        );
        assert_eq!(port.executors, vec![expected; 7]);
    }

    #[tokio::test]
    async fn allocation_reload_failure_stops_before_any_sales_refresh() {
        let mut port = RecordingPosting { fail: Some("load_allocations"), ..Default::default() };
        let mut executor = TestExecutor { _identity: 1 };
        let error = post(&mut port, &mut executor).await.unwrap_err();
        assert!(matches!(error, Error::ConflictError(message) if message == "load_allocations"));
        assert_eq!(port.events, ["finance", "returns", "audit", "load_allocations"]);
    }

    #[tokio::test]
    async fn middle_sales_failure_stops_before_later_sales_with_same_executor() {
        let mut port = RecordingPosting { fail: Some("sales-2"), ..Default::default() };
        let mut executor = TestExecutor { _identity: 1 };
        let expected = &mut executor as *mut TestExecutor as usize;
        let error = post(&mut port, &mut executor).await.unwrap_err();
        assert!(matches!(error, Error::ConflictError(message) if message == "sales-2"));
        assert_eq!(port.events, ["finance", "returns", "audit", "load_allocations", "sales-1", "sales-2"]);
        assert_eq!(port.executors, vec![expected; 6]);
    }
}
