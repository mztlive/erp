//! 复用原履约入口的 API 子单取消，事务内登记意图、事务外一次发送。

mod dispatch;
mod intent;
mod persistence;
mod read;

use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderStage, CancellationActionState, CancellationScope};
use erp_supply::dto::supplier_fulfillment::{SubmitActionResultView, SubmitAfterSalesActionRequest};
use erp_supply::ports::connector::SupplierConnector;
use erp_supply::ports::connector::common::Lookup;
use erp_supply::ports::connector::order::{OrderKey, OrderLine, OrderSnapshot};
use erp_supply::repository::SupplierFulfillmentExt;
use erp_supply::service::supplier_api::SupplierApiService;
use erp_supply::service::supplier_fulfillment::FulfillmentOrderAccess;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::{SupplierApiOrderProcess, missing_capability};
use crate::supplier_after_sales::action_result::{ActionResult, MongoActionResultRead, read_result};
use crate::{Error, Result};

impl SupplierApiOrderProcess {
    /// 复用原取消权限和订单范围，为 API 执行建立逐叶取消意图。
    /// # 参数
    /// `id` 为原履约订单，`request` 为完整冻结商品范围，`actor` 和 `access` 为现有权限入口。
    /// # 返回
    /// API 订单返回已有动作响应；非 API 订单返回空，由原流程处理。
    /// # 错误
    /// 越权、范围变更、能力缺失、未知确认或持久化冲突时拒绝；不发送外部请求。
    pub async fn submit_cancel_if_api(
        &self,
        id: &str,
        request: SubmitAfterSalesActionRequest,
        actor: &AuditActor,
        access: &FulfillmentOrderAccess,
    ) -> Result<Option<SubmitActionResultView>> {
        let db = self.db.clone();
        let port = read::MongoCancellationRead { db: db.clone(), access: access.clone() };
        let (read_id, read_actor) = (id.to_owned(), actor.clone());
        let execution = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { read::authorized_execution(&port, &read_actor, &read_id, executor).await },
                )
            })
            .await?;
        let Some(mut execution) = execution else {
            return Ok(None);
        };
        request.validate()?;
        let frozen = self.decode(&execution)?;
        let scope = CancellationScope::new(&request, &frozen.lines)?;
        let canonical = scope.canonical_request(&request, &frozen.lines)?;
        let action_id = execution
            .cancellation_action_id
            .clone()
            .unwrap_or_else(|| format!("api-cancel-{}", execution.base.id));
        if execution.prepare_cancellations(&scope, &action_id)? {
            self.cancellation_connector(&execution).await?;
            self.persist_cancel_intent(execution, &canonical, actor, access).await?;
        }
        self.cancellation_view(id, actor, access).await.map(Some)
    }

    /// 调度一组原取消动作，每次至多发送一个尚未发送的叶子。
    /// # 参数
    /// `execution` 为已领取的原执行，取消不受新增下单或余额授权开关限制。
    /// # 返回
    /// 查询和动作状态可靠保存；受理后只调查原结果，不重复发送。
    /// # 错误
    /// 数据库失败或持久化竞争时拒绝；供应商异常按原动作保存。
    pub(super) async fn process_cancellations(&self, mut execution: ApiOrderExecution) -> Result<()> {
        if let Err(error) = self.refresh_cancellation_tree(&mut execution).await {
            execution.defer(error.to_string());
            return self.persist_cancellation_state(&mut execution).await;
        }
        let next = execution
            .cancellation_actions
            .iter()
            .find(|(_, action)| action.state == CancellationActionState::Ready)
            .map(|(target, _)| target.clone());
        let Some(target) = next else {
            execution.refresh_cancellation_stage();
            execution.next_run_at = Instant::from_unix_secs(Instant::now().unix_secs().saturating_add(60));
            return self.persist_cancellation_state(&mut execution).await;
        };
        let connector = match self.cancellation_connector(&execution).await {
            Ok(connector) => connector,
            Err(error) => {
                execution.defer(error.to_string());
                return self.persist_cancellation_state(&mut execution).await;
            },
        };
        dispatch::send_once(
            &dispatch::MongoCancellationDispatch { process: self, connector },
            &mut execution,
            &target,
        )
        .await?;
        Ok(())
    }

    async fn cancellation_connector(
        &self,
        execution: &ApiOrderExecution,
    ) -> Result<Arc<dyn SupplierConnector>> {
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(&execution.connection_id, &mut NoTransaction)
            .await?;
        if connection.supplier_id.as_ref() != execution.supplier_id {
            return Err(Error::ConflictError("取消连接不属于原供应商".into()));
        }
        let connector = self
            .runtime
            .fulfillment_connector(&connection)
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        if connector.cancellations().is_none() {
            return Err(missing_capability("原子单取消"));
        }
        Ok(connector)
    }

    async fn refresh_cancellation_tree(&self, execution: &mut ApiOrderExecution) -> Result<()> {
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(&execution.connection_id, &mut NoTransaction)
            .await?;
        let frozen = self.decode(execution)?;
        let connector = self
            .runtime
            .fulfillment_connector(&connection)
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        let observed = connector
            .orders()
            .ok_or_else(|| missing_capability("原取消订单查询"))?
            .cancellation_order(&OrderKey::Merchant(execution.base.id.clone()), &frozen.lines)
            .await
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        let Lookup::Found(observed) = observed else {
            return Err(Error::BusinessLogicError("原取消订单暂不可见，继续查询原结果".into()));
        };
        self.apply_cancellation_tree(execution, observed.value, &frozen.lines).await
    }

    async fn apply_cancellation_tree(
        &self,
        execution: &mut ApiOrderExecution,
        observed: OrderSnapshot,
        frozen: &[OrderLine],
    ) -> Result<()> {
        let tree = observed
            .topology
            .ok_or_else(|| Error::BusinessLogicError("原取消订单当前子单尚未完整核实".into()))?;
        execution.pending_order_tree = Some(tree.clone());
        let original = self
            .db
            .supplier_fulfillment_orders()
            .find_by_id(&execution.base.id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("原供应商履约订单不存在".into()))?;
        if original.external_order_no.as_deref() != Some(tree.root_order_no.as_str())
            || observed.reference.merchant_order_no != execution.base.id
            || observed.reference.external_order_no != tree.root_order_no
        {
            return Err(Error::ConflictError("取消结果与原始供应商根单不一致".into()));
        }
        execution.apply_observed_tree(tree, frozen)?;
        execution.observe_cancellations()?;
        let tree = execution
            .order_tree
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("原取消范围未核实".into()))?;
        if tree
            .action_targets()
            .iter()
            .any(|leaf| !execution.cancellation_actions.contains_key(&leaf.external_order_no))
        {
            execution.stage = ApiOrderStage::CancellationUnknown;
            return Err(Error::BusinessLogicError(
                "原取消范围发生再次拆单，请先核实全部新子单；系统不会重复发送取消".into(),
            ));
        }
        Ok(())
    }

    /// 首次登记与幂等回放都以当前取消资格在新快照中读取完整结果。
    async fn cancellation_view(
        &self,
        id: &str,
        actor: &AuditActor,
        access: &FulfillmentOrderAccess,
    ) -> Result<SubmitActionResultView> {
        let port = MongoActionResultRead { db: self.db.clone(), access: access.clone() };
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    read_result(&port, &actor, &id, &ActionResult::Cancellation, executor).await
                })
            })
            .await
    }
}
