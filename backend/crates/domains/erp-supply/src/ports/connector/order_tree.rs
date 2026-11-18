//! 供应商根单、历史拆单和当前叶子集合；父单不重复累计数量或金额。

use std::collections::{BTreeMap, BTreeSet};

use erp_core::money::Quantity;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::common::SupplierSku;
use super::order::{ConfirmationState, OrderAmounts, OrderLine};
use crate::entity::supplier_fulfillment::{CancelStatus, FulfillmentStatus, RefundStatus};
use crate::{Error, Result};

/// 叶子资格必须由供应商明确证明；暂未返回子单不构成叶子证明。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderNodeRole {
    Parent,
    /// 来源明确报告尚未确认、尚未拆分的原预占父单；只用于确认及取消，不形成交付。
    ReservationRoot,
    Leaf,
    Unresolved,
}

/// 外部订单数量映射到一个冻结 ERP 明细；相同 SKU 的多行分别保留。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderNodeLine {
    pub line_id: String,
    pub sku: SupplierSku,
    pub quantity: Quantity,
}

/// 历史父节点保留原身份和数量；只对当前 Leaf 产生后续动作。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderNode {
    pub external_order_no: String,
    pub parent_order_no: Option<String>,
    pub role: OrderNodeRole,
    pub lines: Vec<OrderNodeLine>,
    pub confirmation: Option<ConfirmationState>,
    pub amounts: Option<OrderAmounts>,
    #[serde(default)]
    pub fulfillment: Option<FulfillmentStatus>,
    #[serde(default)]
    pub cancellation: Option<CancelStatus>,
    #[serde(default)]
    pub refund: Option<RefundStatus>,
}

/// 同一执行组的完整拓扑。version 为本地持久化版本，不能替代供应商来源版本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderTree {
    pub root_order_no: String,
    pub version: u64,
    pub nodes: Vec<OrderNode>,
}

impl OrderTree {
    /// 核对身份、无环、当前叶子及逐行数量守恒。
    /// # 参数
    /// `frozen` 为原执行组批准并冻结的全部采购行。
    /// # 返回
    /// 完整且可用于确认与事实应用的拓扑。
    /// # 错误
    /// 缺单、未核实叶子、重复、环路、无归属行或数量不守恒时拒绝。
    pub fn validate(&self, frozen: &[OrderLine]) -> Result<()> {
        let frozen = frozen_map(frozen)?;
        let nodes = self.node_map()?;
        self.validate_root(&nodes, &frozen)?;
        for node in &self.nodes {
            self.validate_node(node, &nodes, &frozen)?;
        }
        Ok(())
    }

    /// 读取当前已经由来源证明的叶子节点。
    /// # 参数
    /// 无；调用方须先 validate 后用于资金或履约操作。
    /// # 返回
    /// 当前 Leaf 集合，不包含历史父单或未核实节点。
    /// # 错误
    /// 不返回错误。
    pub fn leaves(&self) -> Vec<&OrderNode> {
        self.nodes.iter().filter(|node| node.role == OrderNodeRole::Leaf).collect()
    }

    /// 读取当前有明确协议依据的确认及取消目标。
    /// # 参数
    /// 无；调用方须先完成完整拓扑及数量校验。
    /// # 返回
    /// 当前叶子，或尚未拆分的预占根单；历史父单不参与操作。
    /// # 错误
    /// 不返回错误；预占根单不能作为实际物流叶子使用。
    pub fn action_targets(&self) -> Vec<&OrderNode> {
        self.nodes.iter().filter(|node| node.is_action_target()).collect()
    }

    /// 合并完整新拓扑，保留根单和历次拆单身份。
    /// # 参数
    /// `next` 为完整新观察，`frozen` 为原批准行。
    /// # 返回
    /// 校验成功的新树；相同版本仅允许完全相同载荷。
    /// # 错误
    /// 删除历史节点、更换父级、改变数量、父节点复活或版本回退时拒绝。
    pub fn merge(&self, mut next: Self, frozen: &[OrderLine]) -> Result<Self> {
        preserve_unreported_tracks(self, &mut next);
        next.validate(frozen)?;

        if next.root_order_no != self.root_order_no || next.version < self.version {
            return Err(Error::ConflictError("供应商根单或订单关系版本不一致".into()));
        }
        if next.version == self.version {
            return if next == *self {
                Ok(next)
            } else {
                Err(Error::ConflictError("同一订单关系版本返回不同内容".into()))
            };
        }

        let incoming = next.node_map()?;
        for old in &self.nodes {
            let current = incoming
                .get(old.external_order_no.as_str())
                .ok_or_else(|| Error::ConflictError("新订单关系缺失历史供应商订单".into()))?;
            validate_history(old, current)?;
        }
        Ok(next)
    }

