//! 批准采购额度按冻结执行组分配；全部分配从同一修订计算，不从局部快照推算总额。

use std::collections::{BTreeMap, BTreeSet};

use erp_core::money::{Amount, Quantity};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// 同一采购修订中的已批准商品金额和数量。
#[derive(Debug, Clone)]
pub struct ApprovedGoods {
    pub purchase_line_id: String,
    pub sales_line_id: Option<String>,
    pub quantity: Quantity,
    pub amount: Amount,
}

/// 持久化执行组已经冻结的商品数量。
#[derive(Debug, Clone)]
pub struct BudgetGroup {
    pub execution_id: String,
    pub connection_id: String,
    pub lines: Vec<(String, Quantity)>,
}

/// 经采购批准的费用范围，不接受供应商新增费用作为依据。
#[derive(Debug, Clone)]
pub struct ApprovedExpense {
    pub purchase_line_id: String,
    pub expense_line_id: String,
    pub sales_line_id: Option<String>,
    pub is_freight: bool,
    pub amount: Amount,
}

/// 固定修订内按来源商品批准金额比例分配；金额均为人民币分。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetShare {
    pub execution_id: String,
    pub amount: Amount,
}

/// 运费批准来源及完整分配，供每个组核对同一额度的全部去向。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FreightAllocation {
    pub purchase_line_id: String,
    pub expense_line_id: String,
    pub sales_line_id: Option<String>,
    pub approved_amount: Amount,
    pub basis: FreightAllocationBasis,
    pub shares: Vec<BudgetShare>,
}

/// 唯一已实现的运费分配规则；稳定排序末组承担定点尾差。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FreightAllocationBasis {
    ApprovedGoodsAmount,
}

/// 执行组专属预算，实际扣费和应付核销另行核实。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovedExecutionBudget {
    pub purchase_revision_id: String,
    pub execution_id: String,
    pub approved_goods: Amount,
    pub approved_freight: Amount,
    pub approved_other: Amount,
    pub approved_total: Amount,
    pub freight_allocations: Vec<FreightAllocation>,
}

impl ApprovedExecutionBudget {
    /// 从完整采购修订与全部执行组冻结本组金额预算。
    /// # 参数
    /// `revision` 为不可变采购修订，`execution` 为当前组，商品、费用和执行组必须完整。
    /// # 返回
    /// 数量及金额守恒的本组预算和运费来源分配。
    /// # 错误
    /// 重复来源、数量不守恒、手工供给混入、跨连接或未支持收费时拒绝。
    pub fn freeze(
        revision: &str,
        execution: &str,
        goods: &[ApprovedGoods],
        expenses: &[ApprovedExpense],
        groups: &[BudgetGroup],
    ) -> Result<Self> {
        validate_sources(revision, execution, goods, expenses, groups)?;
        let approved_goods = goods_amount(execution, goods, groups)?;
        let mut allocations = Vec::new();
        let mut freight = Decimal::ZERO;
        for expense in expenses {
            let Some(allocation) = freight_allocation(execution, expense, goods, groups)? else { continue };
            let share = allocation
                .shares
                .iter()
                .find(|share| share.execution_id == execution)
                .ok_or_else(|| invalid("运费分配缺少本次执行组"))?;
            freight = add(freight, share.amount.to_decimal())?;
            allocations.push(allocation);
        }
        Ok(Self {
            purchase_revision_id: revision.into(),
            execution_id: execution.into(),
            approved_goods,
            approved_freight: amount(freight)?,
            approved_other: amount(Decimal::ZERO)?,
            approved_total: amount(add(approved_goods.to_decimal(), freight)?)?,
            freight_allocations: allocations,
        })
    }
}

fn validate_sources(
    revision: &str,
    execution: &str,
    goods: &[ApprovedGoods],
    expenses: &[ApprovedExpense],
    groups: &[BudgetGroup],
) -> Result<()> {
    if revision.trim().is_empty()
        || execution.trim().is_empty()
        || !groups.iter().any(|group| group.execution_id == execution)
        || groups.iter().map(|group| &group.execution_id).collect::<BTreeSet<_>>().len() != groups.len()
        || goods.iter().map(|good| &good.purchase_line_id).collect::<BTreeSet<_>>().len() != goods.len()
        || expenses.iter().map(|expense| &expense.expense_line_id).collect::<BTreeSet<_>>().len()
            != expenses.len()
    {
        return Err(invalid("采购预算来源或执行组身份不完整、重复"));
    }
    for group in groups {
        if group.connection_id.trim().is_empty()
            || group.lines.is_empty()
            || group.lines.iter().map(|(line, _)| line).collect::<BTreeSet<_>>().len() != group.lines.len()
            || group.lines.iter().any(|(line, quantity)| {
                quantity.to_decimal() <= Decimal::ZERO
                    || !goods.iter().any(|good| &good.purchase_line_id == line)
            })
        {
            return Err(invalid("接口预算明细不属于冻结采购或数量无效"));
        }
    }
    if goods
        .iter()
        .any(|good| good.quantity.to_decimal() <= Decimal::ZERO || good.amount.to_decimal() < Decimal::ZERO)
        || expenses.iter().any(|expense| expense.amount.to_decimal() < Decimal::ZERO)
    {
        return Err(invalid("采购批准数量或金额无效"));
    }
    Ok(())
}

