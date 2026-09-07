//! Red-invoice reversal plans and stable per-account delta aggregation.

use crate::entity::payable::PurchaseInvoiceAllocation;
use crate::entity::receivable::{
    AllocationAction, InvoiceDirection, RedInvoiceAllocationBasis, RedInvoiceAllocationLine,
    RedInvoiceAllocationPlan, RedInvoiceAllocationPlanError, RedInvoiceAllocationReversal,
    SalesInvoiceAllocation,
};
use crate::{Error, Result};
use erp_core::money::Amount;
use std::collections::HashMap;

/// 将销项分配事实适配为领域红票规划输入并构建计划。
///
/// # 参数
/// * `blue` - 原蓝票查询得到的全部销项分配事实
/// * `related` - 同一批账户下可能引用原分配的全部相关事实
/// * `requested` - 可选本次红冲含税金额
///
/// # 返回
/// 返回完成历史净额、顺序消费和比例税额舍入的领域计划。
///
/// # 错误
/// 领域规划失败时映射为与既有服务相同的业务、内部或金额错误。
///
/// # 约束
/// 本函数只转换持久化事实形态，不实现或复制红冲计算规则。
pub fn sales_red_invoice_allocation_plan(
    blue: &[SalesInvoiceAllocation],
    related: &[SalesInvoiceAllocation],
    requested: Option<Amount>,
) -> Result<RedInvoiceAllocationPlan> {
    let basis = sales_red_invoice_allocation_bases(blue);
    let reversals = sales_red_invoice_allocation_reversals(related);
    RedInvoiceAllocationPlan::build(InvoiceDirection::Sales, basis, &reversals, requested)
        .map_err(map_red_invoice_allocation_plan_error)
}

/// 将销项蓝票正向分配转换为领域原始基数输入。
///
/// # 参数
/// * `blue` - 原蓝票查询得到的全部销项分配事实
///
/// # 返回
/// 返回保持查询顺序的正向分配基数，非 `APPLY` 事实被忽略。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 只复制事实字段，不扣减历史红冲或执行金额计算。
fn sales_red_invoice_allocation_bases(blue: &[SalesInvoiceAllocation]) -> Vec<RedInvoiceAllocationBasis> {
    blue.iter()
        .filter(|line| line.allocation_action == AllocationAction::Apply)
        .map(|line| RedInvoiceAllocationBasis {
            original_allocation_id: line.base.id.clone(),
            account_id: line.receivable_account_id.to_string(),
            allocation_seq: line.allocation_seq,
            gross: line.allocated_gross_amount,
            net: line.allocated_net_amount,
            tax: line.allocated_tax_amount,
        })
        .collect()
}

/// 将销项历史反向分配转换为领域红冲事实输入。
///
/// # 参数
/// * `related` - 同一批应收账户下的全部相关销项分配事实
///
/// # 返回
/// 返回所有携带原分配引用的 `REVERSE` 事实，保持查询顺序。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 缺少反向引用的损坏事实沿用旧逻辑忽略，由领域计划只匹配原分配身份。
fn sales_red_invoice_allocation_reversals(
    related: &[SalesInvoiceAllocation],
) -> Vec<RedInvoiceAllocationReversal> {
    related
        .iter()
        .filter(|line| line.allocation_action == AllocationAction::Reverse)
        .filter_map(|line| {
            line.reverses_allocation_id
                .as_ref()
                .map(|original_id| RedInvoiceAllocationReversal {
                    original_allocation_id: original_id.to_string(),
                    gross: line.allocated_gross_amount,
                    net: line.allocated_net_amount,
                    tax: line.allocated_tax_amount,
                })
        })
        .collect()
}

/// 按账户聚合红票 reversal 含税增量（FIN-R11）。
///
/// 同一账户多行求和为一条 delta，保持首次出现顺序；聚合总额与计划行合计
/// 守恒。方向决策、事务与写入仍由 Service 持有，Repository 只执行返回的计划。
///
/// # 参数
/// * `lines` - 本次红票计划的反向分配行
///
/// # 返回
/// 返回按账户去重、首次出现顺序的 `(account_id, gross合计)`。
pub fn aggregate_reversal_deltas(lines: &[RedInvoiceAllocationLine]) -> Vec<(String, Amount)> {
    let mut order = Vec::new();
    let mut sums: HashMap<String, Amount> = HashMap::new();
    for line in lines {
        sums.entry(line.account_id.clone())
            .and_modify(|total| *total = total.checked_add(line.gross))
            .or_insert_with(|| {
                order.push(line.account_id.clone());
                line.gross
            });
    }
    order
        .into_iter()
        .map(|account_id| {
            let total = sums.remove(&account_id).expect("聚合账户必须存在");
            (account_id, total)
        })
        .collect()
}

/// 将进项分配事实适配为领域红票规划输入并构建计划。
///
/// # 参数
/// * `blue` - 原蓝票查询得到的全部进项分配事实
/// * `related` - 同一批账户下可能引用原分配的全部相关事实
/// * `requested` - 可选本次红冲含税金额
///
/// # 返回
/// 返回完成历史净额、顺序消费和比例税额舍入的领域计划。
///
/// # 错误
/// 领域规划失败时映射为与既有服务相同的业务、内部或金额错误。
///
/// # 约束
/// 本函数只转换 D19 持久化事实形态，不将进项实体依赖反向引入 D18 发票模型。
pub fn purchase_red_invoice_allocation_plan(
    blue: &[PurchaseInvoiceAllocation],
    related: &[PurchaseInvoiceAllocation],
    requested: Option<Amount>,
) -> Result<RedInvoiceAllocationPlan> {
    let basis = purchase_red_invoice_allocation_bases(blue);
    let reversals = purchase_red_invoice_allocation_reversals(related);
    RedInvoiceAllocationPlan::build(InvoiceDirection::Purchase, basis, &reversals, requested)
        .map_err(map_red_invoice_allocation_plan_error)
}

