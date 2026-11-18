//! 独立确认的持久化发送边界；未知动作只调查原目标。

use serde::{Deserialize, Serialize};

use super::{ApiOrderExecution, ApiOrderStage};
use crate::ports::connector::order::{ConfirmOrder, ConfirmationState, OrderLine};
use crate::ports::connector::order_tree::{OrderNodeRole, OrderTree};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConfirmationActionState {
    SentUnknown,
    Confirmed,
    Rejected,
}

/// 原目标、拓扑版本与载荷永久保留，不在再次拆单后改写。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmationAction {
    pub request: ConfirmOrder,
    pub state: ConfirmationActionState,
}

impl ApiOrderExecution {
    /// 由服务端分配本地拓扑版本后应用完整观察。
    /// # 参数
    /// `incoming` 为供应商观察，外部携带的 version 不作为本地版本依据。
    /// # 返回
    /// 相同内容保留原版本；内容变化安全递增，再执行历史及数量核验。
    /// # 错误
    /// 版本溢出或关系校验失败时原有效拓扑不变。
    pub fn apply_observed_tree(&mut self, mut incoming: OrderTree, frozen: &[OrderLine]) -> Result<()> {
        incoming.version = match &self.order_tree {
            Some(previous) => {
                incoming.version = previous.version;
                if *previous == incoming {
                    previous.version
                } else {
                    previous
                        .version
                        .checked_add(1)
                        .ok_or_else(|| Error::ConflictError("供应商订单关系版本已耗尽".into()))?
                }
            },
            None => 1,
        };
        self.apply_order_tree(incoming, frozen)
    }

    /// 应用已核验的完整订单关系，保留根和历史父节点。
    /// # 参数
    /// `incoming` 为来源新关系，`frozen` 为原请求行。
    /// # 返回
    /// 保存可用于后续确认与数量应用的当前拓扑。
    /// # 错误
    /// 数量不守恒、版本回退或历史身份变化时原记录不改变。
    pub fn apply_order_tree(&mut self, incoming: OrderTree, frozen: &[OrderLine]) -> Result<()> {
        let tree = match &self.order_tree {
            Some(previous) => previous.merge(incoming, frozen)?,
            None => {
                incoming.validate(frozen)?;
                incoming
            },
        };

        self.order_tree = Some(tree);
        self.pending_order_tree = None;
        Ok(())
    }

    /// 在事务内登记单个叶子的确认发送意图。
    /// # 参数
    /// `request` 为当前拓扑叶子和稳定动作；返回后须先提交再调用供应商。
    /// # 返回
    /// 首次意图为 true；同键同载荷重放为 false，禁止重复发送。
    /// # 错误
    /// 已拆分目标、过期版本、不同载荷或不同订单身份时拒绝。
    pub fn begin_confirmation(&mut self, request: &ConfirmOrder) -> Result<bool> {
        if let Some(previous) = self.confirmations.get(&request.order.external_order_no) {
            return if previous.request == *request {
                Ok(false)
            } else {
                Err(Error::ConflictError("原订单确认目标或载荷已变化，请先查询原结果".into()))
            };
        }
        if self.confirmations.values().any(|previous| previous.request.action.id == request.action.id) {
            return Err(Error::ConflictError("原确认动作不能复用于其他供应商子单".into()));
        }

        self.validate_confirmation_target(request)?;

        self.confirmations.insert(
            request.order.external_order_no.clone(),
            ConfirmationAction { request: request.clone(), state: ConfirmationActionState::SentUnknown },
        );
        self.stage = ApiOrderStage::ConfirmationUnknown;
        Ok(true)
    }

