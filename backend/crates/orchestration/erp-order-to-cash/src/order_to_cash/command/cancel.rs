use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_core::ids::SalesOrderId;
use erp_permissions::sales_order;
use erp_read_models::sales_center::order::dto::SalesOrderDetailView;
use erp_sales::dto::sales_order::CancelSalesOrderApprovalRequest;
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::repository::{SalesOrderExt, SalesOrderSubmissionRepositoryExt};
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::prepare_cancel;
use erp_workflow::service::document_registry::find_approval_binding;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::super::adapter::{
    execute_sales_order_domain_action, require_frozen_binding, sales_approval_ports,
};
use super::super::cancel_approval::{
    SalesOrderCancelPersistInput, build_sales_order_cancel_input, load_cancel_runtime,
    persist_sales_order_cancel,
};
use super::super::{SalesOrderCommandProcess, subject_ref_for_sales_business};
use crate::{Error, Result};

impl SalesOrderCommandProcess {
    /// 撤回审批中的销售单，回到可修正草稿。
    ///
    /// `GoodsService` 与 `Voucher` 均先按主体加载 RUNNING/BLOCKED 实例并调用
    /// 统一 `prepare_cancel`，关闭开放任务后再执行 `cancel_action`。
    /// 已 `APPROVED` 必须拒绝。
    ///
    /// # 参数
    /// * `id` - 销售单主键
    /// * `req` - 撤回请求（原因必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回撤回后的销售单详情。
    ///
    /// # 错误
    /// 非审批中、已最终通过、原因缺失或并发冲突时返回错误。
    #[tracing::instrument(
        name = "sales_order.cancel_approval",
        skip_all,
        fields(layer = "service", domain = "sales_order", operation = "cancel_approval")
    )]
    pub async fn cancel_approval_submission(
        &self,
        id: &str,
        req: CancelSalesOrderApprovalRequest,
        actor: &AuditActor,
    ) -> Result<SalesOrderDetailView> {
        req.validate()?;
        let service = self.clone();
        let id_copy = id.to_string();
        let actor_copy = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let command =
                        Cancellation { service: &service, id: &id_copy, request: &req, actor: &actor_copy };
                    cancel(&command, req.expected_version, executor).await
                })
            })
            .await?;

        self.command_result(id, actor, &sales_order::CANCEL_APPROVAL).await
    }

    /// 对象授权和版本检查通过后，在原事务准备并应用取消计划。
    async fn cancel_stage(
        &self,
        id: &str,
        req: &CancelSalesOrderApprovalRequest,
        actor: &AuditActor,
        mut order: SalesOrder,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ports = sales_approval_ports(order.business_type)?;
        let binding = find_approval_binding(&self.db, id, executor).await.map_err(Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let subject = subject_ref_for_sales_business(order.business_type, id)
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        let subject_version = self
            .db
            .sales_order_submissions()
            .find_latest_by_order(&SalesOrderId::new(id), executor)
            .await?
            .map(|submission| submission.submission_no)
            .ok_or_else(|| Error::ConflictError("销售单没有可撤回的提交版本".into()))?;
        let runtime = load_cancel_runtime(&self.db, &binding, &subject, subject_version, executor).await?;

        let now = Instant::now();
        let idempotency_key = normalize_idempotency_key(&req.idempotency_key)?;
        let input =
            build_sales_order_cancel_input(&runtime, &req.reason, actor.id(), &idempotency_key, None, now)?;
        let prepared = prepare_cancel(input)?;

        execute_sales_order_domain_action(&mut order, ports.cancel_action, actor.id())?;
        let audit =
            actor.clone().resource_log("sales_order.cancel_approval", "sales_order", id.to_string())?;
        persist_sales_order_cancel(
            &self.db,
            self.services.approvals.as_ref(),
            SalesOrderCancelPersistInput {
                order,
                prepared,
                open_tasks: runtime.open_tasks,
                actor_id: actor.id().to_string(),
                reason: req.reason.clone(),
                now,
                audit,
            },
            executor,
        )
        .await
    }
}

/// 撤回命令只将完整对象授权和后续取消执行作为替身边界。
#[async_trait]
trait CancellationSource: Sync {
    /// 在原事务中返回已授权的当前销售单。
    async fn authorized_order(&self, executor: &mut dyn Executor) -> Result<SalesOrder>;

    /// 使用已检查版本的单据继续准备和写入取消计划。
    async fn apply(&self, order: SalesOrder, executor: &mut dyn Executor) -> Result<()>;
}

/// 原命令输入在整个撤回事务中保持固定。
struct Cancellation<'a> {
    service: &'a SalesOrderCommandProcess,
    id: &'a str,
    request: &'a CancelSalesOrderApprovalRequest,
    actor: &'a AuditActor,
}

#[async_trait]
impl CancellationSource for Cancellation<'_> {
    /// 完整授权先于版本、审批绑定和运行状态检查。
    async fn authorized_order(&self, executor: &mut dyn Executor) -> Result<SalesOrder> {
        self.service.command_access(self.actor, sales_order::CANCEL_APPROVAL).current(self.id, executor).await
    }

    /// 原事务继续读取审批事实并写入，不创建新执行器。
    async fn apply(&self, order: SalesOrder, executor: &mut dyn Executor) -> Result<()> {
        self.service.cancel_stage(self.id, self.request, self.actor, order, executor).await
    }
}

