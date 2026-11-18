//! 取消及退款结果使用原动作资格，在新读取快照中过滤完整订单并完成交付检查。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::ids::SupplierOrderActionId;
use erp_permissions::{Permission, supplier_fulfillment};
use erp_supply::CompiledScope;
use erp_supply::api_order::ApiOrderExt;
use erp_supply::dto::supplier_fulfillment::SubmitActionResultView;
use erp_supply::repository::{
    SupplierFulfillmentExt, SupplierFulfillmentOrderRepositoryExt, SupplierOrderActionLineRepositoryExt,
};
use erp_supply::service::supplier_fulfillment::FulfillmentOrderAccess;
use erp_supply::service::supplier_fulfillment::mapping::action_line_view;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::{Error, Result};

/// 仅由原命令构造的结果定位，不改变该动作的完整资格和范围。
pub(crate) enum ActionResult {
    Refund(String),
    Cancellation,
}

impl ActionResult {
    /// 使用原退款或取消资格，不追加独立详情资格。
    fn permission(&self) -> Permission {
        match self {
            Self::Refund(_) => supplier_fulfillment::ORDER_REFUND,
            Self::Cancellation => supplier_fulfillment::ORDER_CANCEL,
        }
    }
}

struct MongoActionResultRead {
    db: Database,
    access: FulfillmentOrderAccess,
}

#[async_trait]
trait ActionResultRead: Sync {
    type Scope: Send + Sync;
    type View: Send;

    /// 在当前读取阶段取得原命令动作的完整条件。
    /// # 参数
    /// actor/permission 为真实身份及固定动作；executor 为新读取快照。
    /// # 返回
    /// 仅供本次完整结果读取的查询条件。
    /// # 错误
    /// 身份、功能资格、范围或执行边界无效时拒绝。
    async fn authorize(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<Self::Scope>;

    /// 对完整订单执行授权过滤，再按精确订单读取动作与明细。
    /// # 参数
    /// order_id/target 为真实结果定位；scope 为本阶段条件；executor 为原读取执行器。
    /// # 返回
    /// 已经过滤的完整命令结果。
    /// # 错误
    /// 无权、对象缺失、动作不属于订单或读取失败时拒绝。
    async fn view(
        &self,
        order_id: &str,
        target: &ActionResult,
        scope: &Self::Scope,
        executor: &mut dyn Executor,
    ) -> Result<Self::View>;

    /// 在全部关联读取之后完成本阶段交付检查。
    /// # 参数
    /// scope 为原查询条件，executor 为原读取执行器。
    /// # 返回
    /// 原边界和期限仍有效时成功。
    /// # 错误
    /// 跨阶段、到期或授权依赖变化时拒绝交付。
    async fn validate(&self, scope: &Self::Scope, executor: &mut dyn Executor) -> Result<()>;
}

#[async_trait]
impl ActionResultRead for MongoActionResultRead {
    type Scope = CompiledScope;
    type View = SubmitActionResultView;