    fn node_map(&self) -> Result<BTreeMap<&str, &OrderNode>> {
        if self.root_order_no.trim().is_empty() || self.version == 0 || self.nodes.is_empty() {
            return Err(Error::ValidationError("供应商订单关系尚不完整".into()));
        }

        let mut nodes = BTreeMap::new();
        for node in &self.nodes {
            if node.external_order_no.trim().is_empty()
                || nodes.insert(node.external_order_no.as_str(), node).is_some()
            {
                return Err(Error::ValidationError("供应商订单号为空或重复".into()));
            }
        }
        Ok(nodes)
    }

    fn validate_root(
        &self,
        nodes: &BTreeMap<&str, &OrderNode>,
        frozen: &BTreeMap<&str, &OrderLine>,
    ) -> Result<()> {
        let root = nodes
            .get(self.root_order_no.as_str())
            .ok_or_else(|| Error::ValidationError("缺少原始供应商根单".into()))?;
        if root.parent_order_no.is_some() {
            return Err(Error::ValidationError("供应商根单不能再有父单".into()));
        }

        let quantities = line_quantities(root, frozen)?;
        let expected = frozen.iter().map(|(id, line)| ((*id).to_string(), line.quantity)).collect();
        ensure_same_quantities(&quantities, &expected)
    }

    fn validate_node(
        &self,
        node: &OrderNode,
        nodes: &BTreeMap<&str, &OrderNode>,
        frozen: &BTreeMap<&str, &OrderLine>,
    ) -> Result<()> {
        let quantities = line_quantities(node, frozen)?;
        self.ensure_connected(node, nodes)?;

        let children: Vec<_> = self
            .nodes
            .iter()
            .filter(|child| child.parent_order_no.as_deref() == Some(node.external_order_no.as_str()))
            .collect();

        match node.role {
            OrderNodeRole::ReservationRoot
                if node.external_order_no == self.root_order_no
                    && node.parent_order_no.is_none()
                    && children.is_empty()
                    && node.confirmation == Some(ConfirmationState::Pending) =>
            {
                Ok(())
            },
            OrderNodeRole::Leaf if children.is_empty() => Ok(()),
            OrderNodeRole::Parent if !children.is_empty() => {
                let mut total = BTreeMap::new();
                for child in children {
                    for (id, quantity) in line_quantities(child, frozen)? {
                        add_quantity(&mut total, id, quantity)?;
                    }
                }
                ensure_same_quantities(&quantities, &total)
            },
            _ => Err(Error::ValidationError("供应商叶子订单未完整核实".into())),
        }
    }

    fn ensure_connected(&self, node: &OrderNode, nodes: &BTreeMap<&str, &OrderNode>) -> Result<()> {
        let mut visited = BTreeSet::new();
        let mut current = node;
        loop {
            if !visited.insert(current.external_order_no.as_str()) {
                return Err(Error::ValidationError("供应商订单关系存在循环".into()));
            }
            match &current.parent_order_no {
                Some(parent) => {
                    current = nodes
                        .get(parent.as_str())
                        .ok_or_else(|| Error::ValidationError("供应商父订单缺失".into()))?;
                },
                None if current.external_order_no == self.root_order_no => return Ok(()),
                None => return Err(Error::ValidationError("供应商订单不属于原始根单".into())),
            }
        }
    }
}

impl OrderNode {
    /// 判断节点是否具备首次确认或取消的协议身份。
    /// # 参数
    /// 无；完整树必须另行通过 validate 校验。
    /// # 返回
    /// 当前叶子或明确预占根单返回真，历史父节点及未决节点返回假。
    /// # 错误
    /// 不返回错误，不证明取消已完成或资金已返还。
    pub fn is_action_target(&self) -> bool {
        matches!(self.role, OrderNodeRole::Leaf | OrderNodeRole::ReservationRoot)
    }
}