/// 先证明完整对象资格，再暴露版本结果及取消状态。
async fn cancel(
    source: &impl CancellationSource,
    expected_version: u64,
    executor: &mut dyn Executor,
) -> Result<()> {
    let order = source.authorized_order(executor).await?;
    if !order.matches_version(expected_version) {
        return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".into()));
    }
    source.apply(order, executor).await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_core::ids::{CustomerAccountId, PartyId};
    use erp_sales::entity::sales_order::{BusinessType, OriginSystem, SalesOrderData};
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    enum Failure {
        Denied,
        Unavailable,
        Apply,
    }

    struct Source {
        order: SalesOrder,
        failure: Option<Failure>,
        events: Mutex<Vec<&'static str>>,
        executors: Mutex<Vec<usize>>,
    }

    impl Source {
        /// 构造真实销售实体及可控授权、取消设施结果。
        fn new(failure: Option<Failure>) -> Self {
            let order = SalesOrder::new(
                SalesOrderId::new("order"),
                SalesOrderData {
                    sales_owner_user_id: "owner".into(),
                    business_org_unit_id: "org".into(),
                    order_no: "SO-1".into(),
                    business_type: BusinessType::GoodsService,
                    origin_system: OriginSystem::Erp,
                    source_identity_id: None,
                    customer_id: CustomerAccountId::new("customer"),
                    contract_id: None,
                    settlement_party_id: PartyId::new("party"),
                    source_status_code: None,
                },
                "owner",
            )
            .unwrap();
            Self { order, failure, events: Mutex::new(vec![]), executors: Mutex::new(vec![]) }
        }

        /// 记录实际生产编排传入的阶段执行器。
        fn record(&self, event: &'static str, executor: &mut dyn Executor) {
            self.events.lock().unwrap().push(event);
            self.executors.lock().unwrap().push(executor as *mut dyn Executor as *mut () as usize);
        }
    }

    #[async_trait]
    impl CancellationSource for Source {
        /// 拒绝和设施错误必须先于对象版本错误返回。
        async fn authorized_order(&self, executor: &mut dyn Executor) -> Result<SalesOrder> {
            self.record("authorize", executor);
            match self.failure {
                Some(Failure::Denied) => Err(Error::NotFound("销售单不存在或无权操作".into())),
                Some(Failure::Unavailable) => Err(Error::Internal("身份读取失败".into())),
                _ => Ok(self.order.clone()),
            }
        }

        /// 只有授权和版本均满足才允许进入取消准备与写入。
        async fn apply(&self, order: SalesOrder, executor: &mut dyn Executor) -> Result<()> {
            self.record("apply", executor);
            assert_eq!(order.base.id, self.order.base.id);
            assert_eq!(order.base.version, self.order.base.version);
            if matches!(self.failure, Some(Failure::Apply)) {
                return Err(Error::ConflictError("审批状态已变化".into()));
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn cancellation_denial_does_not_reveal_a_stale_order_version() {
        let source = Source::new(Some(Failure::Denied));
        let result = cancel(&source, source.order.base.version + 1, &mut SnapshotExecutor::default()).await;
        assert!(matches!(result, Err(Error::NotFound(message)) if message == "销售单不存在或无权操作"));
        assert_eq!(*source.events.lock().unwrap(), ["authorize"]);
    }

    #[tokio::test]
    async fn cancellation_identity_error_stops_before_version_and_runtime_checks() {
        let source = Source::new(Some(Failure::Unavailable));
        let result = cancel(&source, source.order.base.version + 1, &mut SnapshotExecutor::default()).await;
        assert!(matches!(result, Err(Error::Internal(message)) if message == "身份读取失败"));
        assert_eq!(*source.events.lock().unwrap(), ["authorize"]);
    }

    #[tokio::test]
    async fn cancellation_stale_authorized_version_does_not_prepare_or_write() {
        let source = Source::new(None);
        let result = cancel(&source, source.order.base.version + 1, &mut SnapshotExecutor::default()).await;
        assert!(
            matches!(result, Err(Error::ConflictError(message)) if message == "数据已被其他请求修改，请刷新后重试")
        );
        assert_eq!(*source.events.lock().unwrap(), ["authorize"]);
    }

    #[tokio::test]
    async fn cancellation_authorization_and_apply_keep_the_same_executor_and_errors() {
        for failure in [None, Some(Failure::Apply)] {
            let source = Source::new(failure);
            let mut executor = SnapshotExecutor::default();
            let expected = &mut executor as *mut SnapshotExecutor as usize;

            let result = cancel(&source, source.order.base.version, &mut executor).await;
            if matches!(source.failure, Some(Failure::Apply)) {
                assert!(matches!(result, Err(Error::ConflictError(message)) if message == "审批状态已变化"));
            } else {
                result.unwrap();
            }
            assert_eq!(*source.events.lock().unwrap(), ["authorize", "apply"]);
            assert_eq!(*source.executors.lock().unwrap(), [expected, expected]);
        }
    }
}