    async fn authorize(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<Self::Scope> {
        Ok(self.access.resolve(actor, permission, executor).await?)
    }

    async fn view(
        &self,
        order_id: &str,
        target: &ActionResult,
        scope: &Self::Scope,
        executor: &mut dyn Executor,
    ) -> Result<Self::View> {
        let order = self
            .db
            .supplier_fulfillment_orders()
            .find_authorized(order_id, scope, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原订单不存在或无权读取动作结果".into()))?;
        let action_id = self.action_id(order_id, target, executor).await?;
        let action = self
            .db
            .supplier_order_actions()
            .find_by_id(&action_id, executor)
            .await?
            .filter(|action| action.supplier_fulfillment_order_id.as_ref() == order_id)
            .ok_or_else(|| Error::NotFound("原动作不存在或不属于当前订单".into()))?;
        let lines = match target {
            ActionResult::Refund(_) => {
                self.db
                    .supplier_order_action_lines()
                    .find_lines_by_action_ids(&[SupplierOrderActionId::new(action_id)], executor)
                    .await?
            },
            ActionResult::Cancellation => Vec::new(),
        };
        Ok(SubmitActionResultView {
            action: action.into(),
            order: order.into(),
            lines: lines.into_iter().map(action_line_view).collect(),
        })
    }

    async fn validate(&self, scope: &Self::Scope, executor: &mut dyn Executor) -> Result<()> {
        Ok(scope.validate(executor).await?)
    }
}

impl MongoActionResultRead {
    /// 在原读取快照定位取消动作；退款只接受原命令产生的精确动作身份。
    async fn action_id(
        &self,
        order_id: &str,
        target: &ActionResult,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        match target {
            ActionResult::Refund(id) => Ok(id.clone()),
            ActionResult::Cancellation => self
                .db
                .supplier_api_order_executions()
                .find_by_id(order_id, executor)
                .await?
                .and_then(|execution| execution.cancellation_action_id)
                .ok_or_else(|| Error::NotFound("原取消动作不存在".into())),
        }
    }
}

/// 原写事务结束后，在新快照中按原动作授权并复验完整结果。
pub(crate) async fn read_action_result(
    db: &Database,
    access: &FulfillmentOrderAccess,
    actor: &AuditActor,
    order_id: &str,
    target: ActionResult,
) -> Result<SubmitActionResultView> {
    let port = MongoActionResultRead { db: db.clone(), access: access.clone() };
    let actor = actor.clone();
    let order_id = order_id.to_owned();
    db.client()
        .with_transaction(move |executor| {
            Box::pin(async move { read_result(&port, &actor, &order_id, &target, executor).await })
        })
        .await
}

/// 原命令结束后重新授权完整回包；交付失败时丢弃已经读取的数据。
/// # 参数
/// port 为原命令结果读取设施；actor/order_id/target 为真实身份及目标；executor 为新读取快照。
/// # 返回
/// 原动作完整授权及交付检查均通过后的结果。
/// # 错误
/// 授权、读取或交付检查任一失败时返回原错误。
async fn read_result<P: ActionResultRead>(
    port: &P,
    actor: &AuditActor,
    order_id: &str,
    target: &ActionResult,
    executor: &mut dyn Executor,
) -> Result<P::View> {
    let scope = port.authorize(actor, &target.permission(), executor).await?;
    let view = port.view(order_id, target, &scope, executor).await?;
    port.validate(&scope, executor).await?;
    Ok(view)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_core::AccountKind;
    use persistence_core::testing::SnapshotExecutor;
    use persistence_core::{ExecutionBoundary, NoTransaction};

    use super::*;

    struct ReadPort {
        allowed: bool,
        expired: bool,
        trace: Mutex<Vec<&'static str>>,
        permissions: Mutex<Vec<Permission>>,
    }

    #[async_trait]
    impl ActionResultRead for ReadPort {
        type Scope = ExecutionBoundary;
        type View = u32;

        async fn authorize(
            &self,
            actor: &AuditActor,
            permission: &Permission,
            executor: &mut dyn Executor,
        ) -> Result<Self::Scope> {
            self.trace.lock().unwrap().push("authorize");
            self.permissions.lock().unwrap().push(permission.clone());
            assert_eq!(actor.id(), "operator");
            if !self.allowed {
                return Err(Error::Forbidden("原动作资格已撤销".into()));
            }
            executor
                .boundary()
                .filter(ExecutionBoundary::is_snapshot)
                .ok_or_else(|| Error::ValidationError("结果读取需要原快照".into()))
        }

        async fn view(
            &self,
            order_id: &str,
            _: &ActionResult,
            scope: &Self::Scope,
            executor: &mut dyn Executor,
        ) -> Result<Self::View> {
            self.trace.lock().unwrap().push("view");
            assert_eq!(order_id, "order");
            assert!(scope.is_current(executor));
            Ok(42)
        }

        async fn validate(&self, scope: &Self::Scope, executor: &mut dyn Executor) -> Result<()> {
            self.trace.lock().unwrap().push("validate");
            assert!(scope.is_current(executor));
            if self.expired {
                return Err(Error::ConflictError("DATA_SCOPE_CHANGED".into()));
            }
            Ok(())
        }
    }

    fn port(allowed: bool, expired: bool) -> ReadPort {
        ReadPort { allowed, expired, trace: Mutex::default(), permissions: Mutex::default() }
    }

    fn actor() -> AuditActor {
        AuditActor::new("operator".into(), "operator".into(), AccountKind::Admin)
    }

    #[tokio::test]
    async fn command_results_use_original_action_permission_and_validate_the_complete_read() {
        for (target, permission) in [
            (ActionResult::Refund("refund".into()), supplier_fulfillment::ORDER_REFUND),
            (ActionResult::Cancellation, supplier_fulfillment::ORDER_CANCEL),
        ] {
            let port = port(true, false);
            let view = read_result(&port, &actor(), "order", &target, &mut SnapshotExecutor::default()).await;
            assert_eq!(view.unwrap(), 42);
            assert_eq!(*port.trace.lock().unwrap(), ["authorize", "view", "validate"]);
            assert_eq!(*port.permissions.lock().unwrap(), [permission]);
        }
    }

    #[tokio::test]
    async fn revoked_command_qualification_stops_before_loading_the_result() {
        let port = port(false, false);
        let result = read_result(
            &port,
            &actor(),
            "order",
            &ActionResult::Cancellation,
            &mut SnapshotExecutor::default(),
        )
        .await;
        assert!(matches!(result, Err(Error::Forbidden(_))));
        assert_eq!(*port.trace.lock().unwrap(), ["authorize"]);
    }

    #[tokio::test]
    async fn expired_delivery_does_not_return_the_previously_loaded_result() {
        let port = port(true, true);
        let result = read_result(
            &port,
            &actor(),
            "order",
            &ActionResult::Refund("refund".into()),
            &mut SnapshotExecutor::default(),
        )
        .await;
        assert!(matches!(result, Err(Error::ConflictError(message)) if message == "DATA_SCOPE_CHANGED"));
        assert_eq!(*port.trace.lock().unwrap(), ["authorize", "view", "validate"]);
    }

    #[tokio::test]
    async fn result_reading_without_snapshot_stops_before_loading_data() {
        let port = port(true, false);
        let result =
            read_result(&port, &actor(), "order", &ActionResult::Cancellation, &mut NoTransaction).await;
        assert!(matches!(result, Err(Error::ValidationError(_))));
        assert_eq!(*port.trace.lock().unwrap(), ["authorize"]);
    }
}
