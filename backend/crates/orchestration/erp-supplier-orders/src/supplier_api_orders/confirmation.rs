//! 预占订单确认独立于支付通知；未知结果仅查询原目标。
mod execution;

use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderStage, ConfirmationActionState};
use erp_supply::entity::execution_budget::ApprovedExecutionBudget;
use erp_supply::ports::connector::common::Lookup;
use erp_supply::ports::connector::order::{ConfirmOrder, ConfirmationState, OrderKey, OrderReference};
use erp_supply::ports::connector::order_tree::OrderTree;
use erp_supply::repository::SupplierFulfillmentExt;
use erp_supply::service::supplier_api::SupplierApiService;
use execution::{MongoConfirmationApplication, confirm_current_order};
use persistence_core::NoTransaction;

use super::SupplierApiOrderProcess;
use super::steps::freeze_action;
use crate::{Error, Result};

impl SupplierApiOrderProcess {
    pub(super) async fn discover_orders(&self, mut execution: ApiOrderExecution) -> Result<()> {
        if let Err(error) = self.refresh_order_tree(&mut execution).await {
            execution.defer(error.supplier_order_message());
        }
        self.persist(&mut execution).await
    }

    /// 按当前原订单事实确认尚未发送的有效子单，不复用创建购买预检。
    /// # 参数
    /// 服务端持久化执行记录；连接及确认能力由注册表重新解析。
    /// # 返回
    /// 本轮明确结果或原目标待核实状态已保存。
    /// # 错误
    /// 采购、金额、授权、资金或持久化不满足时停止，原未知动作不重发。
    pub(super) async fn confirm_order(&self, execution: ApiOrderExecution) -> Result<()> {
        let connector = self.connector(&execution).await?;
        let application = MongoConfirmationApplication::new(self, connector.as_ref())?;
        confirm_current_order(&application, execution).await
    }

    pub(super) async fn recover_confirmation(&self, mut execution: ApiOrderExecution) -> Result<()> {
        if let Err(error) = self.refresh_order_tree(&mut execution).await {
            execution.defer(error.supplier_order_message());
        } else if execution.stage == ApiOrderStage::ConfirmationUnknown {
            execution.defer("原订单确认结果尚未查明；未确认或暂不可见均不代表可以重发");
        }
        self.persist(&mut execution).await
    }

    async fn refresh_order_tree(&self, execution: &mut ApiOrderExecution) -> Result<()> {
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(&execution.connection_id, &mut NoTransaction)
            .await?;
        let frozen = self.decode(execution)?;

        let result = self
            .runtime
            .observe_order_with_lines(
                &connection,
                &OrderKey::Merchant(execution.base.id.clone()),
                &frozen.lines,
            )
            .await
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        let Lookup::Found(observed) = result else {
            return Err(Error::BusinessLogicError("原订单当前暂不可见，继续查询原结果".into()));
        };
        if observed.value.reference.merchant_order_no != execution.base.id {
            return Err(Error::ConflictError("供应商订单与原采购执行身份不一致".into()));
        }

        let tree = observed
            .value
            .topology
            .ok_or_else(|| Error::BusinessLogicError("供应商当前有效子单尚未全部核实".into()))?;
        execution.pending_order_tree = Some(tree.clone());
        let original = self
            .db
            .supplier_fulfillment_orders()
            .find_by_id(&execution.base.id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("原供应商履约订单不存在".into()))?;
        if original.external_order_no.as_deref() != Some(tree.root_order_no.as_str())
            || observed.value.reference.external_order_no != tree.root_order_no
        {
            return Err(Error::ConflictError("供应商当前根单与原创建单号不一致，已阻断确认".into()));
        }

        let budget = frozen.approved_budget.as_ref().ok_or_else(|| {
            Error::BusinessLogicError("原执行组缺少可核实的批准运费和商品预算，已阻断确认".into())
        })?;
        validate_leaf_budget(&tree, budget)?;

        let mut updated = execution.clone();
        updated.apply_observed_tree(tree, &frozen.lines)?;
        apply_confirmation_observations(&mut updated)?;
        *execution = updated;
        execution.next_run_at = Instant::now();
        Ok(())
    }
}

fn next_confirmation(execution: &mut ApiOrderExecution) -> Result<ConfirmOrder> {
    let tree = execution
        .order_tree
        .as_ref()
        .ok_or_else(|| Error::BusinessLogicError("当前供应商订单关系尚未核实".into()))?;
    let target = tree
        .action_targets()
        .into_iter()
        .find(|node| {
            node.confirmation == Some(ConfirmationState::Pending)
                && !execution.confirmations.contains_key(&node.external_order_no)
        })
        .ok_or_else(|| {
            Error::BusinessLogicError("当前没有已经核实且可首次确认的子单，请查询原结果".into())
        })?;

    let order = OrderReference {
        merchant_order_no: execution.base.id.clone(),
        external_order_no: target.external_order_no.clone(),
    };
    let version = tree.version;
    let action =
        freeze_action(execution, &format!("confirm:{}", order.external_order_no), &(&order, version))?;
    Ok(ConfirmOrder { action, order, topology_version: version })
}

