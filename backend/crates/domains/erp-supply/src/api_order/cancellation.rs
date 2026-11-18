//! 当前叶子取消的发送证据；请求受理不等于取消完成，也不证明资金返还。

use std::collections::BTreeSet;

use erp_core::money::{Amount, round_to_cent};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{ApiOrderExecution, ApiOrderStage, ConfirmationActionState};
use crate::dto::supplier_fulfillment::{AfterSalesActionLineRequest, SubmitAfterSalesActionRequest};
use crate::entity::supplier_fulfillment::{CancelStatus, SupplierFulfillmentOrder};
use crate::ports::connector::common::ActionKey;
use crate::ports::connector::order::{ActionAccepted, CancelOrder, OrderLine, OrderReference};
use crate::ports::connector::order_tree::OrderNodeLine;
use crate::{Error, Result};

/// 首次未发送、已发送未知、申请受理及查询证实终态分别保存。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CancellationActionState {
    Ready,
    SentUnknown,
    Accepted,
    Canceled,
    Failed,
}

/// 每个子单独立冻结原目标、数量和发送状态，禁止未知结果重新发送。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancellationAction {
    pub action: ActionKey,
    pub order: OrderReference,
    pub topology_version: u64,
    pub lines: Vec<OrderNodeLine>,
    pub reason_code: String,
    pub root_action_id: String,
    pub state: CancellationActionState,
    pub external_request_id: Option<String>,
}

impl CancellationAction {
    /// 构造已经冻结范围的整叶取消请求。
    /// # 参数
    /// 无；只读取服务器保存的原动作。
    /// # 返回
    /// 本叶整单取消，不把 ERP 行或金额改成供应商部分取消参数。
    /// # 错误
    /// 不返回错误；发送资格须先由执行组方法原子登记。
    pub fn request(&self) -> CancelOrder {
        CancelOrder {
            action: self.action.clone(),
            order: self.order.clone(),
            lines: None,
            reason_code: self.reason_code.clone(),
        }
    }
}

/// 经服务器校验的完整执行组取消范围。
#[derive(Debug, Clone)]
pub struct CancellationScope {
    pub payload_hash: String,
    pub reason_code: String,
}

impl CancellationScope {
    /// 校验取消范围恰好覆盖原冻结组，不接受没有子单选择依据的任意部分数量。
    /// # 参数
    /// `request` 为原履约入口请求，`frozen` 为加密快照中的真实采购行。
    /// # 返回
    /// 原因、行顺序规范化后的稳定载荷摘要。
    /// # 错误
    /// 缺行、重复、数量不同、原因缺失及过长文本时拒绝；兼容金额不参与范围判断。
    pub fn new(request: &SubmitAfterSalesActionRequest, frozen: &[OrderLine]) -> Result<Self> {
        let reason = request.reason_code.as_deref().unwrap_or_default().trim();
        if reason.is_empty()
            || reason.len() > 128
            || request.comment.as_ref().is_some_and(|value| value.len() > 1000)
            || frozen.is_empty()
            || request.lines.len() != frozen.len()
        {
            return Err(Error::ValidationError(
                "请填写有效取消原因，并完整选择本次接口执行的原商品范围".into(),
            ));
        }

        let mut seen = BTreeSet::new();
        for line in &request.lines {
            let item = frozen
                .iter()
                .find(|item| item.line_id == line.supplier_fulfillment_item_id.as_ref())
                .ok_or_else(|| Error::ValidationError("取消商品不属于本次原采购执行".into()))?;
            if !seen.insert(item.line_id.as_str())
                || line.quantity != item.quantity
                || line.quantity.to_decimal() <= Amount::zero().to_decimal()
            {
                return Err(Error::ValidationError(
                    "取消数量必须完整对应原冻结采购组，部分数量须先明确子单范围".into(),
                ));
            }
        }

        let mut lines: Vec<_> = request
            .lines
            .iter()
            .map(|line| (line.supplier_fulfillment_item_id.clone(), line.quantity))
            .collect();
        lines.sort_by(|first, second| first.0.as_ref().cmp(second.0.as_ref()));
        let payload = serde_json::to_vec(&(lines, reason, request.comment.as_deref().map(str::trim)))
            .map_err(|_| Error::ValidationError("取消请求无法生成稳定摘要".into()))?;
        Ok(Self { payload_hash: hex::encode(Sha256::digest(payload)), reason_code: reason.into() })
    }

