//! 红字发票冲减计划，以及按账户稳定聚合的差额。

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use erp_core::money::Amount;

use crate::entity::payable::{AllocationAction as PayableAllocationAction, PurchaseInvoiceAllocation};
use crate::entity::receivable::{
    AllocationAction, InvoiceDirection, RedInvoiceAllocationBasis, RedInvoiceAllocationLine,
    RedInvoiceAllocationPlan, RedInvoiceAllocationPlanError, RedInvoiceAllocationReversal,
    SalesInvoiceAllocation,
};
use crate::{Error, Result};

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
    let basis = sales_red_invoice_allocation_bases(blue)?;
    let reversals = sales_red_invoice_allocation_reversals(related)?;
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
/// 正向分配未采集票面税额时拒绝。
///
/// # 约束
/// 只复制事实字段，不扣减历史红冲或执行金额计算。
fn sales_red_invoice_allocation_bases(
    blue: &[SalesInvoiceAllocation],
) -> Result<Vec<RedInvoiceAllocationBasis>> {
    blue.iter()
        .filter(|line| line.allocation_action == AllocationAction::Apply)
        .map(|line| {
            let (net, tax) = allocation_tax(line)?;
            Ok(RedInvoiceAllocationBasis {
                original_allocation_id: line.base.id.clone(),
                account_id: line.receivable_account_id.to_string(),
                allocation_seq: line.allocation_seq,
                gross: line.allocated_gross_amount,
                net,
                tax,
            })
        })
        .collect()
}

/// 仅将实际反向分配转换为税额已知的红冲依据。
fn sales_red_invoice_allocation_reversals(
    related: &[SalesInvoiceAllocation],
) -> Result<Vec<RedInvoiceAllocationReversal>> {
    related
        .iter()
        .filter(|line| line.allocation_action == AllocationAction::Reverse)
        .filter_map(|line| line.reverses_allocation_id.as_ref().map(|id| (line, id)))
        .map(|(line, original_id)| {
            let (net, tax) = allocation_tax(line)?;
            Ok(RedInvoiceAllocationReversal {
                original_allocation_id: original_id.to_string(),
                gross: line.allocated_gross_amount,
                net,
                tax,
            })
        })
        .collect()
}

