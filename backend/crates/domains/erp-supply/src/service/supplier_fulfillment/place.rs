//! 下单事实、能力与派发后本域状态；不触及消息或工作项。
use std::collections::HashMap;

use erp_core::common::time::Instant;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use super::SupplierFulfillmentService;
use crate::dto::supplier_fulfillment::*;
use crate::entity::failure::SupplierFailureClass;
use crate::entity::supplier_api::{SupplierApiCapabilityCode, SupplierApiConnection};
use crate::entity::supplier_fulfillment::*;
use crate::ports::supplier_gateway::DispatchOutcome;
use crate::repository::{SupplierApiExt, SupplierFulfillmentExt};
use crate::{Error, Result};
/// 本域状态更新后需要的集成消息结果，错误政策由调用方给出。
pub enum DispatchMessageResult {
    Processed,
    Failed(SupplierFailureClass),
}
impl SupplierFulfillmentService {
    /// 核对下单前置条件并返回连接与供给。
    /// 连接必须存在、启用且属于请求供应商，并声明 `order` 能力；请求中的供给修订必须都能加载，且属于该供
    /// 应商和连接。
    /// 返回已校验的供应商连接，以及按修订 ID 索引的供给。
    /// 失败条件：
    /// * `NotFound` - 连接不存在，或供给修订不能全部加载
    /// * `BusinessLogicError` - 连接未启用、不属于下单供应商、缺少下单能力，或供给不属于该供应商或连接
    /// 仓储读取失败时向上传播错误。
    /// `executor`：创建用例的原事务执行器。
    pub async fn ensure_placeable(
        &self,
        req: &PlaceFulfillmentOrderRequest,
        executor: &mut dyn Executor,
    ) -> Result<(SupplierApiConnection, HashMap<String, crate::entity::supplier_offering::SupplierOffering>)>
    {
        let connection = self
            .db
            .supplier_api_connections()
            .find_by_id(&req.connection_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商连接不存在".to_string()))?;
        if !connection.is_active() {
            return Err(Error::BusinessLogicError("供应商连接未启用".to_string()));
        }
        if connection.supplier_id != req.supplier_id {
            return Err(Error::BusinessLogicError("供应商连接不属于下单供应商".to_string()));
        }
        let capabilities = self.capabilities(&connection)?;
        ensure_capability(&capabilities, SupplierApiCapabilityCode::Order)?;

        let mut revision_ids =
            req.items.iter().map(|item| item.supplier_offering_revision_id.clone()).collect::<Vec<_>>();
        revision_ids.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
        revision_ids.dedup_by(|left, right| left.as_ref() == right.as_ref());
        let by_revision =
            self.db.supplier_fulfillment().load_offerings_by_revision_ids(&revision_ids, executor).await?;
        if by_revision.len() != revision_ids.len() {
            return Err(Error::NotFound("供应商供给修订或供给不存在".to_string()));
        }
        for offering in by_revision.values() {
            if !offering.belongs_to_ordering_source(&req.supplier_id, &req.connection_id) {
                return Err(Error::BusinessLogicError("供给不属于下单供应商或供应商连接不匹配".to_string()));
            }
        }
        Ok((connection, by_revision))
    }

    /// 构建下单事实（子订单 + 明细 + 首个 `PLACE` 动作）。
    /// 明细含税成本快照由单位成本 × 数量按分舍入派生（§4.2 铁律 1）；`PLACE` 动作幂等键为 ERP 供应商子订
    /// 单号（§6.19）。
    /// 返回 `(子订单, 明细, 动作)` 三元组。
    /// 跟进人校验失败、明细供给缺失，或实体构造校验失败时返回对应错误。
    pub fn build_place_facts(
        &self,
        req: &PlaceFulfillmentOrderRequest,
        offerings: &HashMap<String, crate::entity::supplier_offering::SupplierOffering>,
        follow_up_user_id: String,
        business_org_unit_id: String,
    ) -> Result<(SupplierFulfillmentOrder, Vec<SupplierFulfillmentItem>, SupplierOrderAction)> {
        let order_id = SupplierFulfillmentOrderId::new(next_id());
        let order = SupplierFulfillmentOrder::new(
            order_id.clone(),
            SupplierFulfillmentOrderData::submitting(
                req.fulfillment_order_no.clone(),
                req.supplier_id.clone(),
                req.connection_id.clone(),
                req.split_no,
                Instant::now(),
                req.address_snapshot_encrypted.clone(),
                req.address_snapshot_fingerprint.clone(),
            )
            .with_follow_up(follow_up_user_id, business_org_unit_id),
        )?;
        order.ensure_follow_up().map_err(Error::from)?;

        let items = self.build_place_items(&order_id, req, offerings)?;
        let action = SupplierOrderAction::new(
            SupplierOrderActionId::new(next_id()),
            SupplierOrderActionData::place(order_id, req.fulfillment_order_no.clone(), items.len()),
        )?;
        Ok((order, items, action))
    }

    /// 构建下单明细（含税成本快照派生）。
    /// 返回明细集合。
    /// 修订缺少供给时返回 `NotFound`；数量或成本快照恒等校验失败时返回对应错误。
    pub fn build_place_items(
        &self,
        order_id: &SupplierFulfillmentOrderId,
        req: &PlaceFulfillmentOrderRequest,
        offerings: &HashMap<String, crate::entity::supplier_offering::SupplierOffering>,
    ) -> Result<Vec<SupplierFulfillmentItem>> {
        req.items
            .iter()
            .map(|item| {
                let offering = offerings
                    .get(item.supplier_offering_revision_id.as_ref())
                    .ok_or_else(|| Error::NotFound("供应商供给不存在".to_string()))?;
                let data = SupplierFulfillmentItemData::from_unit_cost(
                    order_id.clone(),
                    item.supplier_offering_revision_id.clone(),
                    offering.supplier_sku_code.clone(),
                    offering.supplier_product_code.clone(),
                    item.quantity,
                    item.unit_cost_snapshot_gross,
                    item.input_tax_rate,
                )?;
                SupplierFulfillmentItem::new(SupplierFulfillmentItemId::new(next_id()), data)
                    .map_err(Error::from)
            })
            .collect()
    }
    /// 按派发结果就地更新订单和动作（不落库），并返回集成消息结果。
    /// 明确接单或拒单返回 `DispatchMessageResult::Processed`。结果未知或失败返回
    /// `DispatchMessageResult::Failed`，并带上失败类别。
    /// 订单或动作更新失败时返回对应错误。
    /// `can_auto_retry`：为假时把动作标为失败，下单动作同时把订单标为例外。
    pub fn apply_dispatch_outcome(
        order: &mut SupplierFulfillmentOrder,
        action: &mut SupplierOrderAction,
        outcome: DispatchOutcome,
        can_auto_retry: bool,
    ) -> Result<DispatchMessageResult> {
        match outcome {
            DispatchOutcome::Succeeded { external_request_id, external_order_no } => {
                if action.action_type == SupplierOrderActionType::Place {
                    if let Some(order_no) = external_order_no {
                        order.update(SupplierFulfillmentOrderUpdate { external_order_no: Some(order_no) })?;
                    }
                    order.advance_fulfillment(FulfillmentStatus::Accepted)?;
                }
                action.update(SupplierOrderActionUpdate {
                    status: Some(SupplierOrderActionStatus::Succeeded),
                    external_request_id: Some(external_request_id),
                    response_summary: Some("供应商已接单（模拟网关）".to_string()),
                    ..Default::default()
                })?;
                Ok(DispatchMessageResult::Processed)
            },
            DispatchOutcome::Rejected { summary } => {
                action.update(SupplierOrderActionUpdate {
                    status: Some(SupplierOrderActionStatus::Failed),
                    response_summary: Some(summary),
                    ..Default::default()
                })?;
                if action.action_type == SupplierOrderActionType::Place {
                    order.advance_fulfillment(FulfillmentStatus::Rejected)?;
                }
                Ok(DispatchMessageResult::Processed)
            },
            DispatchOutcome::ResultUnknown { summary } => {
                action.update(SupplierOrderActionUpdate {
                    status: Some(SupplierOrderActionStatus::ResultUnknown),
                    response_summary: Some(summary),
                    ..Default::default()
                })?;
                if action.action_type == SupplierOrderActionType::Place {
                    order.advance_fulfillment(FulfillmentStatus::ResultUnknown)?;
                }
                Ok(DispatchMessageResult::Failed(SupplierFailureClass::ResultUnknown))
            },
            DispatchOutcome::Failed { error_class, summary } => {
                if can_auto_retry {
                    action.record_attempt(Some(Instant::now()));
                } else {
                    action.update(SupplierOrderActionUpdate {
                        status: Some(SupplierOrderActionStatus::Failed),
                        response_summary: Some(summary),
                        ..Default::default()
                    })?;
                    if action.action_type == SupplierOrderActionType::Place {
                        order.advance_fulfillment(FulfillmentStatus::Exception)?;
                    }
                }
                Ok(DispatchMessageResult::Failed(error_class))
            },
        }
    }
}
/// 校验连接能力声明包含指定能力且为启用态（D25 跨域读取判定）。
/// 所需能力存在且为启用态时无返回值。
/// 能力缺失或未启用时返回 `BusinessLogicError`。
pub fn ensure_capability(
    capabilities: &[SupplierApiCapabilityCode],
    needed: SupplierApiCapabilityCode,
) -> Result<()> {
    let supported = capabilities.contains(&needed);
    if !supported {
        return Err(Error::BusinessLogicError(format!("供应商连接缺少能力: {}", needed.as_str())));
    }
    Ok(())
}
/// 在原执行器写入子订单、全部明细和首个动作。
/// 子订单、明细和动作已写入。
/// 仓储写入失败时向上传播错误。
pub async fn persist_place_facts(
    db: &Database,
    order: &SupplierFulfillmentOrder,
    items: &[SupplierFulfillmentItem],
    action: &SupplierOrderAction,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.supplier_fulfillment()
        .create_fulfillment_with_items_and_place_action(order, items, action, executor)
        .await?;
    Ok(())
}
/// 按原顺序保存订单和动作CAS，保持调用方事务。
/// 订单先于动作写回。
/// 任一步仓储更新失败时返回对应错误。
/// `order`：待按 CAS 写回的子订单。
/// `action`：待按 CAS 写回的动作。
pub async fn persist_dispatch_entities(
    db: &Database,
    order: &mut SupplierFulfillmentOrder,
    action: &mut SupplierOrderAction,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.supplier_fulfillment_orders().update(order, executor).await?;
    db.supplier_order_actions().update(action, executor).await?;
    Ok(())
}