    /// 从已校验冻结采购计算动作记载金额，不消费客户端兼容金额。
    /// # 参数
    /// `request` 为同一范围请求，`frozen` 为服务器真实冻结行。
    /// # 返回
    /// 保留取消范围、按原采购计算金额的服务器动作数据；不建立任何资金核销。
    /// # 错误
    /// 原范围变化或定点金额溢出时拒绝。
    pub fn canonical_request(
        &self,
        request: &SubmitAfterSalesActionRequest,
        frozen: &[OrderLine],
    ) -> Result<SubmitAfterSalesActionRequest> {
        if Self::new(request, frozen)?.payload_hash != self.payload_hash {
            return Err(Error::ConflictError("取消范围与原请求不一致".into()));
        }

        let lines = frozen
            .iter()
            .map(|line| {
                let amount = line
                    .quantity
                    .to_decimal()
                    .checked_mul(line.approved_unit_price.to_decimal())
                    .ok_or_else(|| Error::ValidationError("原取消金额超出范围".into()))?;
                Ok(AfterSalesActionLineRequest {
                    supplier_fulfillment_item_id: line.line_id.clone().into(),
                    quantity: line.quantity,
                    amount: Amount::try_from(round_to_cent(amount))?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(SubmitAfterSalesActionRequest {
            request_id: request.request_id.clone(),
            lines,
            reason_code: Some(self.reason_code.clone()),
            comment: request.comment.clone(),
        })
    }
}

impl ApiOrderExecution {
    /// 核验取消关联的原履约订单、连接和根单身份。
    /// # 参数
    /// `order` 为同一事务内服务器读取的原履约订单。
    /// # 返回
    /// 原订单、供应商、连接和已保存根单均相同时成功。
    /// # 错误
    /// 任一归属或根单不一致时拒绝，不使用请求中的归属。
    pub fn ensure_cancellation_order(&self, order: &SupplierFulfillmentOrder) -> Result<()> {
        if order.base.id != self.base.id
            || order.connection_id.as_ref() != self.connection_id
            || order.supplier_id.as_ref() != self.supplier_id
            || order.external_order_no.is_none()
            || order.external_order_no.as_deref()
                != self.order_tree.as_ref().map(|tree| tree.root_order_no.as_str())
        {
            return Err(Error::ConflictError("取消订单与原供应商连接或根单不一致".into()));
        }
        Ok(())
    }

    /// 为完整当前叶子冻结取消意图；重复请求不得重新开放发送。
    /// # 参数
    /// `scope` 为服务器已核对的原范围，`root_action_id` 为现有取消动作头。
    /// # 返回
    /// 首次冻结返回真，同载荷返回假。
    /// # 错误
    /// 同操作异载荷、无可信拓扑或原确认未知时拒绝。
    pub fn prepare_cancellations(&mut self, scope: &CancellationScope, root_action_id: &str) -> Result<bool> {
        if let Some(original) = &self.cancellation_payload_hash {
            if original != &scope.payload_hash {
                return Err(Error::ConflictError("原取消请求内容不同，请先查询原取消结果".into()));
            }
            return Ok(false);
        }

        if self.confirmations.values().any(|action| action.state == ConfirmationActionState::SentUnknown) {
            return Err(Error::BusinessLogicError("原订单确认尚未查明，请先查询确认结果后取消".into()));
        }
        let tree = self
            .order_tree
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("当前有效子单尚未核实，请先查询原订单".into()))?;
        let leaves = tree.action_targets();
        if leaves.is_empty() || root_action_id.trim().is_empty() {
            return Err(Error::ValidationError("取消范围或动作身份缺失".into()));
        }

        let mut actions = self.cancellation_actions.clone();
        for leaf in leaves {
            let order = OrderReference {
                merchant_order_no: self.base.id.clone(),
                external_order_no: leaf.external_order_no.clone(),
            };
            let payload = serde_json::to_vec(&(&order, &leaf.lines, &scope.reason_code))
                .map_err(|_| Error::ValidationError("子单取消内容无法保存".into()))?;
            let hash = hex::encode(Sha256::digest(payload));
            actions.insert(
                leaf.external_order_no.clone(),
                CancellationAction {
                    action: ActionKey { id: format!("cancel-{}", hash), payload_hash: hash },
                    order,
                    topology_version: tree.version,
                    lines: leaf.lines.clone(),
                    reason_code: scope.reason_code.clone(),
                    root_action_id: root_action_id.into(),
                    state: CancellationActionState::Ready,
                    external_request_id: None,
                },
            );
        }

        self.cancellation_actions = actions;
        self.cancellation_payload_hash = Some(scope.payload_hash.clone());
        self.cancellation_action_id = Some(root_action_id.into());
        self.stage = ApiOrderStage::CancelOrders;
        Ok(true)
    }

    /// 原子保存一次取消发送意图，原子提交后才能调用供应商。
    /// # 参数
    /// `target` 为已经刷新核实的当前叶子订单号。
    /// # 返回
    /// 仅 Ready 首次返回请求，已发送或受理的动作返回空且不重发。
    /// # 错误
    /// 当前叶子身份、数量或原取消范围变化时拒绝。
    pub fn begin_cancellation(&mut self, target: &str) -> Result<Option<CancelOrder>> {
        let Some(request) = self.cancellation_request(target)? else { return Ok(None) };
        let action = self.cancellation_actions.get_mut(target).expect("原取消请求已核实");
        action.topology_version = self.order_tree.as_ref().expect("原树已核实").version;
        action.state = CancellationActionState::SentUnknown;
        self.stage = ApiOrderStage::CancellationUnknown;
        Ok(Some(request))
    }

    /// 读取尚未发送的稳定原取消请求，供只读资格核验。
    /// # 参数
    /// `target` 为已经持久化的真实预占根单或当前叶子身份。
    /// # 返回
    /// 未发送且当前数量范围不变时返回原请求；已发送返回空。
    /// # 错误
    /// 目标已拆分、身份丢失或范围改变时拒绝，不替换原动作目标。
    pub fn cancellation_request(&self, target: &str) -> Result<Option<CancelOrder>> {
        let action = self
            .cancellation_actions
            .get(target)
            .ok_or_else(|| Error::NotFound("原子单取消动作不存在".into()))?;
        if action.state != CancellationActionState::Ready {
            return Ok(None);
        }

        let tree = self
            .order_tree
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("当前子单关系尚未核实".into()))?;
        let node = tree
            .nodes
            .iter()
            .find(|node| node.external_order_no == target)
            .ok_or_else(|| Error::BusinessLogicError("原取消子单当前暂不可见，继续调查".into()))?;
        if !node.is_action_target() || node.lines != action.lines {
            return Err(Error::ConflictError("原取消子单已拆分或范围改变，请先核查当前全部子单".into()));
        }
        Ok(Some(action.request()))
    }

    /// 保存原动作的受理回执，不登记取消完成。
    /// # 参数
    /// `target` 为原叶子，`accepted` 为本次唯一写调用返回值。
    /// # 返回
    /// 原动作进入受理后调查状态。
    /// # 错误
    /// 身份不同、原动作未发送或已存在冲突终态时拒绝。
    pub fn accept_cancellation(&mut self, target: &str, accepted: ActionAccepted) -> Result<()> {
        let action = self
            .cancellation_actions
            .get_mut(target)
            .ok_or_else(|| Error::NotFound("原取消动作不存在".into()))?;
        if action.action.id != accepted.action_id || action.state != CancellationActionState::SentUnknown {
            return Err(Error::ConflictError("取消受理身份与原请求不一致，继续查询原结果".into()));
        }

        action.state = CancellationActionState::Accepted;
        action.external_request_id = accepted.external_request_id;
        self.refresh_cancellation_stage();
        Ok(())
    }

    /// 采用原目标的查询结果更新取消动作；没有最终证据时保持原状态。
    /// # 参数
    /// 无；当前拓扑必须已按原根单完整核实。
    /// # 返回
    /// 只把明确取消或失败应用到原目标，不传播给新子单。
    /// # 错误
    /// 已确认终态发生冲突时拒绝。
    pub fn observe_cancellations(&mut self) -> Result<()> {
        let tree = self
            .order_tree
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("取消结果缺少原订单关系".into()))?;
        let mut actions = self.cancellation_actions.clone();
        for (target, action) in &mut actions {
            let Some(node) = tree.nodes.iter().find(|node| &node.external_order_no == target) else {
                continue;
            };
            if !node.is_action_target() {
                continue;
            }
            let next = match node.cancellation {
                Some(CancelStatus::Canceled) => CancellationActionState::Canceled,
                Some(CancelStatus::Failed) => CancellationActionState::Failed,
                Some(CancelStatus::CancelPending) => CancellationActionState::Accepted,
                _ => continue,
            };
            if matches!(action.state, CancellationActionState::Canceled | CancellationActionState::Failed) {
                if next != action.state && next != CancellationActionState::Accepted {
                    return Err(Error::ConflictError("原子单取消终态发生冲突，请核实来源证据".into()));
                }
            } else {
                action.state = next;
            }
        }

        self.cancellation_actions = actions;
        self.refresh_cancellation_stage();
        Ok(())
    }

    /// 汇总下一步，只为尚未发送的独立目标保留发送资格。
    /// # 参数
    /// 无；仅使用本组持久化取消状态。
    /// # 返回
    /// 有未发动作时继续逐单取消，有未知/受理时仅查询，否则停止新写并保留跟踪。
    /// # 错误
    /// 不返回错误；停止不表示供应商全量取消或资金已经返还。
    pub fn refresh_cancellation_stage(&mut self) {
        self.stage = if self
            .cancellation_actions
            .values()
            .any(|action| action.state == CancellationActionState::Ready)
        {
            ApiOrderStage::CancelOrders
        } else if self.cancellation_actions.values().any(|action| {
            matches!(action.state, CancellationActionState::SentUnknown | CancellationActionState::Accepted)
        }) {
            ApiOrderStage::CancellationUnknown
        } else {
            ApiOrderStage::Stopped
        };
    }

    /// 核验全部当前有效叶子的取消终态，不以历史父单完成代替新子单。
    /// # 参数
    /// 无；读取已核验当前关系及各原目标动作。
    /// # 返回
    /// 每个当前叶子均有取消完成的原动作和当前查询事实时为真。
    /// # 错误
    /// 不返回错误；缺单和再次拆单均保持未完成。
    pub fn all_cancellations_confirmed(&self) -> bool {
        self.order_tree.as_ref().is_some_and(|tree| {
            let leaves = tree.action_targets();
            !leaves.is_empty()
                && leaves.iter().all(|node| {
                    node.cancellation == Some(CancelStatus::Canceled)
                        && self
                            .cancellation_actions
                            .get(&node.external_order_no)
                            .is_some_and(|action| action.state == CancellationActionState::Canceled)
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use std::slice::from_ref;

    use erp_core::ids::SupplierFulfillmentItemId;

    use super::*;
    use crate::ports::connector::common::SupplierSku;
    use crate::ports::connector::order::ConfirmationState;
    use crate::ports::connector::order_tree::{OrderNode, OrderNodeRole, OrderTree};

    fn fixture() -> (ApiOrderExecution, Vec<OrderLine>, SubmitAfterSalesActionRequest) {
        let line = OrderLine {
            source_purchase_terms: None,
            line_id: "item".into(),
            sku: SupplierSku { product_id: None, spec_id: "sku".into() },
            quantity: "2".parse().unwrap(),
            approved_unit_price: "10".parse().unwrap(),
            attributes: vec![],
        };
        let root = OrderNode {
            external_order_no: "root".into(),
            parent_order_no: None,
            role: OrderNodeRole::Parent,
            lines: vec![OrderNodeLine {
                line_id: line.line_id.clone(),
                sku: line.sku.clone(),
                quantity: line.quantity,
            }],
            confirmation: Some(ConfirmationState::Confirmed),
            amounts: None,
            fulfillment: None,
            cancellation: None,
            refund: None,
        };
        let mut nodes = vec![root.clone()];
        for id in ["a", "b"] {
            let mut leaf = root.clone();
            leaf.external_order_no = id.into();
            leaf.parent_order_no = Some("root".into());
            leaf.role = OrderNodeRole::Leaf;
            leaf.lines[0].quantity = "1".parse().unwrap();
            nodes.push(leaf);
        }
        let mut execution = ApiOrderExecution::queued(
            "group".into(),
            "purchase".into(),
            "revision".into(),
            "connection".into(),
            "supplier".into(),
            vec![],
        );
        execution
            .apply_order_tree(OrderTree { root_order_no: "root".into(), version: 1, nodes }, from_ref(&line))
            .unwrap();
        let request = SubmitAfterSalesActionRequest {
            request_id: None,
            lines: vec![AfterSalesActionLineRequest {
                supplier_fulfillment_item_id: SupplierFulfillmentItemId::new("item"),
                quantity: "2".parse().unwrap(),
                amount: Amount::zero(),
            }],
            reason_code: Some("reason".into()),
            comment: None,
        };
        (execution, vec![line], request)
    }

    #[test]
    fn reservation_cancellation_freezes_the_real_parent_without_a_fabricated_leaf() {
        let (mut value, frozen, request) = fixture();
        let mut root = value.order_tree.as_ref().unwrap().nodes[0].clone();
        root.role = OrderNodeRole::ReservationRoot;
        root.confirmation = Some(ConfirmationState::Pending);
        value.order_tree = Some(OrderTree { root_order_no: "root".into(), version: 1, nodes: vec![root] });
        value.order_tree.as_ref().unwrap().validate(&frozen).unwrap();
        let scope = CancellationScope::new(&request, &frozen).unwrap();
        assert!(value.prepare_cancellations(&scope, "cancel-reservation").unwrap());
        assert!(value.order_tree.as_ref().unwrap().leaves().is_empty());
        let original = value.cancellation_request("root").unwrap().unwrap();
        assert_eq!(original.order.external_order_no, "root");
        assert_eq!(value.cancellation_actions["root"].state, CancellationActionState::Ready);
        assert_eq!(value.begin_cancellation("root").unwrap(), Some(original.clone()));
        let mut restored: ApiOrderExecution =
            serde_json::from_str(&serde_json::to_string(&value).unwrap()).unwrap();
        assert!(restored.begin_cancellation("root").unwrap().is_none());
        restored.order_tree.as_mut().unwrap().nodes[0].cancellation = Some(CancelStatus::Canceled);
        restored.observe_cancellations().unwrap();
        assert!(restored.all_cancellations_confirmed());
        assert_eq!(restored.cancellation_actions["root"].request(), original);
    }

    #[test]
    fn cancel_scope_uses_frozen_quantities_and_server_cost_ignoring_client_compatibility_amount() {
        let (_, frozen, request) = fixture();
        let scope = CancellationScope::new(&request, &frozen).unwrap();
        let mut altered = request.clone();
        altered.lines[0].amount = "999999".parse().unwrap();
        assert_eq!(scope.payload_hash, CancellationScope::new(&altered, &frozen).unwrap().payload_hash);
        let canonical = scope.canonical_request(&altered, &frozen).unwrap();
        assert_eq!(canonical.lines[0].amount, "20".parse().unwrap());
        assert_eq!(request.lines[0].amount, Amount::zero());

        altered.lines[0].quantity = "1".parse().unwrap();
        assert!(CancellationScope::new(&altered, &frozen).is_err());
        altered = request.clone();
        altered.lines[0].supplier_fulfillment_item_id = SupplierFulfillmentItemId::new("another-order");
        assert!(CancellationScope::new(&altered, &frozen).is_err());
    }

    #[test]
    fn accepted_and_partial_cancel_are_not_completed_or_refunded_and_unknown_targets_never_reopen() {
        let (mut execution, frozen, request) = fixture();
        let scope = CancellationScope::new(&request, &frozen).unwrap();
        assert!(execution.prepare_cancellations(&scope, "aggregate").unwrap());
        let first = execution.begin_cancellation("a").unwrap().unwrap();
        execution
            .accept_cancellation(
                "a",
                ActionAccepted { action_id: first.action.id, external_request_id: Some("accepted".into()) },
            )
            .unwrap();
        assert!(!execution.all_cancellations_confirmed());
        assert!(!execution.prepare_cancellations(&scope, "aggregate").unwrap());

        let mut recovered: ApiOrderExecution =
            serde_json::from_str(&serde_json::to_string(&execution).unwrap()).unwrap();
        assert!(recovered.begin_cancellation("a").unwrap().is_none());

        recovered.order_tree.as_mut().unwrap().nodes[1].cancellation = Some(CancelStatus::CancelPending);
        recovered.order_tree.as_mut().unwrap().nodes[2].cancellation = Some(CancelStatus::Canceled);
        recovered.observe_cancellations().unwrap();
        assert_eq!(recovered.stage, ApiOrderStage::CancellationUnknown);
        assert!(!recovered.all_cancellations_confirmed());
        assert!(recovered.order_tree.as_ref().unwrap().nodes.iter().all(|node| node.refund.is_none()));

        recovered.order_tree.as_mut().unwrap().nodes[1].cancellation = Some(CancelStatus::Canceled);
        recovered.observe_cancellations().unwrap();
        assert!(recovered.all_cancellations_confirmed());
        assert_eq!(recovered.stage, ApiOrderStage::Stopped);

        let mut changed = request;
        changed.reason_code = Some("changed".into());
        assert!(
            recovered
                .prepare_cancellations(&CancellationScope::new(&changed, &frozen).unwrap(), "aggregate")
                .is_err()
        );
    }

    #[test]
    fn split_parent_cancel_does_not_confirm_new_leaves_or_resend_original_unknown() {
        let (mut execution, frozen, request) = fixture();
        execution
            .prepare_cancellations(&CancellationScope::new(&request, &frozen).unwrap(), "aggregate")
            .unwrap();
        execution.begin_cancellation("a").unwrap().unwrap();

        let mut changed = execution.order_tree.clone().unwrap();
        changed.version = 2;
        changed.nodes[1].role = OrderNodeRole::Parent;
        changed.nodes[1].cancellation = Some(CancelStatus::Canceled);
        for id in ["a1", "a2"] {
            let mut leaf = changed.nodes[1].clone();
            leaf.external_order_no = id.into();
            leaf.parent_order_no = Some("a".into());
            leaf.role = OrderNodeRole::Leaf;
            leaf.lines[0].quantity = "0.5".parse().unwrap();
            leaf.cancellation = None;
            changed.nodes.push(leaf);
        }

        execution.apply_order_tree(changed, &frozen).unwrap();
        execution.observe_cancellations().unwrap();
        assert_eq!(execution.cancellation_actions["a"].state, CancellationActionState::SentUnknown);
        assert!(execution.begin_cancellation("a").unwrap().is_none());
        assert!(!execution.all_cancellations_confirmed());
    }
}