    /// 仅将原目标的明确确认结果应用到原动作。
    /// # 参数
    /// `external_order_no` 为原动作目标，`state` 来自原单查询或本次响应。
    /// # 返回
    /// 明确成功或拒绝时更新原动作；Pending 保留未知且不允许重新发送。
    /// # 错误
    /// 找不到原动作或已确认事实冲突时拒绝，不传播成功到新拆分子单。
    pub fn observe_confirmation(&mut self, external_order_no: &str, state: ConfirmationState) -> Result<()> {
        let action = self
            .confirmations
            .get_mut(external_order_no)
            .ok_or_else(|| Error::NotFound("找不到原订单确认动作".into()))?;
        if state == ConfirmationState::Pending {
            return Ok(());
        }
        let next = match state {
            ConfirmationState::Confirmed => ConfirmationActionState::Confirmed,
            ConfirmationState::Rejected => ConfirmationActionState::Rejected,
            ConfirmationState::Pending => return Ok(()),
        };
        if action.state != ConfirmationActionState::SentUnknown && action.state != next {
            return Err(Error::ConflictError("原订单确认结果发生冲突".into()));
        }

        action.state = next;
        let mut reservation_root = false;
        if let Some(node) = self
            .order_tree
            .as_mut()
            .and_then(|tree| tree.nodes.iter_mut().find(|node| node.external_order_no == external_order_no))
        {
            reservation_root = node.role == OrderNodeRole::ReservationRoot;
            if !reservation_root {
                node.confirmation = Some(state);
            }
        }

        self.stage = if next == ConfirmationActionState::Rejected {
            ApiOrderStage::Stopped
        } else if reservation_root {
            ApiOrderStage::DiscoverOrders
        } else if self.all_leaves_confirmed() {
            ApiOrderStage::Observe
        } else if self
            .confirmations
            .values()
            .any(|action| action.state == ConfirmationActionState::SentUnknown)
        {
            ApiOrderStage::ConfirmationUnknown
        } else {
            ApiOrderStage::ConfirmOrder
        };
        Ok(())
    }

    /// 判断全部当前叶子是否均有明确成功事实。
    /// # 参数
    /// 无。
    /// # 返回
    /// 缺少拓扑或任何叶子未确认时为 false。
    /// # 错误
    /// 不返回错误。
    pub fn all_leaves_confirmed(&self) -> bool {
        self.order_tree.as_ref().is_some_and(|tree| {
            let leaves = tree.leaves();
            !leaves.is_empty()
                && !tree.nodes.iter().any(|node| node.role == OrderNodeRole::Unresolved)
                && leaves.iter().all(|node| node.confirmation == Some(ConfirmationState::Confirmed))
        })
    }

    /// 判断原预占根单已确认、但来源尚未提供可跟踪的拆单结果。
    /// # 参数
    /// 无；读取原根动作回执与当前来源观察，二者不相互替代。
    /// # 返回
    /// 已确认的预占根动作仍对应预占观察时返回真，须继续查询原根单。
    /// # 错误
    /// 不返回错误，不把根动作成功复制给后续子单。
    pub fn awaiting_reservation_discovery(&self) -> bool {
        self.order_tree.as_ref().is_some_and(|tree| {
            tree.nodes.iter().any(|node| {
                node.role == OrderNodeRole::ReservationRoot
                    && self
                        .confirmations
                        .get(&node.external_order_no)
                        .is_some_and(|action| action.state == ConfirmationActionState::Confirmed)
            })
        })
    }

    fn validate_confirmation_target(&self, request: &ConfirmOrder) -> Result<()> {
        let tree = self
            .order_tree
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("供应商当前子单尚未核实".into()))?;
        if request.order.merchant_order_no != self.base.id
            || request.topology_version != tree.version
            || request.action.id.trim().is_empty()
            || request.action.payload_hash.trim().is_empty()
        {
            return Err(Error::ConflictError("订单确认身份或版本不一致".into()));
        }