fn allocation_tax(line: &SalesInvoiceAllocation) -> Result<(Amount, Amount)> {
    line.allocated_net_amount
        .zip(line.allocated_tax_amount)
        .ok_or_else(|| Error::BusinessLogicError("附件登记未采集票面税额，不支持票面红冲".into()))
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
///
/// # 错误
/// 不返回错误。
///
/// # Panics
/// 同一账户合计超出 `Decimal` 可表示范围时，沿用 [`Amount::checked_add`] 的 panic 语义。
pub fn aggregate_reversal_deltas(lines: &[RedInvoiceAllocationLine]) -> Vec<(String, Amount)> {
    let mut deltas: Vec<(String, Amount)> = Vec::new();
    let mut indices: HashMap<&str, usize> = HashMap::new();
    for line in lines {
        match indices.entry(line.account_id.as_str()) {
            Entry::Occupied(index) => {
                let total = &mut deltas[*index.get()].1;
                *total = total.checked_add(line.gross);
            },
            Entry::Vacant(index) => {
                index.insert(deltas.len());
                deltas.push((line.account_id.clone(), line.gross));
            },
        }
    }
    deltas
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
        .filter(|line| line.allocation_action == PayableAllocationAction::Apply)
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
        .filter(|line| line.allocation_action == PayableAllocationAction::Reverse)
        .filter_map(|line| {
            line.reverses_allocation_id.as_ref().map(|original_id| RedInvoiceAllocationReversal {
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
        },
        RedInvoiceAllocationPlanError::UncoveredRequest => {
            Error::Internal("红票反向分配计划未覆盖请求金额".to_string())
        },
        RedInvoiceAllocationPlanError::InvalidAmount(error) => Error::Logic(error),
    }
}

/// 按指定采购子账生成红票计划，保留原票其他采购的分配。
/// # 参数
/// * `blue` / `related` - 原票及相关历史反向事实。
/// * `requested` - 本次红冲金额。
/// * `account` - 指定采购应付。
/// # 返回
/// 反向分配计划。
/// # 错误
/// 目标无余额或金额超限时拒绝。
pub fn purchase_red_invoice_for_account(
    blue: &[PurchaseInvoiceAllocation],
    related: &[PurchaseInvoiceAllocation],
    requested: Option<Amount>,
    account: &str,
) -> Result<RedInvoiceAllocationPlan> {
    RedInvoiceAllocationPlan::build_for_account(
        InvoiceDirection::Purchase,
        purchase_red_invoice_allocation_bases(blue),
        &purchase_red_invoice_allocation_reversals(related),
        requested,
        account,
    )
    .map_err(map_red_invoice_allocation_plan_error)
}

#[cfg(test)]
mod red_invoice_reversal_tests {
    use std::str::FromStr;

    use application_core::ErrorClass;
    use entity_core::BaseModel;
    use erp_core::ids::{InvoiceId, PayableAccountId, PurchaseInvoiceAllocationId};
    use erp_core::money::Amount;

    use super::{
        aggregate_reversal_deltas, map_red_invoice_allocation_plan_error,
        purchase_red_invoice_allocation_plan,
    };
    use crate::Error;
    use crate::entity::payable::{AllocationAction as PayableAllocationAction, PurchaseInvoiceAllocation};
    use crate::entity::receivable::{RedInvoiceAllocationLine, RedInvoiceAllocationPlanError};

    fn purchase_allocation(
        id: &str,
        seq: u32,
        action: PayableAllocationAction,
        original: Option<&str>,
        amounts: (&str, &str, &str),
    ) -> PurchaseInvoiceAllocation {
        PurchaseInvoiceAllocation {
            base: BaseModel { id: id.into(), ..BaseModel::fake() },
            invoice_id: InvoiceId::new("invoice"),
            payable_account_id: PayableAccountId::new(format!("account-{id}")),
            allocation_seq: seq,
            allocation_action: action,
            allocated_gross_amount: amounts.0.parse().unwrap(),
            allocated_net_amount: amounts.1.parse().unwrap(),
            allocated_tax_amount: amounts.2.parse().unwrap(),
            reverses_allocation_id: original.map(PurchaseInvoiceAllocationId::new),
        }
    }

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
        let lines = [line("acc-a", "100.00"), line("acc-b", "50.00"), line("acc-a", "30.00")];
        let deltas = aggregate_reversal_deltas(&lines);
        assert_eq!(deltas.len(), 2);
        assert_eq!(deltas[0].0, "acc-a");
        assert_eq!(deltas[0].1, Amount::from_str("130.00").unwrap());
        assert_eq!(deltas[1].0, "acc-b");
        assert_eq!(deltas[1].1, Amount::from_str("50.00").unwrap());
        let plan_total: Amount =
            lines.iter().fold(Amount::from_str("0").unwrap(), |sum, line| sum.checked_add(line.gross));
        let delta_total =
            deltas.iter().fold(Amount::from_str("0").unwrap(), |sum, (_, gross)| sum.checked_add(*gross));
        assert_eq!(plan_total, delta_total);
    }

    /// 空计划聚合为空，不触发写入。
    #[test]
    fn empty_plan_aggregates_to_empty() {
        assert!(aggregate_reversal_deltas(&[]).is_empty());
    }

    /// 正反金额抵消后仍保留首次账户位置及零金额，不丢弃账户事实。
    #[test]
    fn aggregate_keeps_zero_totals_and_first_seen_order() {
        let lines = [line("b", "4.00"), line("a", "0.00"), line("b", "-4.00")];
        assert_eq!(
            aggregate_reversal_deltas(&lines),
            vec![("b".to_string(), Amount::zero()), ("a".to_string(), Amount::zero())]
        );
    }

    /// 聚合溢出沿用金额类型的显式失败，不改成截断或零值。
    #[test]
    #[should_panic(expected = "Addition overflowed")]
    fn aggregate_preserves_amount_overflow() {
        let lines = [line("a", "79228162514264337593543950335"), line("a", "1")];
        aggregate_reversal_deltas(&lines);
    }

    /// 进项规划保留动作和原分配引用筛选，再按原序号消费实际剩余金额。
    #[test]
    fn purchase_plan_filters_actions_and_unrelated_reversals_before_ordered_consumption() {
        let blue = [
            purchase_allocation("later", 2, PayableAllocationAction::Apply, None, ("100", "80", "20")),
            purchase_allocation("first", 1, PayableAllocationAction::Apply, None, ("50", "40", "10")),
            purchase_allocation(
                "ignored",
                3,
                PayableAllocationAction::Reverse,
                Some("later"),
                ("999", "0", "999"),
            ),
        ];
        let related = [
            purchase_allocation(
                "posted",
                1,
                PayableAllocationAction::Reverse,
                Some("later"),
                ("25", "20", "5"),
            ),
            purchase_allocation(
                "apply",
                2,
                PayableAllocationAction::Apply,
                Some("first"),
                ("999", "0", "999"),
            ),
            purchase_allocation("missing", 3, PayableAllocationAction::Reverse, None, ("999", "0", "999")),
            purchase_allocation(
                "other",
                4,
                PayableAllocationAction::Reverse,
                Some("unrelated"),
                ("999", "0", "999"),
            ),
        ];
        let plan =
            purchase_red_invoice_allocation_plan(&blue, &related, Some("60".parse().unwrap())).unwrap();
        assert_eq!(
            plan.lines().iter().map(|line| line.original_allocation_id.as_str()).collect::<Vec<_>>(),
            ["first", "later"]
        );
        assert_eq!(plan.lines()[0].gross, "50".parse().unwrap());
        assert_eq!(plan.lines()[1].gross, "10".parse().unwrap());
        assert_eq!(plan.totals(), ("60".parse().unwrap(), "48".parse().unwrap(), "12".parse().unwrap()));
        assert!(!plan.is_full_reversal());
    }

    /// 历史超冲先于本次金额错误返回；空基数仍明确拒绝。
    #[test]
    fn purchase_plan_preserves_historical_and_empty_basis_errors() {
        let blue =
            [purchase_allocation("base", 1, PayableAllocationAction::Apply, None, ("100", "80", "20"))];
        let related = [purchase_allocation(
            "posted",
            1,
            PayableAllocationAction::Reverse,
            Some("base"),
            ("125", "100", "25"),
        )];
        assert!(matches!(
            purchase_red_invoice_allocation_plan(&blue, &related, Some(Amount::zero())),
            Err(Error::BusinessLogicError(message)) if message == "进项发票历史红冲累计超过原分配"
        ));
        assert!(matches!(
            purchase_red_invoice_allocation_plan(&[], &[], None),
            Err(Error::BusinessLogicError(message)) if message == "原蓝票没有可红冲的有效分配"
        ));
    }

    /// 领域红票规划错误到服务错误的分类与文案对照表（转译集中前先锁定）。
    #[test]
    fn plan_error_mapping_keeps_external_class_and_message() {
        for (error, class, message) in [
            (
                RedInvoiceAllocationPlanError::SalesHistoricalOverReversal,
                ErrorClass::BusinessRule,
                "销项发票历史红冲累计超过原分配",
            ),
            (
                RedInvoiceAllocationPlanError::PurchaseHistoricalOverReversal,
                ErrorClass::BusinessRule,
                "进项发票历史红冲累计超过原分配",
            ),
            (
                RedInvoiceAllocationPlanError::NoRemainingAllocation,
                ErrorClass::BusinessRule,
                "原蓝票没有可红冲的有效分配",
            ),
            (
                RedInvoiceAllocationPlanError::InvalidRequestedAmount,
                ErrorClass::BusinessRule,
                "红冲金额必须大于零且不超过原蓝票剩余有效分配",
            ),
        ] {
            let mapped = map_red_invoice_allocation_plan_error(error);
            assert_eq!(mapped.class(), class);
            assert!(mapped.to_string().contains(message), "实际：{mapped}");
        }
        let uncovered =
            map_red_invoice_allocation_plan_error(RedInvoiceAllocationPlanError::UncoveredRequest);
        assert_eq!(uncovered.class(), ErrorClass::Internal);
        assert!(matches!(uncovered, Error::Internal(_)));
        assert!(uncovered.to_string().contains("红票反向分配计划未覆盖请求金额"));

        let invalid = map_red_invoice_allocation_plan_error(RedInvoiceAllocationPlanError::InvalidAmount(
            erp_core::Error::from("舍入溢出"),
        ));
        assert_eq!(invalid.class(), ErrorClass::Internal);
        assert!(matches!(invalid, Error::Logic(_)));
        assert!(invalid.to_string().contains("舍入溢出"));
    }
}
