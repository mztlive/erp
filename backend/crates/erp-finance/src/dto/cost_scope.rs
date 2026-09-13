//! 成本部分读取的唯一裁剪规则：金额取获授权分配，整笔字段不通过差额暴露。
use super::cost::{CostEntryView, ScopedCostEntryView};
use erp_core::money::Amount;
use std::collections::BTreeSet;

impl CostEntryView {
    /// 根据独立整笔资格及可见销售分配生成读取视图。
    ///
    /// # 返回
    /// 无整笔资格且无可见分配返回 None；部分金额保持原始分配正反事实。
    pub fn restrict(
        self,
        whole: bool,
        orders: &BTreeSet<String>,
    ) -> crate::Result<Option<ScopedCostEntryView>> {
        let allocations = self
            .allocations
            .into_iter()
            .filter(|line| whole || line.sales_order_id.as_ref().is_some_and(|id| orders.contains(id)))
            .collect::<Vec<_>>();
        if !whole && allocations.is_empty() {
            return Ok(None);
        }
        let scope_gross_amount = sum(allocations.iter().map(|line| line.allocated_gross_amount))?;
        let scope_net_amount = sum(allocations.iter().map(|line| line.allocated_net_amount))?;
        Ok(Some(ScopedCostEntryView {
            id: self.id,
            cost_type: self.cost_type,
            cost_stage: self.cost_stage,
            cost_scope: self.cost_scope,
            cost_basis: self.cost_basis,
            supplier_id: self.supplier_id,
            gross_amount: whole.then_some(self.gross_amount),
            net_amount: whole.then_some(self.net_amount),
            tax_amount: whole.then_some(self.tax_amount),
            tax_inclusion: self.tax_inclusion,
            input_tax_rate: self.input_tax_rate,
            occurred_at: self.occurred_at,
            source_fact_type: self.source_fact_type,
            source_document_id: whole.then_some(self.source_document_id),
            source_line_id: whole.then_some(self.source_line_id),
            source_version: whole.then_some(self.source_version),
            created_at: self.created_at,
            allocations,
            whole_document_access: whole,
            scope_gross_amount,
            scope_net_amount,
            access_restriction: (!whole).then_some("permission_limited"),
        }))
    }
}

/// 超大合计必须拒绝，不允许 Decimal 溢出或舍入后伪造份额。
fn sum(mut values: impl Iterator<Item = Amount>) -> crate::Result<Amount> {
    let total = values.try_fold(rust_decimal::Decimal::ZERO, |sum, value| {
        sum.checked_add(value.to_decimal())
            .ok_or_else(|| crate::Error::ValidationError("成本份额合计超出金额上限".into()))
    })?;
    Ok(Amount::try_from(total)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::cost::CostAllocationView;
    use crate::entity::cost::{CostScope, CostStage, CostType};
    use erp_core::{common::time::Instant, money::Rate};

    fn entry() -> CostEntryView {
        CostEntryView {
            id: "cost".into(),
            cost_type: CostType::Other,
            cost_stage: CostStage::Actual,
            cost_scope: CostScope::NonVoucherFulfillment,
            cost_basis: None,
            supplier_id: None,
            gross_amount: "100".parse().unwrap(),
            net_amount: "100".parse().unwrap(),
            tax_amount: Amount::zero(),
            tax_inclusion: false,
            input_tax_rate: "0".parse::<Rate>().unwrap(),
            occurred_at: Instant::from_unix_secs(1),
            created_at: 1,
            source_fact_type: "manual".into(),
            source_document_id: "private-source".into(),
            source_line_id: "private-line".into(),
            source_version: "1".into(),
            allocations: [("a", "60"), ("b", "40")]
                .into_iter()
                .map(|(id, value)| CostAllocationView {
                    id: id.into(),
                    cost_entry_id: "cost".into(),
                    sales_order_id: Some(id.into()),
                    sales_order_line_id: None,
                    allocated_gross_amount: value.parse().unwrap(),
                    allocated_net_amount: value.parse().unwrap(),
                    rounding_residual_flag: false,
                })
                .collect(),
        }
    }
    #[test]
    fn reduction_keeps_authoritative_direction_and_only_visible_allocations() {
        let mut cost = entry();
        cost.cost_stage = CostStage::Reduction;
        let view = cost
            .restrict(false, &BTreeSet::from(["a".into()]))
            .unwrap()
            .unwrap();
        assert_eq!(view.cost_stage, CostStage::Reduction);
        assert_eq!(view.scope_net_amount, "60".parse().unwrap());
        assert!(view.net_amount.is_none());
    }

    #[test]
    fn partial_read_keeps_only_sixty_and_redacts_whole_amounts_and_source() {
        let view = entry()
            .restrict(false, &BTreeSet::from(["a".into()]))
            .unwrap()
            .unwrap();
        assert_eq!(view.scope_net_amount, "60".parse().unwrap());
        assert_eq!(view.allocations.len(), 1);
        let json = serde_json::to_value(view).unwrap();
        for field in [
            "gross_amount",
            "net_amount",
            "tax_amount",
            "source_document_id",
            "source_line_id",
            "source_version",
        ] {
            assert!(json[field].is_null(), "{field} must be hidden");
        }
        assert!(!json.to_string().contains("private-source"));
    }
    #[test]
    fn whole_permission_and_empty_permission_have_different_results() {
        assert!(entry().restrict(false, &BTreeSet::new()).unwrap().is_none());
        let view = entry().restrict(true, &BTreeSet::new()).unwrap().unwrap();
        assert_eq!(view.net_amount, Some("100".parse().unwrap()));
        assert_eq!(view.allocations.len(), 2);
        assert!(view.access_restriction.is_none());
    }
}