fn apply_confirmation_observations(execution: &mut ApiOrderExecution) -> Result<()> {
    let observations: Vec<_> = execution
        .order_tree
        .as_ref()
        .into_iter()
        .flat_map(|tree| &tree.nodes)
        .filter_map(|node| node.confirmation.map(|state| (node.external_order_no.clone(), state)))
        .collect();
    for (target, state) in observations {
        if execution.confirmations.contains_key(&target) {
            execution.observe_confirmation(&target, state)?;
        }
    }

    execution.stage =
        if execution.confirmations.values().any(|action| action.state == ConfirmationActionState::Rejected) {
            ApiOrderStage::Stopped
        } else if execution
            .confirmations
            .values()
            .any(|action| action.state == ConfirmationActionState::SentUnknown)
        {
            ApiOrderStage::ConfirmationUnknown
        } else if execution.awaiting_reservation_discovery() {
            ApiOrderStage::DiscoverOrders
        } else if execution.all_leaves_confirmed() {
            ApiOrderStage::Observe
        } else {
            ApiOrderStage::ConfirmOrder
        };
    Ok(())
}

fn validate_leaf_budget(tree: &OrderTree, budget: &ApprovedExecutionBudget) -> Result<()> {
    let mut total = [Amount::zero().to_decimal(); 4];
    let leaves = tree.action_targets();
    if leaves.is_empty() {
        return Err(Error::BusinessLogicError("供应商当前有效子单尚未核实".into()));
    }
    for leaf in leaves {
        let amounts = leaf
            .amounts
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("当前子单缺少可核实的应付金额，已阻断确认".into()))?;
        let (Some(goods), Some(shipping), Some(service)) =
            (amounts.goods_cost, amounts.shipping_cost, amounts.service_cost)
        else {
            return Err(Error::BusinessLogicError(
                "当前子单商品、运费或附加费用尚未分别核实，已阻断确认".into(),
            ));
        };
        let parts = [goods, shipping, service, amounts.payable];
        for (index, amount) in parts.into_iter().enumerate() {
            if amount < Amount::zero() {
                return Err(Error::BusinessLogicError("供应商子单金额无效".into()));
            }
            total[index] = total[index]
                .checked_add(amount.to_decimal())
                .ok_or_else(|| Error::ValidationError("子单金额汇总超限".into()))?;
        }
        if goods
            .to_decimal()
            .checked_add(shipping.to_decimal())
            .and_then(|value| value.checked_add(service.to_decimal()))
            != Some(amounts.payable.to_decimal())
        {
            return Err(Error::BusinessLogicError("供应商子单费用不守恒，已阻断确认".into()));
        }
    }

    let approved =
        [budget.approved_goods, budget.approved_freight, budget.approved_other, budget.approved_total];
    if total.iter().zip(approved).any(|(actual, allowed)| *actual > allowed.to_decimal()) {
        return Err(Error::BusinessLogicError("供应商子单商品金额或运费超出对应批准范围，已阻断确认".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::slice::from_ref;

    use erp_supply::ports::connector::common::SupplierSku;
    use erp_supply::ports::connector::order::{OrderAmounts, OrderLine};
    use erp_supply::ports::connector::order_tree::{OrderNode, OrderNodeLine, OrderNodeRole};

    use super::*;

    fn amounts(goods: &str, freight: &str, service: &str, payable: &str) -> OrderAmounts {
        OrderAmounts {
            payable: payable.parse().unwrap(),
            goods_cost: Some(goods.parse().unwrap()),
            shipping_cost: Some(freight.parse().unwrap()),
            service_cost: Some(service.parse().unwrap()),
        }
    }

    pub(super) fn budget() -> ApprovedExecutionBudget {
        ApprovedExecutionBudget {
            purchase_revision_id: "revision".into(),
            execution_id: "group".into(),
            approved_goods: "20".parse().unwrap(),
            approved_freight: "2".parse().unwrap(),
            approved_other: Amount::zero(),
            approved_total: "22".parse().unwrap(),
            freight_allocations: vec![],
        }
    }

    pub(super) fn fixture() -> (ApiOrderExecution, Vec<OrderLine>) {
        let line = OrderLine {
            source_purchase_terms: None,
            line_id: "line".into(),
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
            confirmation: None,
            amounts: Some(amounts("20", "2", "0", "22")),
            fulfillment: None,
            cancellation: None,
            refund: None,
        };
        let mut nodes = vec![root.clone()];
        for id in ["child-a", "child-b"] {
            let mut leaf = root.clone();
            leaf.external_order_no = id.into();
            leaf.parent_order_no = Some("root".into());
            leaf.role = OrderNodeRole::Leaf;
            leaf.lines[0].quantity = "1".parse().unwrap();
            leaf.confirmation = Some(ConfirmationState::Pending);
            leaf.amounts = Some(amounts("10", "1", "0", "11"));
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
        execution.stage = ApiOrderStage::ConfirmOrder;
        (execution, vec![line])
    }

    #[test]
    fn confirmation_counts_current_leaves_once_and_rejects_freight_inside_total_budget() {
        let (execution, _) = fixture();
        let mut tree = execution.order_tree.unwrap();
        validate_leaf_budget(&tree, &budget()).unwrap();

        tree.nodes[0].amounts = Some(amounts("2000", "200", "0", "2200"));
        validate_leaf_budget(&tree, &budget()).unwrap();

        tree.nodes[1].amounts = Some(amounts("9", "2", "0", "11"));
        let error = validate_leaf_budget(&tree, &budget()).unwrap_err();
        assert!(error.to_string().contains("对应批准范围"));
    }

    #[test]
    fn confirmation_requires_complete_cost_evidence_and_rejects_service_or_nonconserving_amounts() {
        let (execution, _) = fixture();
        let original = execution.order_tree.unwrap();
        let mut missing = original.clone();
        missing.nodes[1].amounts = None;
        assert!(validate_leaf_budget(&missing, &budget()).is_err());

        for field in ["goods", "freight", "service"] {
            let mut tree = original.clone();
            let amount = tree.nodes[1].amounts.as_mut().unwrap();
            match field {
                "goods" => amount.goods_cost = None,
                "freight" => amount.shipping_cost = None,
                _ => amount.service_cost = None,
            }
            assert!(validate_leaf_budget(&tree, &budget()).is_err());
        }

        for amount in
            [amounts("9", "1", "1", "11"), amounts("10", "1", "0", "10"), amounts("-1", "1", "0", "0")]
        {
            let mut tree = original.clone();
            tree.nodes[1].amounts = Some(amount);
            assert!(validate_leaf_budget(&tree, &budget()).is_err());
        }
    }

    #[test]
    fn unknown_confirmation_recovers_original_target_and_never_selects_it_for_resend() {
        let (mut execution, _) = fixture();
        let first = next_confirmation(&mut execution).unwrap();
        assert_eq!(first.order.external_order_no, "child-a");
        assert!(execution.begin_confirmation(&first).unwrap());

        let mut recovered: ApiOrderExecution =
            serde_json::from_str(&serde_json::to_string(&execution).unwrap()).unwrap();
        apply_confirmation_observations(&mut recovered).unwrap();
        assert_eq!(recovered.stage, ApiOrderStage::ConfirmationUnknown);
        assert_eq!(recovered.confirmations["child-a"].state, ConfirmationActionState::SentUnknown);
        let second = next_confirmation(&mut recovered).unwrap();
        assert_eq!(second.order.external_order_no, "child-b");

        recovered.order_tree.as_mut().unwrap().nodes[1].confirmation = Some(ConfirmationState::Confirmed);
        apply_confirmation_observations(&mut recovered).unwrap();
        assert_eq!(recovered.stage, ApiOrderStage::ConfirmOrder);
        assert_eq!(recovered.confirmations["child-a"].request, first);
        assert!(recovered.begin_confirmation(&second).unwrap());
        assert!(next_confirmation(&mut recovered).is_err());

        recovered.order_tree.as_mut().unwrap().nodes[2].confirmation = Some(ConfirmationState::Confirmed);
        apply_confirmation_observations(&mut recovered).unwrap();
        assert_eq!(recovered.stage, ApiOrderStage::Observe);
        assert_eq!(recovered.confirmations.len(), 2);
    }

    #[test]
    fn only_explicit_rejected_observation_stops_confirmation() {
        let (mut execution, _) = fixture();
        let request = next_confirmation(&mut execution).unwrap();
        execution.begin_confirmation(&request).unwrap();
        execution.order_tree.as_mut().unwrap().nodes[1].confirmation = Some(ConfirmationState::Rejected);
        apply_confirmation_observations(&mut execution).unwrap();
        assert_eq!(execution.stage, ApiOrderStage::Stopped);
        assert_eq!(execution.confirmations["child-a"].state, ConfirmationActionState::Rejected);
        assert_eq!(
            execution.order_tree.as_ref().unwrap().nodes[2].confirmation,
            Some(ConfirmationState::Pending)
        );
    }
}