fn goods_shares(good: &ApprovedGoods, groups: &[BudgetGroup]) -> Result<Vec<BudgetShare>> {
    let mut weights = BTreeMap::new();
    for group in groups {
        if let Some((_, quantity)) = group.lines.iter().find(|(line, _)| line == &good.purchase_line_id) {
            weights.insert(group.execution_id.clone(), quantity.to_decimal());
        }
    }
    let assigned = weights.values().try_fold(Decimal::ZERO, |sum, value| add(sum, *value))?;
    if assigned != good.quantity.to_decimal() {
        return Err(invalid("接口执行组未完整覆盖批准采购数量，不能分配费用"));
    }
    allocate(good.amount, &weights)
}

fn goods_amount(execution: &str, goods: &[ApprovedGoods], groups: &[BudgetGroup]) -> Result<Amount> {
    let group = groups
        .iter()
        .find(|group| group.execution_id == execution)
        .ok_or_else(|| invalid("采购预算缺少执行组"))?;
    let mut total = Decimal::ZERO;
    for good in goods.iter().filter(|good| group.lines.iter().any(|(line, _)| line == &good.purchase_line_id))
    {
        let shares = goods_shares(good, groups)?;
        let share = shares
            .iter()
            .find(|share| share.execution_id == execution)
            .ok_or_else(|| invalid("商品预算分配缺少执行组"))?;
        total = add(total, share.amount.to_decimal())?;
    }
    amount(total)
}

fn freight_allocation(
    execution: &str,
    expense: &ApprovedExpense,
    goods: &[ApprovedGoods],
    groups: &[BudgetGroup],
) -> Result<Option<FreightAllocation>> {
    let candidates: Vec<_> = goods
        .iter()
        .filter(|good| expense.sales_line_id.is_none() || good.sales_line_id == expense.sales_line_id)
        .collect();
    let relevant = groups.iter().any(|group| {
        group.execution_id == execution
            && group
                .lines
                .iter()
                .any(|(line, _)| candidates.iter().any(|good| &good.purchase_line_id == line))
    });
    if !relevant || expense.amount.to_decimal().is_zero() {
        return Ok(None);
    }
    if !expense.is_freight {
        return Err(invalid("本次接口履约涉及尚未支持的采购收费，请明确费用归属后处理"));
    }
    let weights = freight_weights(&candidates, groups)?;
    let connections: BTreeSet<_> = groups
        .iter()
        .filter(|group| weights.contains_key(&group.execution_id))
        .map(|group| &group.connection_id)
        .collect();
    if connections.len() != 1 {
        return Err(invalid("批准运费涉及多个接口连接，须先明确各连接费用预算"));
    }
    Ok(Some(FreightAllocation {
        purchase_line_id: expense.purchase_line_id.clone(),
        expense_line_id: expense.expense_line_id.clone(),
        sales_line_id: expense.sales_line_id.clone(),
        approved_amount: expense.amount,
        basis: FreightAllocationBasis::ApprovedGoodsAmount,
        shares: allocate(expense.amount, &weights)?,
    }))
}

fn freight_weights(goods: &[&ApprovedGoods], groups: &[BudgetGroup]) -> Result<BTreeMap<String, Decimal>> {
    let mut weights = BTreeMap::new();
    for good in goods {
        for share in goods_shares(good, groups)? {
            let previous = weights.get(&share.execution_id).copied().unwrap_or(Decimal::ZERO);
            weights.insert(share.execution_id, add(previous, share.amount.to_decimal())?);
        }
    }
    Ok(weights)
}