fn preserve_unreported_tracks(previous: &OrderTree, next: &mut OrderTree) {
    for node in &mut next.nodes {
        if let Some(old) = previous.nodes.iter().find(|old| old.external_order_no == node.external_order_no) {
            node.confirmation = node.confirmation.or(old.confirmation);
            node.fulfillment = node.fulfillment.or(old.fulfillment);
            node.cancellation = node.cancellation.or(old.cancellation);
            node.refund = node.refund.or(old.refund);
            node.amounts = node.amounts.clone().or_else(|| old.amounts.clone());
        }
    }
}

fn frozen_map(lines: &[OrderLine]) -> Result<BTreeMap<&str, &OrderLine>> {
    let mut values = BTreeMap::new();
    for line in lines {
        if line.line_id.trim().is_empty()
            || line.quantity.to_decimal() <= Decimal::ZERO
            || values.insert(line.line_id.as_str(), line).is_some()
        {
            return Err(Error::ValidationError("冻结采购行身份或数量无效".into()));
        }
    }
    if values.is_empty() {
        return Err(Error::ValidationError("冻结采购行不能为空".into()));
    }
    Ok(values)
}

fn line_quantities(
    node: &OrderNode,
    frozen: &BTreeMap<&str, &OrderLine>,
) -> Result<BTreeMap<String, Quantity>> {
    let mut values = BTreeMap::new();
    for line in &node.lines {
        let original = frozen
            .get(line.line_id.as_str())
            .ok_or_else(|| Error::ValidationError("供应商订单存在无归属明细".into()))?;
        if original.sku != line.sku
            || line.quantity.to_decimal() <= Decimal::ZERO
            || values.insert(line.line_id.clone(), line.quantity).is_some()
        {
            return Err(Error::ValidationError("供应商订单明细身份、数量或重复映射无效".into()));
        }
    }
    if values.is_empty() {
        return Err(Error::ValidationError("供应商订单缺少明细数量证据".into()));
    }
    Ok(values)
}

fn add_quantity(total: &mut BTreeMap<String, Quantity>, id: String, quantity: Quantity) -> Result<()> {
    let previous = total.get(&id).map(|value| value.to_decimal()).unwrap_or(Decimal::ZERO);
    let sum = previous
        .checked_add(quantity.to_decimal())
        .ok_or_else(|| Error::ValidationError("供应商订单数量溢出".into()))?;
    total.insert(id, Quantity::try_from(sum)?);
    Ok(())
}

fn ensure_same_quantities(
    left: &BTreeMap<String, Quantity>,
    right: &BTreeMap<String, Quantity>,
) -> Result<()> {
    if left != right {
        return Err(Error::ValidationError("供应商订单拆分数量与冻结采购行不守恒".into()));
    }
    Ok(())
}

