//! 售后动作的本域明细归属、实体构造与同执行器写入。
use erp_core::ids::{SupplierOrderActionId, SupplierOrderActionLineId};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use super::SupplierFulfillmentService;
use crate::dto::supplier_fulfillment::*;
use crate::entity::supplier_fulfillment::*;
use crate::repository::SupplierFulfillmentExt;
use crate::{Error, Result};
impl SupplierFulfillmentService {
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
    pub fn build_after_sales_action(
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
    pub fn build_action_lines(
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
            .collect::<std::result::Result<Vec<_>, erp_core::Error>>()
            .map_err(crate::Error::from)
    }

    /// 校验动作行范围（§6.19）：行明细必须属于该子订单。
    ///
    /// # 参数
    /// * `order` - 供应商子订单
    /// * `req` - 动作提交请求
    ///
    /// # 错误
    /// * `BusinessLogicError` - 明细归属非法
    pub async fn ensure_action_lines(
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
                return Err(Error::BusinessLogicError("动作行不属于该供应商子订单".to_string()));
            }
        }
        Ok(())
    }
    /// 沿原类型推进取消或退款状态；不附加旧代码不存在的版本/余额校验。
    pub fn advance_after_sales(
        &self,
        order: &mut SupplierFulfillmentOrder,
        action_type: SupplierOrderActionType,
    ) -> Result<()> {
        match action_type {
            SupplierOrderActionType::Cancel => order.advance_cancel(CancelStatus::CancelPending)?,
            SupplierOrderActionType::Refund => order.advance_refund(RefundStatus::RefundPending)?,
            _ => {},
        }
        Ok(())
    }
}
/// 动作头、逐行写入、订单CAS复用调用方执行器。
pub async fn persist_after_sales(
    db: &Database,
    action: &SupplierOrderAction,
    lines: &[SupplierOrderActionLine],
    order: &mut SupplierFulfillmentOrder,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.supplier_order_actions().create(action, executor).await?;
    for line in lines {
        db.supplier_order_action_lines().create(line, executor).await?;
    }
    db.supplier_fulfillment_orders().update(order, executor).await?;
    Ok(())
}
