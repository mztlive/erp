//! 取消入口在同一快照中先证明原订单资格，再判断是否存在 API 执行。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_permissions::supplier_fulfillment;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt};
use erp_supply::service::supplier_fulfillment::FulfillmentOrderAccess;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

pub(super) struct MongoCancellationRead {
    pub db: Database,
    pub access: FulfillmentOrderAccess,
}

#[async_trait]
pub(super) trait CancellationRead: Sync {
    // 在原事务中证明当前账号有权取消该订单。
    //
    // actor/id 为原入口身份和订单，executor 为快照事务。
    //
    // 当前取消资格成立时成功。
    //
    // 身份、范围、对象或事务无效时拒绝。
    async fn authorize(&self, actor: &AuditActor, id: &str, executor: &mut dyn Executor) -> Result<()>;

    // 在已经授权的同一事务中识别 API 执行。
    //
    // id 为原订单，executor 为完成对象授权的原执行器。
    //
    // 当前 API 执行，或非 API 订单的 None。
    //
    // 仓储读取失败时返回原错误。
    async fn execution(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<ApiOrderExecution>>;
}

#[async_trait]
impl CancellationRead for MongoCancellationRead {
    async fn authorize(&self, actor: &AuditActor, id: &str, executor: &mut dyn Executor) -> Result<()> {
        self.access.require_order(actor, &supplier_fulfillment::ORDER_CANCEL, id, executor).await?;
        Ok(())
    }

    async fn execution(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<ApiOrderExecution>> {
        Ok(self.db.supplier_api_order_executions().find_by_id(id, executor).await?)
    }
}

// 在调用方快照中授权并识别 API 取消，拒绝时不读取执行记录。
//
// port 为原订单授权和执行读取设施，actor/id 为当前身份与订单，executor 为入口事务。
//
// API 执行或已授权非 API 订单的 None。
//
// 对象授权或执行读取失败时立即终止，不交给非 API 回退处理。
pub(super) async fn authorized_execution(
    port: &impl CancellationRead,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<ApiOrderExecution>> {
    port.authorize(actor, id, executor).await?;
    port.execution(id, executor).await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_core::AccountKind;
    use persistence_core::testing::SnapshotExecutor;
    use persistence_core::{ExecutionBoundary, NoTransaction};

    use super::*;
    use crate::Error;

    struct ReadPort {
        allowed: bool,
        execution: Option<ApiOrderExecution>,
        trace: Mutex<Vec<&'static str>>,
        boundary: Mutex<Option<ExecutionBoundary>>,
    }

    impl ReadPort {
        fn new(allowed: bool, api: bool) -> Self {
            Self {
                allowed,
                execution: api.then(|| {
                    ApiOrderExecution::queued(
                        "order".into(),
                        "purchase".into(),
                        "revision".into(),
                        "connection".into(),
                        "supplier".into(),
                        vec![],
                    )
                }),
                trace: Mutex::default(),
                boundary: Mutex::default(),
            }
        }
    }

    #[async_trait]
    impl CancellationRead for ReadPort {
        async fn authorize(&self, actor: &AuditActor, id: &str, executor: &mut dyn Executor) -> Result<()> {
            self.trace.lock().unwrap().push("authorize");
            assert_eq!(actor.id(), "operator");
            assert_eq!(id, "order");
            let Some(boundary) = executor.boundary().filter(ExecutionBoundary::is_snapshot) else {
                return Err(Error::ValidationError("写入授权必须使用快照事务".into()));
            };
            if !self.allowed {
                return Err(Error::Forbidden("取消权限不足".into()));
            }
            *self.boundary.lock().unwrap() = Some(boundary);
            Ok(())
        }

        async fn execution(
            &self,
            id: &str,
            executor: &mut dyn Executor,
        ) -> Result<Option<ApiOrderExecution>> {
            self.trace.lock().unwrap().push("execution");
            assert_eq!(id, "order");
            assert!(self.boundary.lock().unwrap().as_ref().unwrap().is_current(executor));
            Ok(self.execution.clone())
        }
    }

    fn actor() -> AuditActor {
        AuditActor::new("operator".into(), "operator".into(), AccountKind::Admin)
    }

    #[tokio::test]
    async fn cancellation_reads_api_and_non_api_orders_in_the_original_authorized_snapshot() {
        for api in [true, false] {
            let port = ReadPort::new(true, api);
            let result = authorized_execution(&port, &actor(), "order", &mut SnapshotExecutor::default())
                .await
                .unwrap();
            assert_eq!(result.is_some(), api);
            assert_eq!(*port.trace.lock().unwrap(), ["authorize", "execution"]);
        }
    }

    #[tokio::test]
    async fn cancellation_denial_does_not_read_api_execution_or_fall_back() {
        let port = ReadPort::new(false, false);
        assert!(matches!(
            authorized_execution(&port, &actor(), "order", &mut SnapshotExecutor::default()).await,
            Err(Error::Forbidden(_))
        ));
        assert_eq!(*port.trace.lock().unwrap(), ["authorize"]);
    }

    #[tokio::test]
    async fn cancellation_without_snapshot_stops_before_reading_execution() {
        let port = ReadPort::new(true, true);
        assert!(matches!(
            authorized_execution(&port, &actor(), "order", &mut NoTransaction).await,
            Err(Error::ValidationError(message)) if message == "写入授权必须使用快照事务"
        ));
        assert_eq!(*port.trace.lock().unwrap(), ["authorize"]);
    }
}