fn validate_history(old: &OrderNode, current: &OrderNode) -> Result<()> {
    let same_lines =
        old.lines.len() == current.lines.len() && old.lines.iter().all(|line| current.lines.contains(line));
    if old.parent_order_no != current.parent_order_no
        || !same_lines
        || old.role == OrderNodeRole::Parent && current.role != OrderNodeRole::Parent
        || old.role != OrderNodeRole::ReservationRoot && current.role == OrderNodeRole::ReservationRoot
        || old.confirmation == Some(ConfirmationState::Confirmed)
            && current.confirmation != Some(ConfirmationState::Confirmed)
    {
        return Err(Error::ConflictError("供应商订单历史关系、数量或确认事实发生冲突".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line() -> OrderLine {
        OrderLine {
            source_purchase_terms: None,
            line_id: "line-1".into(),
            sku: SupplierSku { product_id: Some("spu:1".into()), spec_id: "sku-1".into() },
            quantity: "4".parse().unwrap(),
            approved_unit_price: "10".parse().unwrap(),
            attributes: vec![],
        }
    }

    fn node(id: &str, parent: Option<&str>, role: OrderNodeRole, quantity: &str) -> OrderNode {
        OrderNode {
            external_order_no: id.into(),
            parent_order_no: parent.map(str::to_string),
            role,
            lines: vec![OrderNodeLine {
                line_id: line().line_id,
                sku: line().sku,
                quantity: quantity.parse().unwrap(),
            }],
            confirmation: Some(ConfirmationState::Pending),
            amounts: None,
            fulfillment: None,
            cancellation: None,
            refund: None,
        }
    }

    fn split() -> OrderTree {
        OrderTree {
            root_order_no: "root".into(),
            version: 1,
            nodes: vec![
                node("root", None, OrderNodeRole::Parent, "4"),
                node("a", Some("root"), OrderNodeRole::Leaf, "1"),
                node("b", Some("root"), OrderNodeRole::Leaf, "3"),
            ],
        }
    }

    #[test]
    fn split_and_resplit_keep_history_and_conserve_frozen_quantities() {
        let tree = split();
        tree.validate(&[line()]).unwrap();
        assert_eq!(tree.leaves().len(), 2);

        let mut next = tree.clone();
        next.version = 2;
        next.nodes[2].role = OrderNodeRole::Parent;
        next.nodes.extend([
            node("b1", Some("b"), OrderNodeRole::Leaf, "1"),
            node("b2", Some("b"), OrderNodeRole::Leaf, "2"),
        ]);

        let merged = tree.merge(next, &[line()]).unwrap();
        assert_eq!(merged.nodes.len(), 5);
        assert_eq!(merged.leaves().len(), 3);
        assert_eq!(merged.root_order_no, "root");
        assert_eq!(merged.clone().merge(merged.clone(), &[line()]).unwrap(), merged);
    }

    #[test]
    fn incomplete_duplicate_or_disconnected_topology_cannot_be_used() {
        let mut unresolved = split();
        unresolved.nodes[1].role = OrderNodeRole::Unresolved;
        assert!(unresolved.validate(&[line()]).is_err());

        let mut duplicate = split();
        duplicate.nodes.push(duplicate.nodes[1].clone());
        assert!(duplicate.validate(&[line()]).is_err());

        let mut cycle = split();
        cycle.nodes[1].parent_order_no = Some("b".into());
        cycle.nodes[2].parent_order_no = Some("a".into());
        assert!(cycle.validate(&[line()]).is_err());

        let mut missing = split();
        missing.nodes.pop();
        assert!(missing.validate(&[line()]).is_err());
    }

    #[test]
    fn overquantity_wrong_sku_and_removed_history_are_rejected() {
        let mut over = split();
        over.nodes[1].lines[0].quantity = "2".parse().unwrap();
        assert!(over.validate(&[line()]).is_err());

        let mut wrong_sku = split();
        wrong_sku.nodes[1].lines[0].sku.spec_id = "unknown".into();
        assert!(wrong_sku.validate(&[line()]).is_err());

        let old = split();
        let replacement = OrderTree {
            root_order_no: "root".into(),
            version: 2,
            nodes: vec![node("root", None, OrderNodeRole::Leaf, "4")],
        };
        assert!(old.merge(replacement, &[line()]).is_err());
    }

    #[test]
    fn a_root_is_a_leaf_only_when_explicitly_proven() {
        let mut tree = OrderTree {
            root_order_no: "root".into(),
            version: 1,
            nodes: vec![node("root", None, OrderNodeRole::Unresolved, "4")],
        };
        assert!(tree.validate(&[line()]).is_err());
        tree.nodes[0].role = OrderNodeRole::Leaf;
        tree.validate(&[line()]).unwrap();
    }

    #[test]
    fn missing_independent_tracks_preserve_prior_confirmed_facts() {
        let mut original = split();
        original.nodes[1].confirmation = Some(ConfirmationState::Confirmed);
        original.nodes[1].fulfillment = Some(FulfillmentStatus::Accepted);
        original.nodes[1].cancellation = Some(CancelStatus::Canceled);

        let mut incoming = original.clone();
        incoming.version = 2;
        incoming.nodes[1].confirmation = None;
        incoming.nodes[1].fulfillment = None;
        incoming.nodes[1].cancellation = None;

        let merged = original.merge(incoming, &[line()]).unwrap();
        assert_eq!(merged.nodes[1].confirmation, Some(ConfirmationState::Confirmed));
        assert_eq!(merged.nodes[1].fulfillment, Some(FulfillmentStatus::Accepted));
        assert_eq!(merged.nodes[1].cancellation, Some(CancelStatus::Canceled));

        let mut conflicting = merged.clone();
        conflicting.version = 3;
        conflicting.nodes[1].confirmation = Some(ConfirmationState::Pending);
        assert!(merged.merge(conflicting, &[line()]).is_err());
    }
}
