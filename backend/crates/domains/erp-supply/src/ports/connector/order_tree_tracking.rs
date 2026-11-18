//! 跟踪从已验证原树继续核对冻结行数量，价格不参与此拓扑校验。

use std::collections::BTreeMap;

use erp_core::money::UnitPrice;
use rust_decimal::Decimal;

use super::order::OrderLine;
use super::order_tree::OrderTree;
use crate::order_tracking::TrackingSourceLine;
use crate::{Error, Result};

impl OrderTree {
    /// 使用跟踪冻结数量核对完整树结构；不生成订单、报价或资金依据。
    /// 当前根及全部子节点结构和数量守恒时成功。
    /// 原树缺失根、冻结行缺失/重复或数量与树不符时拒绝。
    pub fn validate_tracking(&self, sources: &[TrackingSourceLine]) -> Result<()> {
        self.validate(&self.tracking_lines(sources)?)
    }

    /// 以原树的订货身份验证新树，保留历次拆单和已确认事实。
    /// 验证通过的新树。
    /// 新树改变原 SKU、数量、根、历史节点或版本时拒绝。
    /// `next` 为本次完整新观察；`sources` 为原执行冻结数量。
    pub fn merge_tracking(&self, next: Self, sources: &[TrackingSourceLine]) -> Result<Self> {
        let lines = self.tracking_lines(sources)?;
        self.validate(&lines)?;

        let mut next = next;
        next.version = if self.nodes == next.nodes && self.root_order_no == next.root_order_no {
            self.version
        } else {
            self.version.checked_add(1).ok_or_else(|| Error::ConflictError("订单关系版本超限".into()))?
        };
        self.merge(next, &lines)
    }

    /// 比较根、父子关系及逐行映射，不将查询遗漏的独立状态轨道当成拓扑变化。
    /// 所有节点身份、角色及数量分配完全一致时返回 true。
    /// 用于比较前须分别完成 validate 校验。
    pub fn same_topology(&self, other: &Self) -> bool {
        self.root_order_no == other.root_order_no
            && self.nodes.len() == other.nodes.len()
            && self.nodes.iter().all(|node| {
                other.nodes.iter().any(|candidate| {
                    node.external_order_no == candidate.external_order_no
                        && node.parent_order_no == candidate.parent_order_no
                        && node.role == candidate.role
                        && node.lines == candidate.lines
                })
            })
    }

    fn tracking_lines(&self, sources: &[TrackingSourceLine]) -> Result<Vec<OrderLine>> {
        let root = self
            .nodes
            .iter()
            .find(|node| node.external_order_no == self.root_order_no)
            .ok_or_else(|| Error::ValidationError("供应商订单缺少原根节点".into()))?;

        let mut seen = BTreeMap::new();
        for source in sources {
            let line = root
                .lines
                .iter()
                .find(|line| line.line_id == source.fulfillment_item_id)
                .ok_or_else(|| Error::ValidationError("原订单树与冻结采购行不一致".into()))?;
            // validate/merge 仅读取 SKU 和数量，零单价不是来源报价或下单批准金额。
            let value = OrderLine {
                source_purchase_terms: None,
                line_id: source.fulfillment_item_id.clone(),
                sku: line.sku.clone(),
                quantity: source.quantity,
                approved_unit_price: UnitPrice::try_from(Decimal::ZERO)?,
                attributes: vec![],
            };
            if seen.insert(value.line_id.clone(), value).is_some() {
                return Err(Error::ValidationError("订单跟踪冻结行重复".into()));
            }
        }
        Ok(seen.into_values().collect())
    }
}
