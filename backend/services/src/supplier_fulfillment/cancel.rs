use database::{
    AccessControlExt, IntegrationOpsExt, NoTransaction, SupplierApiExt, SupplierFulfillmentExt, Transactional,
};
use entities::ids::{SupplierOrderActionId, SupplierOrderActionLineId};
use entities::integration_ops::InboxMessageStatus;
use entities::supplier_api::SupplierApiCapabilityCode;
use entities::supplier_fulfillment::{
    CancelStatus, RefundStatus, SupplierFulfillmentOrder, SupplierFulfillmentOrderId, SupplierOrderAction,
    SupplierOrderActionData, SupplierOrderActionLine, SupplierOrderActionLineData, SupplierOrderActionType,
};
use id_generator::next_id;
use validator::Validate;

use super::dto::{SubmitActionResultView, SubmitAfterSalesActionRequest};
use super::mapping::action_line_view;
use super::place::{build_action_message, ensure_capability};
use super::SupplierFulfillmentService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl SupplierFulfillmentService {
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
        self.ensure_action_lines(&order, &req).await?;
        let mut action = self.build_after_sales_action(&order, &req, &idempotency_key, action_type)?;
        let lines = self.build_action_lines(&action, &req)?;
        match action_type {
            SupplierOrderActionType::Cancel => order.advance_cancel(CancelStatus::CancelPending)?,
            SupplierOrderActionType::Refund => order.advance_refund(RefundStatus::RefundPending)?,
            _ => {}
        }
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
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_order_actions()
                        .create(&action_for_tx, session)
                        .await?;
                    for line in &lines_for_tx {
                        db.supplier_order_action_lines().create(line, session).await?;
                    }
                    db.supplier_fulfillment_orders()
                        .update(&mut order_for_tx, session)
                        .await?;
                    db.inbox_messages().create(&message_for_tx, session).await?;
                    db.audit_logs().create(&audit_for_tx, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        tracing::info!(account = %actor.id(), action_id = %action.base.id, "售后动作事务已提交，开始事务外供应商派发");
        self.settle_dispatch(&mut order, &mut action, &mut message, &connection, actor)
            .await?;
        Ok(SubmitActionResultView {
            action: action.into(),
            lines: lines.into_iter().map(action_line_view).collect(),
            order: order.into(),
        })
    }

    /// 构建取消/退款动作头。
    ///
    /// # 参数
    /// * `order` - 供应商子订单
    /// * `req` - 动作提交请求
    /// * `idempotency_key` - 「订单号 + 动作类型」
    /// * `action_type` - `Cancel` 或 `Refund`
    ///
    /// # 返回
    /// 返回动作实体。
    ///
    /// # 错误
    /// 实体构造校验失败时返回 `LogicError`。
    fn build_after_sales_action(
        &self,
        order: &SupplierFulfillmentOrder,
        req: &SubmitAfterSalesActionRequest,
        idempotency_key: &str,
        action_type: SupplierOrderActionType,
    ) -> Result<SupplierOrderAction> {
        SupplierOrderAction::new(
            SupplierOrderActionId::new(next_id()),
            SupplierOrderActionData::manual_adjustment(
                SupplierFulfillmentOrderId::new(order.base.id.as_str()),
                action_type,
                idempotency_key,
                req.reason_code.as_deref(),
            ),
        )
        .map_err(Into::into)
    }

    /// 构建取消/退款动作行（行号从 1 起，冻结实际提交范围）。
    ///
    /// # 参数
    /// * `action` - 动作头
    /// * `req` - 动作提交请求
    ///
    /// # 返回
    /// 返回动作行集合。
    ///
    /// # 错误
    /// 数量/金额非正的实体校验失败时返回 `LogicError`。
    fn build_action_lines(
        &self,
        action: &SupplierOrderAction,
        req: &SubmitAfterSalesActionRequest,
    ) -> Result<Vec<SupplierOrderActionLine>> {
        req.lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                SupplierOrderActionLine::new(
                    SupplierOrderActionLineId::new(next_id()),
                    SupplierOrderActionLineData::from_request_index(
                        SupplierOrderActionId::new(action.base.id.as_str()),
                        index,
                        line.supplier_fulfillment_item_id.clone(),
                        line.quantity,
                        line.amount,
                    ),
                )
            })
            .collect::<std::result::Result<Vec<_>, entities::Error>>()
            .map_err(crate::errors::Error::from)
    }

    /// 校验动作行范围（§6.19）：行明细必须属于该子订单。
    ///
    /// # 参数
    /// * `order` - 供应商子订单
    /// * `req` - 动作提交请求
    ///
    /// # 错误
    /// * `BusinessLogicError` - 明细归属非法
    async fn ensure_action_lines(
        &self,
        order: &SupplierFulfillmentOrder,
        req: &SubmitAfterSalesActionRequest,
    ) -> Result<()> {
        let order_id = SupplierFulfillmentOrderId::new(order.base.id.as_str());
        let items = self
            .db
            .supplier_fulfillment_items()
            .find_items_by_order_ids(std::slice::from_ref(&order_id), &mut NoTransaction)
            .await?;
        let item_ids: std::collections::HashSet<&str> =
            items.iter().map(|item| item.base.id.as_ref()).collect();
        for line in &req.lines {
            if !item_ids.contains(line.supplier_fulfillment_item_id.as_ref()) {
                return Err(Error::BusinessLogicError(
                    "动作行不属于该供应商子订单".to_string(),
                ));
            }
        }
        Ok(())
    }
}