/// 将进项蓝票正向分配转换为领域原始基数输入。
///
/// # 参数
/// * `blue` - 原蓝票查询得到的全部进项分配事实
///
/// # 返回
/// 返回保持查询顺序的正向分配基数，非 `APPLY` 事实被忽略。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 只复制 D19 事实字段，不在 Service 内扣减历史红冲或执行金额计算。
fn purchase_red_invoice_allocation_bases(
    blue: &[PurchaseInvoiceAllocation],
) -> Vec<RedInvoiceAllocationBasis> {
    blue.iter()
        .filter(|line| line.allocation_action == crate::entity::payable::AllocationAction::Apply)
        .map(|line| RedInvoiceAllocationBasis {
            original_allocation_id: line.base.id.clone(),
            account_id: line.payable_account_id.to_string(),
            allocation_seq: line.allocation_seq,
            gross: line.allocated_gross_amount,
            net: line.allocated_net_amount,
            tax: line.allocated_tax_amount,
        })
        .collect()
}

/// 将进项历史反向分配转换为领域红冲事实输入。
///
/// # 参数
/// * `related` - 同一批应付账户下的全部相关进项分配事实
///
/// # 返回
/// 返回所有携带原分配引用的 `REVERSE` 事实，保持查询顺序。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 缺少反向引用的损坏事实沿用旧逻辑忽略，且 D19 实体不会进入 D18 领域模型。
fn purchase_red_invoice_allocation_reversals(
    related: &[PurchaseInvoiceAllocation],
) -> Vec<RedInvoiceAllocationReversal> {
    related
        .iter()
        .filter(|line| line.allocation_action == crate::entity::payable::AllocationAction::Reverse)
        .filter_map(|line| {
            line.reverses_allocation_id
                .as_ref()
                .map(|original_id| RedInvoiceAllocationReversal {
                    original_allocation_id: original_id.to_string(),
                    gross: line.allocated_gross_amount,
                    net: line.allocated_net_amount,
                    tax: line.allocated_tax_amount,
                })
        })
        .collect()
}

/// 将领域红票规划错误映射回冻结的服务错误分类和文案。
///
/// # 参数
/// * `error` - 领域计划构建失败原因
///
/// # 返回
/// 返回与迁移前相同的 `BusinessLogicError`、`Internal` 或 `Logic` 服务错误。
///
/// # 错误
/// 本函数只构造错误值，不再失败。
///
/// # 约束
/// 不解析字符串决定分类；每个领域变体显式保持既有外部错误语义。
fn map_red_invoice_allocation_plan_error(error: RedInvoiceAllocationPlanError) -> Error {
    match error {
        error @ (RedInvoiceAllocationPlanError::SalesHistoricalOverReversal
        | RedInvoiceAllocationPlanError::PurchaseHistoricalOverReversal
        | RedInvoiceAllocationPlanError::NoRemainingAllocation
        | RedInvoiceAllocationPlanError::InvalidRequestedAmount) => {
            Error::BusinessLogicError(error.to_string())
        }
        RedInvoiceAllocationPlanError::UncoveredRequest => {
            Error::Internal("红票反向分配计划未覆盖请求金额".to_string())
        }
        RedInvoiceAllocationPlanError::InvalidAmount(error) => Error::Logic(error),
    }
}

#[cfg(test)]
mod red_invoice_reversal_tests {
    use super::aggregate_reversal_deltas;
    use crate::entity::receivable::RedInvoiceAllocationLine;
    use erp_core::money::Amount;
    use std::str::FromStr;

    fn line(account: &str, gross: &str) -> RedInvoiceAllocationLine {
        RedInvoiceAllocationLine {
            original_allocation_id: format!("base-{account}-{gross}"),
            account_id: account.to_string(),
            gross: Amount::from_str(gross).unwrap(),
            net: Amount::from_str("0").unwrap(),
            tax: Amount::from_str("0").unwrap(),
        }
    }

    /// 同账户多行聚合为一条并保持首次出现顺序，总额守恒。
    #[test]
    fn same_account_lines_aggregate_with_stable_order_and_conservation() {
        let lines = [
            line("acc-a", "100.00"),
            line("acc-b", "50.00"),
            line("acc-a", "30.00"),
        ];
        let deltas = aggregate_reversal_deltas(&lines);
        assert_eq!(deltas.len(), 2);
        assert_eq!(deltas[0].0, "acc-a");
        assert_eq!(deltas[0].1, Amount::from_str("130.00").unwrap());
        assert_eq!(deltas[1].0, "acc-b");
        assert_eq!(deltas[1].1, Amount::from_str("50.00").unwrap());
        let plan_total: Amount = lines.iter().fold(Amount::from_str("0").unwrap(), |sum, line| {
            sum.checked_add(line.gross)
        });
        let delta_total = deltas
            .iter()
            .fold(Amount::from_str("0").unwrap(), |sum, (_, gross)| {
                sum.checked_add(*gross)
            });
        assert_eq!(plan_total, delta_total);
    }

    /// 空计划聚合为空，不触发写入。
    #[test]
    fn empty_plan_aggregates_to_empty() {
        assert!(aggregate_reversal_deltas(&[]).is_empty());
    }
}