        let node = tree
            .nodes
            .iter()
            .find(|node| node.external_order_no == request.order.external_order_no)
            .ok_or_else(|| Error::NotFound("当前供应商子单不存在".into()))?;
        if !node.is_action_target() || node.confirmation != Some(ConfirmationState::Pending) {
            return Err(Error::BusinessLogicError("当前子单不可确认，请先查询原结果".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::slice::from_ref;

    use super::*;
    use crate::ports::connector::common::{ActionKey, SupplierSku};
    use crate::ports::connector::order::OrderReference;
    use crate::ports::connector::order_tree::{OrderNode, OrderNodeLine};

    fn execution() -> (ApiOrderExecution, Vec<OrderLine>) {
        let mut value = ApiOrderExecution::queued(
            "group".into(),
            "purchase".into(),
            "revision".into(),
            "connection".into(),
            "supplier".into(),
            vec![],
        );
        let line = OrderLine {
            source_purchase_terms: None,
            line_id: "line".into(),
            sku: SupplierSku { product_id: None, spec_id: "sku".into() },
            quantity: "2".parse().unwrap(),
            approved_unit_price: "1".parse().unwrap(),
            attributes: vec![],
        };
        value
            .apply_order_tree(
                OrderTree {
                    root_order_no: "root".into(),
                    version: 1,
                    nodes: vec![OrderNode {
                        external_order_no: "root".into(),
                        parent_order_no: None,
                        role: OrderNodeRole::Leaf,
                        lines: vec![OrderNodeLine {
                            line_id: line.line_id.clone(),
                            sku: line.sku.clone(),
                            quantity: line.quantity,
                        }],
                        confirmation: Some(ConfirmationState::Pending),
                        amounts: None,
                        fulfillment: None,
                        cancellation: None,
                        refund: None,
                    }],
                },
                from_ref(&line),
            )
            .unwrap();
        (value, vec![line])
    }

    fn request() -> ConfirmOrder {
        ConfirmOrder {
            action: ActionKey { id: "group-confirm-root".into(), payload_hash: "payload".into() },
            order: OrderReference { merchant_order_no: "group".into(), external_order_no: "root".into() },
            topology_version: 1,
        }
    }

    #[test]
    fn reservation_root_confirmation_keeps_original_action_and_discovers_actual_children() {
        let (mut value, frozen) = execution();
        value.order_tree.as_mut().unwrap().nodes[0].role = OrderNodeRole::ReservationRoot;
        value.order_tree.as_ref().unwrap().validate(&frozen).unwrap();
        assert!(value.order_tree.as_ref().unwrap().leaves().is_empty());

        assert!(value.begin_confirmation(&request()).unwrap());
        let mut recovered: ApiOrderExecution =
            serde_json::from_str(&serde_json::to_string(&value).unwrap()).unwrap();
        assert!(!recovered.begin_confirmation(&request()).unwrap());
        recovered.observe_confirmation("root", ConfirmationState::Pending).unwrap();
        assert_eq!(recovered.stage, ApiOrderStage::ConfirmationUnknown);

        recovered.observe_confirmation("root", ConfirmationState::Confirmed).unwrap();
        assert_eq!(recovered.stage, ApiOrderStage::DiscoverOrders);
        assert!(recovered.awaiting_reservation_discovery());
        assert!(!recovered.all_leaves_confirmed());

        let mut split = recovered.order_tree.as_ref().unwrap().clone();
        split.nodes[0].role = OrderNodeRole::Parent;
        split.nodes[0].confirmation = Some(ConfirmationState::Confirmed);
        for id in ["child-a", "child-b"] {
            let mut child = split.nodes[0].clone();
            child.external_order_no = id.into();
            child.parent_order_no = Some("root".into());
            child.role = OrderNodeRole::Leaf;
            child.lines[0].quantity = "1".parse().unwrap();
            child.confirmation = Some(ConfirmationState::Pending);
            split.nodes.push(child);
        }
        recovered.apply_observed_tree(split, &frozen).unwrap();
        assert_eq!(recovered.order_tree.as_ref().unwrap().leaves().len(), 2);
        assert!(!recovered.awaiting_reservation_discovery());
        assert!(!recovered.all_leaves_confirmed());
        assert_eq!(recovered.confirmations.len(), 1);
        assert_eq!(recovered.confirmations["root"].request, request());
        assert_eq!(recovered.confirmations["root"].state, ConfirmationActionState::Confirmed);
    }

    #[test]
    fn confirmation_is_persisted_before_send_and_unknown_never_reopens() {
        let (mut value, _) = execution();
        assert!(value.begin_confirmation(&request()).unwrap());
        assert_eq!(value.stage, ApiOrderStage::ConfirmationUnknown);

        let encoded = serde_json::to_string(&value).unwrap();
        let mut recovered: ApiOrderExecution = serde_json::from_str(&encoded).unwrap();
        assert!(!recovered.begin_confirmation(&request()).unwrap());
        recovered.observe_confirmation("root", ConfirmationState::Pending).unwrap();
        assert!(!recovered.begin_confirmation(&request()).unwrap());
        assert_eq!(recovered.stage, ApiOrderStage::ConfirmationUnknown);

        recovered.observe_confirmation("root", ConfirmationState::Confirmed).unwrap();
        assert_eq!(recovered.stage, ApiOrderStage::Observe);
        assert!(!recovered.begin_confirmation(&request()).unwrap());
    }

    #[test]
    fn changed_payload_or_old_topology_never_reuses_confirmation() {
        let (mut value, _) = execution();
        let mut stale = request();
        stale.topology_version = 2;
        assert!(value.begin_confirmation(&stale).is_err());
        value.begin_confirmation(&request()).unwrap();
        let mut changed = request();
        changed.action.payload_hash = "changed".into();
        assert!(value.begin_confirmation(&changed).is_err());
        assert!(value.ensure_unsent_for_purchase_change().is_err());
    }

    #[test]
    fn confirmation_of_split_parent_does_not_confirm_new_leaves() {
        let (mut value, lines) = execution();
        value.begin_confirmation(&request()).unwrap();
        let mut split = value.order_tree.clone().unwrap();
        split.version = 2;
        split.nodes[0].role = OrderNodeRole::Parent;
        for id in ["child1", "child2"] {
            let mut child = split.nodes[0].clone();
            child.external_order_no = id.into();
            child.parent_order_no = Some("root".into());
            child.role = OrderNodeRole::Leaf;
            child.lines[0].quantity = "1".parse().unwrap();
            split.nodes.push(child);
        }
        value.apply_order_tree(split, &lines).unwrap();
        value.observe_confirmation("root", ConfirmationState::Confirmed).unwrap();
        assert!(!value.all_leaves_confirmed());
        assert_eq!(value.stage, ApiOrderStage::ConfirmOrder);
        assert!(
            value
                .order_tree
                .unwrap()
                .leaves()
                .iter()
                .all(|leaf| leaf.confirmation == Some(ConfirmationState::Pending))
        );
    }

    #[test]
    fn historical_execution_does_not_acquire_confirmation_or_topology() {
        let value =
            ApiOrderExecution::queued("group".into(), "p".into(), "r".into(), "c".into(), "s".into(), vec![]);
        let mut json = serde_json::to_value(&value).unwrap();
        json.as_object_mut().unwrap().remove("order_tree");
        json.as_object_mut().unwrap().remove("pending_order_tree");
        json.as_object_mut().unwrap().remove("confirmations");
        let restored: ApiOrderExecution = serde_json::from_value(json).unwrap();
        assert!(restored.order_tree.is_none());
        assert!(restored.confirmations.is_empty());
        assert_eq!(restored.stage, ApiOrderStage::Queued);
    }

    #[test]
    fn observation_versions_are_assigned_locally_and_identical_content_is_replayed() {
        let (mut value, lines) = execution();
        let mut incoming = value.order_tree.clone().unwrap();
        incoming.version = 900;
        value.apply_observed_tree(incoming.clone(), &lines).unwrap();
        assert_eq!(value.order_tree.as_ref().unwrap().version, 1);
        incoming.nodes[0].confirmation = Some(ConfirmationState::Confirmed);
        value.apply_observed_tree(incoming.clone(), &lines).unwrap();
        assert_eq!(value.order_tree.as_ref().unwrap().version, 2);
        value.apply_observed_tree(incoming, &lines).unwrap();
        assert_eq!(value.order_tree.as_ref().unwrap().version, 2);
    }
}
