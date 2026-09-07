use erp_audit::AuditExt;
use erp_core::ids::SupplierOrderActionId;
use erp_integration::entity::integration_ops::InboxMessageStatus;
use erp_integration::repository::IntegrationOpsExt;
use erp_supply::entity::supplier_api::SupplierApiCapabilityCode;
use erp_supply::entity::supplier_fulfillment::SupplierOrderActionType;
use erp_supply::repository::{SupplierApiExt, SupplierFulfillmentExt};
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::place::build_action_message;
use super::SupplierFulfillmentProcess;
use crate::{Error, Result};
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_supply::dto::supplier_fulfillment::{SubmitActionResultView, SubmitAfterSalesActionRequest};
use erp_supply::service::supplier_fulfillment::mapping::action_line_view;
use erp_supply::service::supplier_fulfillment::place::ensure_capability;

impl SupplierFulfillmentProcess {
    /// 提交供应商取消（幂等键：「订单号 + CANCEL」，§6.19）。
    ///
    /// 同事务创建 `CANCEL` 动作头/行并把 `cancel_status` 推进到 `CANCEL_PENDING`；
    /// 事务外派发供应商 API。重复提交（同一幂等键）返回原动作结果，不再次调用。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    /// * `req` - 取消动作提交请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回动作与动作后订单视图。
    ///
    /// # 错误
    /// * `NotFound` - 订单不存在
    /// * `BusinessLogicError` - 动作范围非法或连接缺少取消能力
    /// * `ConflictError` - 唯一键冲突（并发重复提交）
    pub async fn submit_cancel(
        &self,
        id: &str,
        req: SubmitAfterSalesActionRequest,
        actor: &AuditActor,
    ) -> Result<SubmitActionResultView> {
        self.submit_after_sales_action(id, req, SupplierOrderActionType::Cancel, actor)
            .await
    }

    /// 提交供应商取消/退款动作的公共编排。
    ///
    /// 完成幂等命中、售后申请存在性、连接能力、动作行净余额校验后，同事务写入
    /// 动作头/行与订单状态推进，事务外派发供应商 API。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    /// * `req` - 动作提交请求
    /// * `action_type` - 动作类型（`Cancel` 或 `Refund`）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回动作与动作后订单视图。
    ///
    /// # 错误
    /// 同 [`Self::submit_cancel`]。
    pub(super) async fn submit_after_sales_action(
        &self,
        id: &str,
        req: SubmitAfterSalesActionRequest,
        action_type: SupplierOrderActionType,
        actor: &AuditActor,
    ) -> Result<SubmitActionResultView> {
        req.validate()?;
        let mut order = self.load_order(id).await?;
        let idempotency_key = format!("{}+{}", order.fulfillment_order_no, action_type.as_str(),);
        if let Some(existing) = self
            .db
            .supplier_order_actions()
            .find_by_idempotency_key(&idempotency_key, &mut NoTransaction)
            .await?
        {
            tracing::info!(account = %actor.id(), order_id = %id, action_type = %action_type.as_str(), "售后动作幂等命中");
            let lines = self
                .db
                .supplier_order_action_lines()
                .find_lines_by_action_ids(
                    &[SupplierOrderActionId::new(existing.base.id.as_str())],
                    &mut NoTransaction,
                )
                .await?;
            return Ok(SubmitActionResultView {
                action: existing.into(),
                lines: lines.into_iter().map(action_line_view).collect(),
                order: order.into(),
            });
        }
        let connection = self
            .db
            .supplier_api_connections()
            .find_by_id(&order.connection_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商连接不存在".to_string()))?;
        let capabilities = self
            .db
            .supplier_api_capabilities()
            .find_capabilities_by_connection(&order.connection_id, &mut NoTransaction)
            .await?;
        let needed = match action_type {
            SupplierOrderActionType::Cancel => SupplierApiCapabilityCode::Cancel,
            SupplierOrderActionType::Refund => SupplierApiCapabilityCode::Refund,
            _ => return Err(Error::BusinessLogicError("不支持的售后动作类型".to_string())),
        };
        ensure_capability(&capabilities, needed)?;
        self.domain().ensure_action_lines(&order, &req).await?;
        let mut action =
            self.domain()
                .build_after_sales_action(&order, &req, &idempotency_key, action_type)?;
        let lines = self.domain().build_action_lines(&action, &req)?;
        self.domain().advance_after_sales(&mut order, action_type)?;
        let mut message = build_action_message(&action, &connection, InboxMessageStatus::Received)?;
        let audit = actor.clone().resource_log(
            "supplier_fulfillment.after_sales_action",
            "supplier_order_action",
            action.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let mut order_for_tx = order.clone();
        let action_for_tx = action.clone();
        let lines_for_tx = lines.clone();
        let message_for_tx = message.clone();
        let audit_for_tx = audit.clone();
        super::execution::after_intent(client
            .with_transaction(move |session| {
                Box::pin(async move {
                    erp_supply::service::supplier_fulfillment::cancel::persist_after_sales(
                        &db,
                        &action_for_tx,
                        &lines_for_tx,
                        &mut order_for_tx,
                        session,
                    )
                    .await?;
                    db.inbox_messages().create(&message_for_tx, session).await?;
                    db.audit_logs().create(&audit_for_tx, session).await?;
                    Ok::<(), crate::Error>(())
                })
            }), || async {
        tracing::info!(account = %actor.id(), action_id = %action.base.id, "售后动作事务已提交，开始事务外供应商派发");
        self.settle_dispatch(&mut order, &mut action, &mut message, &connection, actor)
            .await
        }).await?;
        Ok(SubmitActionResultView {
            action: action.into(),
            lines: lines.into_iter().map(action_line_view).collect(),
            order: order.into(),
        })
    }
}
