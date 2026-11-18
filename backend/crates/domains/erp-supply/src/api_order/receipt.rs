//! 实际收货通知的单次动作，与创建、预占确认及 ERP 交付状态分别持久化。

use std::collections::BTreeMap;

use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};

use super::ApiOrderExecution;
use crate::ports::connector::order::OrderReference;
use crate::ports::connector::order_tree::OrderNodeRole;
use crate::ports::connector::receipt::{ConfirmReceipt, ReceiptConfirmationState, ReceiptLine};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReceiptActionState {
    SentUnknown,
    Confirmed,
    NotRequired,
}

/// 没有发送请求时仅记录供应商已经达到的结果，不因此构造 ERP 收货事实。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptAction {
    pub order: OrderReference,
    pub topology_version: u64,
    pub request: Option<ConfirmReceipt>,
    pub state: ReceiptActionState,
}

impl ApiOrderExecution {
    /// 取得尚无原收货动作的当前真实叶子，原目标不能因重启或拆单重复登记。
    /// 返回下一真实叶子及拓扑版本；所有叶子已有动作时为空。
    /// 当前完整订单关系尚未取得时拒绝。
    /// 调用方须先以当前完整拓扑核验实际收到数量。
    pub fn next_receipt_target(&self) -> Result<Option<(OrderReference, u64)>> {
        let tree = self
            .order_tree
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("原供应商子单尚未核实，不能通知收货".into()))?;
        Ok(tree
            .leaves()
            .into_iter()
            .find(|node| !self.receipt_actions.contains_key(&node.external_order_no))
            .map(|node| {
                (
                    OrderReference {
                        merchant_order_no: self.base.id.clone(),
                        external_order_no: node.external_order_no.clone(),
                    },
                    tree.version,
                )
            }))
    }

    /// 唤醒独立收货核验，原创建和确认步骤保持不变。
    /// 持久调度时间及原收货阻断被刷新。
    /// 真实数量须在发送前重新核验。
    /// `now` 来自实际验收/入库根事务或已授权的原结果查询。
    pub fn wake_receipt_confirmation(&mut self, now: Instant) {
        self.receipt_next_run_at = Some(now);
        self.receipt_blocker = None;
    }

    /// 核对本执行的每条冻结来源行均已经真实收到全部数量。
    /// 完整覆盖成功；不将部分实收按 SKU 分配给任何子单。
    /// 缺行、重复、未知来源行、非正或数量不等于原批准数量时拒绝。
    pub fn ensure_receipt_coverage(&self, received: &[ReceiptLine]) -> Result<()> {
        let by_id = received.iter().map(|line| (line.line_id.as_str(), line)).collect::<BTreeMap<_, _>>();
        if received.is_empty()
            || by_id.len() != received.len()
            || by_id.len() != self.source_lines.len()
            || self.source_lines.iter().any(|source| {
                by_id.get(source.fulfillment_item_id.as_str()).is_none_or(|line| {
                    line.quantity != source.quantity
                        || line.quantity.to_decimal().is_sign_negative()
                        || line.quantity.to_decimal().is_zero()
                })
            })
        {
            return Err(Error::BusinessLogicError(
                "实际收到数量尚未完整对应本执行全部冻结行，请核对原验收或入库记录".into(),
            ));
        }
        Ok(())
    }

    /// 在事务内保存原叶子的未知发送边界，同一目标永不重复发送。
    /// 首次保存返回真；完全相同的原请求重放返回假。
    /// 目标不是当前叶子、拓扑或数量变更、证据缺失或同目标异载荷时拒绝。
    pub fn begin_receipt_confirmation(&mut self, request: &ConfirmReceipt) -> Result<bool> {
        if let Some(previous) = self.receipt_actions.get(&request.order.external_order_no) {
            return if previous.request.as_ref() == Some(request) {
                Ok(false)
            } else {
                Err(Error::ConflictError("原收货通知已登记，请查询原结果，不能改换目标或重复发送".into()))
            };
        }

        let tree = self
            .order_tree
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("原供应商叶子尚未核实".into()))?;
        let node = tree
            .nodes
            .iter()
            .find(|node| node.external_order_no == request.order.external_order_no)
            .filter(|node| node.role == OrderNodeRole::Leaf)
            .ok_or_else(|| Error::BusinessLogicError("收货通知只能指向已核实的当前物流子单".into()))?;

        let expected = node
            .lines
            .iter()
            .map(|line| ReceiptLine {
                line_id: line.line_id.clone(),
                sku: line.sku.clone(),
                quantity: line.quantity,
            })
            .collect::<Vec<_>>();
        if request.order.merchant_order_no != self.base.id
            || request.topology_version != tree.version
            || request.lines != expected
            || request.evidence_fingerprint.len() != 64
            || request.action.id.trim().is_empty()
            || request.action.payload_hash.trim().is_empty()
        {
            return Err(Error::ConflictError("收货通知的原单、叶子数量、拓扑或实际收到依据不一致".into()));
        }

        self.receipt_actions.insert(
            request.order.external_order_no.clone(),
            ReceiptAction {
                order: request.order.clone(),
                topology_version: request.topology_version,
                request: Some(request.clone()),
                state: ReceiptActionState::SentUnknown,
            },
        );
        Ok(true)
    }

    /// 只应用供应商原目标的明确收到结果；未查明不授权重发。
    /// 明确结果永久保留；不会改变 ERP 发货、入库及客户验收数量。
    /// 原目标身份冲突或把已发送未知误判为无需通知时拒绝。
    /// `order`、`version` 为原目标；`state` 来自原单只读查询或一次写响应。
    pub fn observe_receipt_confirmation(
        &mut self,
        order: &OrderReference,
        version: u64,
        state: ReceiptConfirmationState,
    ) -> Result<()> {
        if state == ReceiptConfirmationState::Unverified {
            return Ok(());
        }

        let next = if state == ReceiptConfirmationState::Confirmed {
            ReceiptActionState::Confirmed
        } else {
            ReceiptActionState::NotRequired
        };
        if let Some(action) = self.receipt_actions.get_mut(&order.external_order_no) {
            if action.order != *order
                || (action.request.is_some() && next == ReceiptActionState::NotRequired)
                || (action.state != ReceiptActionState::SentUnknown && action.state != next)
            {
                return Err(Error::ConflictError("原收货通知结果或协议适用性发生冲突，请核实原目标".into()));
            }
            action.state = next;
        } else {
            self.receipt_actions.insert(
                order.external_order_no.clone(),
                ReceiptAction { order: order.clone(), topology_version: version, request: None, state: next },
            );
        }
        Ok(())
    }
}