fn allocate(total: Amount, weights: &BTreeMap<String, Decimal>) -> Result<Vec<BudgetShare>> {
    let denominator = weights.values().try_fold(Decimal::ZERO, |sum, weight| add(sum, *weight))?;
    if denominator <= Decimal::ZERO || weights.values().any(|weight| *weight < Decimal::ZERO) {
        return Err(invalid("采购费用缺少可核验的正金额分配依据"));
    }
    let mut remaining = total.to_decimal();
    let mut shares = Vec::new();
    let last_positive = weights.iter().rev().find(|(_, weight)| **weight > Decimal::ZERO).map(|(id, _)| id);
    for (execution, weight) in weights {
        let value = if Some(execution) == last_positive {
            remaining
        } else {
            total
                .to_decimal()
                .checked_mul(*weight)
                .and_then(|value| value.checked_div(denominator))
                .ok_or_else(|| invalid("采购预算分配金额超出范围"))?
                .round_dp_with_strategy(2, RoundingStrategy::ToZero)
        };
        remaining = remaining.checked_sub(value).ok_or_else(|| invalid("采购预算尾差无效"))?;
        shares.push(BudgetShare { execution_id: execution.clone(), amount: amount(value)? });
    }
    Ok(shares)
}

fn add(first: Decimal, second: Decimal) -> Result<Decimal> {
    first.checked_add(second).ok_or_else(|| invalid("采购预算金额超出范围"))
}

fn amount(value: Decimal) -> Result<Amount> {
    Amount::try_from(value).map_err(|_| invalid("采购预算金额无效"))
}

fn invalid(message: &str) -> Error {
    Error::BusinessLogicError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goods() -> Vec<ApprovedGoods> {
        vec![ApprovedGoods {
            purchase_line_id: "p1".into(),
            sales_line_id: Some("s1".into()),
            quantity: "3".parse().unwrap(),
            amount: "0.03".parse().unwrap(),
        }]
    }
    fn groups() -> Vec<BudgetGroup> {
        ["a", "b", "c"]
            .into_iter()
            .map(|id| BudgetGroup {
                execution_id: id.into(),
                connection_id: "connection".into(),
                lines: vec![("p1".into(), "1".parse().unwrap())],
            })
            .collect()
    }
    fn expense() -> ApprovedExpense {
        ApprovedExpense {
            purchase_line_id: "fee-row".into(),
            expense_line_id: "fee".into(),
            sales_line_id: Some("s1".into()),
            is_freight: true,
            amount: "0.01".parse().unwrap(),
        }
    }
    #[test]
    fn all_groups_share_one_freight_budget_and_stable_last_group_owns_rounding() {
        let budgets: Vec<_> = ["a", "b", "c"]
            .iter()
            .map(|id| {
                ApprovedExecutionBudget::freeze("revision", id, &goods(), &[expense()], &groups()).unwrap()
            })
            .collect();
        assert_eq!(
            budgets.iter().map(|budget| budget.approved_freight.to_decimal()).sum::<Decimal>(),
            "0.01".parse::<Decimal>().unwrap()
        );
        assert_eq!(budgets[0].approved_freight, "0".parse().unwrap());
        assert_eq!(budgets[2].approved_freight, "0.01".parse().unwrap());
        assert_eq!(budgets[0].freight_allocations, budgets[2].freight_allocations);
        let mut reverse = groups();
        reverse.reverse();
        assert_eq!(
            budgets[2],
            ApprovedExecutionBudget::freeze("revision", "c", &goods(), &[expense()], &reverse).unwrap()
        );
        let restored: ApprovedExecutionBudget =
            serde_json::from_str(&serde_json::to_string(&budgets[2]).unwrap()).unwrap();
        assert_eq!(restored, budgets[2]);
    }
    #[test]
    fn manual_supply_mixed_in_fee_scope_and_cross_connection_are_blocked() {
        let mut goods = goods();
        goods.push(ApprovedGoods {
            purchase_line_id: "manual".into(),
            sales_line_id: Some("s1".into()),
            quantity: "1".parse().unwrap(),
            amount: "10".parse().unwrap(),
        });
        assert!(ApprovedExecutionBudget::freeze("r", "a", &goods, &[expense()], &groups()).is_err());
        let mut groups = groups();
        groups[1].connection_id = "other".into();
        assert!(ApprovedExecutionBudget::freeze("r", "a", &goods[..1], &[expense()], &groups).is_err());
    }
    #[test]
    fn zero_freight_does_not_expand_budget_and_wrong_scope_is_excluded() {
        let mut different = expense();
        different.sales_line_id = Some("different-sale".into());
        let budget = ApprovedExecutionBudget::freeze("r", "a", &goods(), &[different], &groups()).unwrap();
        assert_eq!(budget.approved_total, budget.approved_goods);
        let mut fee = expense();
        fee.is_freight = false;
        assert!(ApprovedExecutionBudget::freeze("r", "a", &goods(), &[fee], &groups()).is_err());
        assert!(ApprovedExecutionBudget::freeze("r", "a", &goods(), &[], &groups()[..2]).is_err());
    }
}
