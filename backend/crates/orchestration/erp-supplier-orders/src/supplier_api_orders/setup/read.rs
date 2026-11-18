//! 执行资料的对象资格、进度补齐和成本展示共用原快照。
use application_core::AuditActor;
use erp_identity::AccessTarget;
use erp_permissions::{Permission, internal, supplier_fulfillment};
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt};
use erp_supply::authorization::CompiledScope;
use erp_supply::repository::{SupplierFulfillmentExt, SupplierFulfillmentOrderRepositoryExt};
use erp_supply::service::supplier_fulfillment::FulfillmentOrderAccess;
use persistence_core::{Executor, Transactional};

use super::{ExecutionSetupView, SupplierApiOrderProcess};
use crate::{Error, Result};

impl SupplierApiOrderProcess {
    /// 在当前账号的订单读取范围内返回真实执行资料。
    ///
    /// id 为订单身份，actor 为当前认证操作人。
    ///
    /// 同一快照内读取和补齐的执行阶段、配送选项及按资格显示的费用。
    ///
    /// 无权、非自动执行订单、资料不可解密或读取失败时拒绝。
    pub async fn execution_setup(&self, id: &str, actor: &AuditActor) -> Result<ExecutionSetupView> {
        self.setup_result(id, actor, &supplier_fulfillment::ORDER_DETAIL).await
    }

    pub(in super::super) async fn setup_result(
        &self,
        id: &str,
        actor: &AuditActor,
        permission: &Permission,
    ) -> Result<ExecutionSetupView> {
        let (process, id, actor, permission) =
            (self.clone(), id.to_owned(), actor.clone(), permission.clone());
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.setup_view(&id, &actor, &permission, executor).await })
            })
            .await
    }

    pub(in super::super) async fn authorized_setup(
        &self,
        id: &str,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<ApiOrderExecution> {
        if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
            return Err(Error::ValidationError("接口执行资料必须在授权快照内读取".into()));
        }
        self.authorization.require(actor, permission, AccessTarget::record(id)?, executor).await?;
        self.db
            .supplier_api_order_executions()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("此订单未登记供应商自动处理记录，请从采购单核对下单方式".into()))
    }

    pub(in super::super) async fn setup_view(
        &self,
        id: &str,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<ExecutionSetupView> {
        let (access, execution) = self.setup_access(id, actor, permission, executor).await?;
        let view = self.setup_projection(&execution, actor, executor).await?;
        access.validate(executor).await?;
        Ok(view)
    }

    pub(in super::super) async fn setup_access(
        &self,
        id: &str,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<(CompiledScope, ApiOrderExecution)> {
        let access = FulfillmentOrderAccess::new(self.db.clone(), self.authorization.clone())
            .resolve(actor, permission, executor)
            .await?;
        self.db
            .supplier_fulfillment_orders()
            .find_authorized(id, &access, executor)
            .await?
            .ok_or_else(|| Error::NotFound("对象不存在或无权访问".into()))?;

        let execution =
            self.db.supplier_api_order_executions().find_by_id(id, executor).await?.ok_or_else(|| {
                Error::NotFound("此订单未登记供应商自动处理记录，请从采购单核对下单方式".into())
            })?;
        Ok((access, execution))
    }

    pub(in super::super) async fn setup_projection(
        &self,
        execution: &ApiOrderExecution,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ExecutionSetupView> {
        let snapshot = execution.request_encrypted.as_ref().map(|_| self.decode(execution)).transpose()?;
        let (requirement, unavailable) = match self.setup_connector(execution, executor).await {
            Ok(connector) => (Some(connector.address_requirement()), None),
            Err(error) => (None, Some(error.supplier_order_message())),
        };

        let (tree, shipments) = self.execution_progress(execution, executor).await?;
        let mut view =
            ExecutionSetupView::from_execution(execution, snapshot.as_ref(), requirement, tree, shipments);
        if let Some(reason) = unavailable {
            view.blocker = Some(match view.blocker {
                Some(existing) => format!("{existing}；{reason}"),
                None => reason,
            });
        }

        if !self
            .authorization
            .has_permission(actor, &internal::SUPPLIER_OFFERING_COST_DETAIL, executor)
            .await?
        {
            view.without_costs();
        }
        Ok(view)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use erp_party::SensitiveDataCodec;
    use erp_supply::connector_registry::SupplierConnectorRegistry;
    use erp_supply::ports::connector::funds::{OrderFundsCoverage, OrderFundsQuery, SupplierOrderFunds};
    use mongodb::Client;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    struct UnusedFunds;

    #[async_trait]
    impl SupplierOrderFunds for UnusedFunds {
        type Error = Error;

        async fn execution_coverage(
            &self,
            _: &OrderFundsQuery,
            _: &mut dyn Executor,
        ) -> Result<OrderFundsCoverage> {
            panic!("资料读取不得调用资金写能力")
        }

        async fn mark_sent(&self, _: &OrderFundsQuery, _: &mut dyn Executor) -> Result<()> {
            panic!("资料读取不得标记资金发送")
        }
    }

    #[tokio::test]
    async fn detail_and_command_results_require_current_snapshot_qualification_before_execution_reads() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("setup");
        let process = SupplierApiOrderProcess::new(
            db,
            Arc::new(SupplierConnectorRegistry::default()),
            Arc::new(SensitiveDataCodec::from_secret(b"setup-test-only")),
            Arc::new(UnusedFunds),
            unavailable_authorization(),
        );
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        for permission in [supplier_fulfillment::ORDER_DETAIL, supplier_fulfillment::ORDER_SUBMIT] {
            assert!(
                matches!(process.authorized_setup("order", &actor, &permission, &mut ReadExecutor::default()).await, Err(Error::ValidationError(message)) if message == "接口执行资料必须在授权快照内读取")
            );
            assert!(
                matches!(process.authorized_setup("order", &actor, &permission, &mut SnapshotExecutor::default()).await, Err(Error::Internal(message)) if message == "测试授权设施不可用")
            );
        }
    }
}
