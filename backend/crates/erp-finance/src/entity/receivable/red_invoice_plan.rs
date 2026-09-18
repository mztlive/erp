//! 红票分配计划：按原蓝票分配净额、顺序消费与比例税额舍入。

use erp_core::money::{Amount, round_to_cent};
use erp_core::{Error, Result};

use super::invoice::InvoiceDirection;

/// 原蓝票单行分配的红冲计算基数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedInvoiceAllocationBasis {
    /// 原蓝票分配事实主键。
    pub original_allocation_id: String,
    /// 原分配指向的应收或应付子账。
    pub account_id: String,
    /// 原票内分配顺序。
    pub allocation_seq: u32,
    /// 原分配含税金额。
    pub gross: Amount,
    /// 原分配不含税金额。
    pub net: Amount,
    /// 原分配税额。
    pub tax: Amount,
}

/// 已过账红票对某一原蓝票分配的反向事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedInvoiceAllocationReversal {
    /// 被反向的原蓝票分配事实主键。
    pub original_allocation_id: String,
    /// 已红冲含税金额。
    pub gross: Amount,
    /// 已红冲不含税金额。
    pub net: Amount,
    /// 已红冲税额。
    pub tax: Amount,
}

/// 本次红票需要写入的一行反向分配计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedInvoiceAllocationLine {
    /// 被反向的原蓝票分配事实主键。
    pub original_allocation_id: String,
    /// 需要冲减的应收或应付子账。
    pub account_id: String,
    /// 本次红冲含税金额。
    pub gross: Amount,
    /// 本次红冲不含税金额。
    pub net: Amount,
    /// 本次红冲税额。
    pub tax: Amount,
}

/// 红票分配计划构建失败原因。
#[derive(Debug, thiserror::Error)]
pub enum RedInvoiceAllocationPlanError {
    /// 销项历史反向事实已超过原分配任一金额分量。
    #[error("销项发票历史红冲累计超过原分配")]
    SalesHistoricalOverReversal,
    /// 进项历史反向事实已超过原分配任一金额分量。
    #[error("进项发票历史红冲累计超过原分配")]
    PurchaseHistoricalOverReversal,
    /// 所有原分配都已耗尽或没有有效正数含税基数。
    #[error("原蓝票没有可红冲的有效分配")]
    NoRemainingAllocation,
    /// 请求金额不是严格正数或超过剩余可红冲总额。
    #[error("红冲金额必须大于零且不超过原蓝票剩余有效分配")]
    InvalidRequestedAmount,
    /// 计划行未能精确覆盖请求金额。
    #[error("红票反向分配计划未覆盖请求金额")]
    UncoveredRequest,
    /// 比例税额舍入后无法构造合法金额。
    #[error(transparent)]
    InvalidAmount(#[from] Error),
}

/// 已完成净额计算、顺序消费和税额舍入的红票分配计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedInvoiceAllocationPlan {
    lines: Vec<RedInvoiceAllocationLine>,
    fully_reversed: bool,
}

impl RedInvoiceAllocationPlan {
    /// 依据原蓝票基数和既有反向事实构建本次红票分配计划。
    ///
    /// 先按原分配逐行扣除历史红冲，再按 `allocation_seq` 消费请求含税金额；
    /// 部分消费行按剩余税额比例执行银行家舍入，并由含税额减税额得到净额。
    ///
    /// # 参数
    /// * `direction` - 原发票方向，仅用于保持销项与进项历史超冲错误语义
    /// * `basis` - 原蓝票所有正向分配基数
    /// * `reversals` - 已过账且引用原分配的反向事实
    /// * `requested` - 可选本次红冲含税金额；`None` 表示冲完全部剩余基数
    ///
    /// # 返回
    /// 返回按原分配顺序排列的反向行及是否冲完全部剩余金额的标记。
    ///
    /// # 错误
    /// 历史超冲、无剩余、请求金额非法、覆盖不完整或比例金额构造失败时返回错误。
    ///
    /// # 约束
    /// 不生成 ID、不读写数据库、不改变输入事实，且金额计算不使用浮点数。
    pub fn build(
        direction: InvoiceDirection,
        basis: Vec<RedInvoiceAllocationBasis>,
        reversals: &[RedInvoiceAllocationReversal],
        requested: Option<Amount>,
    ) -> std::result::Result<Self, RedInvoiceAllocationPlanError> {
        let mut remaining = remaining_red_invoice_bases(direction, basis, reversals)?;
        remaining.sort_by_key(|line| line.allocation_seq);
        remaining.retain(|line| line.gross > Amount::zero());
        let remaining_total = remaining.iter().fold(Amount::zero(), |sum, line| sum.checked_add(line.gross));
        if remaining_total <= Amount::zero() {
            return Err(RedInvoiceAllocationPlanError::NoRemainingAllocation);
        }
        let requested = validate_red_invoice_requested_amount(requested, remaining_total)?;
        let lines = build_red_invoice_allocation_lines(remaining, requested)?;
        Ok(Self { lines, fully_reversed: requested == remaining_total })
    }

    /// 返回按原分配顺序生成的只读反向分配行。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回计划内部行切片，供 Service 生成 ID 并执行持久化写入。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 调用方不能修改计划，事务与持久化职责仍属于 Service。
    pub fn lines(&self) -> &[RedInvoiceAllocationLine] {
        &self.lines
    }

    /// 判断本次计划是否耗尽原蓝票的全部剩余有效分配。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 请求金额等于构建时剩余总额时返回 `true`。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 该标记只描述构建时事实快照，不执行状态迁移。
    pub fn is_full_reversal(&self) -> bool {
        self.fully_reversed
    }

    /// 汇总计划行的含税、不含税与税额。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `(gross, net, tax)` 精确合计，供红票实体构造与幂等比较使用。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 仅做定点加法，不重新分摊或舍入。
    pub fn totals(&self) -> (Amount, Amount, Amount) {
        self.lines.iter().fold((Amount::zero(), Amount::zero(), Amount::zero()), |(gross, net, tax), line| {
            (gross.checked_add(line.gross), net.checked_add(line.net), tax.checked_add(line.tax))
        })
    }
}

/// 扣除每条原分配已有的全部反向事实，得到剩余红冲基数。
///
/// # 参数
/// * `direction` - 原发票方向，用于稳定历史超冲错误文案
/// * `basis` - 原蓝票正向分配基数
/// * `reversals` - 已过账反向事实
///
/// # 返回
/// 返回保持原输入顺序的剩余分配基数。
///
/// # 错误
/// 任一历史分量超过对应原分配时返回方向匹配的超冲错误。
///
/// # 约束
/// 未引用当前基数的反向事实被忽略，与既有服务算法一致。
fn remaining_red_invoice_bases(
    direction: InvoiceDirection,
    basis: Vec<RedInvoiceAllocationBasis>,
    reversals: &[RedInvoiceAllocationReversal],
) -> std::result::Result<Vec<RedInvoiceAllocationBasis>, RedInvoiceAllocationPlanError> {
    basis.into_iter().map(|line| remaining_red_invoice_basis(direction, line, reversals)).collect()
}

/// 计算单条原蓝票分配扣除历史红冲后的剩余金额三元组。
///
/// # 参数
/// * `direction` - 原发票方向，用于选择稳定超冲错误
/// * `line` - 待净额化的原蓝票分配
/// * `reversals` - 可能引用该原分配的全部反向事实
///
/// # 返回
/// 返回保留原身份、账户和序号的剩余分配基数。
///
/// # 错误
/// 历史含税、不含税或税额任一累计超过原分配时返回错误。
///
/// # 约束
/// 三个金额分量分别精确求和与相减，不做舍入或跨行补差。
fn remaining_red_invoice_basis(
    direction: InvoiceDirection,
    line: RedInvoiceAllocationBasis,
    reversals: &[RedInvoiceAllocationReversal],
) -> std::result::Result<RedInvoiceAllocationBasis, RedInvoiceAllocationPlanError> {
    let reversed = reversals
        .iter()
        .filter(|reversal| reversal.original_allocation_id == line.original_allocation_id)
        .fold((Amount::zero(), Amount::zero(), Amount::zero()), |(gross, net, tax), reversal| {
            (gross.checked_add(reversal.gross), net.checked_add(reversal.net), tax.checked_add(reversal.tax))
        });
    if reversed.0 > line.gross || reversed.1 > line.net || reversed.2 > line.tax {
        return Err(match direction {
            InvoiceDirection::Sales => RedInvoiceAllocationPlanError::SalesHistoricalOverReversal,
            InvoiceDirection::Purchase => RedInvoiceAllocationPlanError::PurchaseHistoricalOverReversal,
        });
    }
    Ok(RedInvoiceAllocationBasis {
        gross: line.gross.checked_sub(reversed.0),
        net: line.net.checked_sub(reversed.1),
        tax: line.tax.checked_sub(reversed.2),
        ..line
    })
}

/// 解析并校验本次红冲请求含税金额。
///
/// # 参数
/// * `requested` - 客户端可选请求金额
/// * `remaining_total` - 原蓝票剩余有效分配合计
///
/// # 返回
/// `None` 返回全部剩余金额，显式合法金额按原值返回。
///
/// # 错误
/// 请求非正或超过剩余总额时返回 `InvalidRequestedAmount`。
///
/// # 约束
/// 不舍入、不截断，也不改变 Amount 的两位小数精度。
fn validate_red_invoice_requested_amount(
    requested: Option<Amount>,
    remaining_total: Amount,
) -> std::result::Result<Amount, RedInvoiceAllocationPlanError> {
    let requested = requested.unwrap_or(remaining_total);
    if requested <= Amount::zero() || requested > remaining_total {
        return Err(RedInvoiceAllocationPlanError::InvalidRequestedAmount);
    }
    Ok(requested)
}

/// 按原分配顺序消费请求金额并生成反向分配行。
///
/// # 参数
/// * `basis` - 已排序且含税金额为正的剩余分配基数
/// * `requested` - 已验证的本次红冲含税金额
///
/// # 返回
/// 返回精确覆盖请求金额的反向分配行。
///
/// # 错误
/// 比例税额无法构造 Amount 或遍历后仍有未覆盖金额时返回错误。
///
/// # 约束
/// 完整消费行保留原净税金额；仅最后一个部分消费行执行比例银行家舍入。
fn build_red_invoice_allocation_lines(
    basis: Vec<RedInvoiceAllocationBasis>,
    requested: Amount,
) -> std::result::Result<Vec<RedInvoiceAllocationLine>, RedInvoiceAllocationPlanError> {
    let mut unplanned = requested;
    let mut lines = Vec::new();
    for line in basis {
        if unplanned == Amount::zero() {
            break;
        }
        let gross = std::cmp::min(line.gross, unplanned);
        let (net, tax) = if gross == line.gross {
            (line.net, line.tax)
        } else {
            partial_red_invoice_amounts(gross, line.gross, line.tax)?
        };
        lines.push(RedInvoiceAllocationLine {
            original_allocation_id: line.original_allocation_id,
            account_id: line.account_id,
            gross,
            net,
            tax,
        });
        unplanned = unplanned.checked_sub(gross);
    }
    if unplanned != Amount::zero() {
        return Err(RedInvoiceAllocationPlanError::UncoveredRequest);
    }
    Ok(lines)
}

/// 按原分配税额比例计算部分红冲行的净税金额。
///
/// # 参数
/// * `gross` - 本次部分消费含税金额
/// * `basis_gross` - 原行剩余含税基数
/// * `basis_tax` - 原行剩余税额基数
///
/// # 返回
/// 返回 `(net, tax)`，其中税额银行家舍入到分且净额等于含税额减税额。
///
/// # 错误
/// 舍入结果无法表示为两位小数 Amount 时返回底层金额错误。
///
/// # 约束
/// 调用前保证 `0 < gross < basis_gross`，因此除数不为零。
fn partial_red_invoice_amounts(
    gross: Amount,
    basis_gross: Amount,
    basis_tax: Amount,
) -> Result<(Amount, Amount)> {
    let tax = Amount::try_from(round_to_cent(
        gross.to_decimal() * basis_tax.to_decimal() / basis_gross.to_decimal(),
    ))?;
    Ok((gross.checked_sub(tax), tax))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    /// 构建红票规划测试使用的原蓝票分配基数。
    ///
    /// 参数指定身份、顺序与金额三元组，返回无 I/O 的纯输入；金额解析失败时
    /// 测试会 panic，且调用方负责提供满足含税等于净税之和的样本。
    fn red_basis(id: &str, seq: u32, gross: &str, net: &str, tax: &str) -> RedInvoiceAllocationBasis {
        RedInvoiceAllocationBasis {
            original_allocation_id: id.to_string(),
            account_id: format!("account-{id}"),
            allocation_seq: seq,
            gross: Amount::from_str(gross).unwrap(),
            net: Amount::from_str(net).unwrap(),
            tax: Amount::from_str(tax).unwrap(),
        }
    }

    /// 构建红票规划测试使用的历史反向事实。
    ///
    /// 参数指定原分配身份与金额三元组，返回无 I/O 的纯输入；金额解析失败时
    /// 测试会 panic，且事实只影响身份完全匹配的原分配。
    fn red_reversal(id: &str, gross: &str, net: &str, tax: &str) -> RedInvoiceAllocationReversal {
        RedInvoiceAllocationReversal {
            original_allocation_id: id.to_string(),
            gross: Amount::from_str(gross).unwrap(),
            net: Amount::from_str(net).unwrap(),
            tax: Amount::from_str(tax).unwrap(),
        }
    }

    /// 验证全额红冲按原序号生成计划并精确汇总金额三元组。
    ///
    /// 无参数且返回单元值；构造或断言失败时测试会 panic，并固定未指定请求金额
    /// 等于耗尽全部剩余分配且原输入顺序不影响分配优先级的正常行为。
    #[test]
    fn red_invoice_plan_builds_full_reversal_in_sequence_order() {
        let plan = RedInvoiceAllocationPlan::build(
            InvoiceDirection::Sales,
            vec![
                red_basis("b-2", 2, "60.00", "54.00", "6.00"),
                red_basis("b-1", 1, "40.00", "36.00", "4.00"),
            ],
            &[],
            None,
        )
        .unwrap();

        assert_eq!(plan.lines()[0].original_allocation_id, "b-1");
        assert_eq!(plan.lines()[1].original_allocation_id, "b-2");
        assert_eq!(
            plan.totals(),
            (
                Amount::from_str("100.00").unwrap(),
                Amount::from_str("90.00").unwrap(),
                Amount::from_str("10.00").unwrap(),
            )
        );
        assert!(plan.is_full_reversal());
    }

    /// 验证部分红冲先耗尽前序行，再按剩余比例拆分最后一行净税金额。
    ///
    /// 无参数且返回单元值；构造或断言失败时测试会 panic，并固定部分计划不会
    /// 标记原票全额红冲且计划含税合计精确等于请求金额。
    #[test]
    fn red_invoice_plan_builds_partial_reversal() {
        let plan = RedInvoiceAllocationPlan::build(
            InvoiceDirection::Sales,
            vec![
                red_basis("b-2", 2, "60.00", "54.00", "6.00"),
                red_basis("b-1", 1, "40.00", "36.00", "4.00"),
            ],
            &[],
            Some(Amount::from_str("50.00").unwrap()),
        )
        .unwrap();

        assert_eq!(plan.lines().len(), 2);
        assert_eq!(plan.lines()[1].gross, Amount::from_str("10.00").unwrap());
        assert_eq!(plan.lines()[1].net, Amount::from_str("9.00").unwrap());
        assert_eq!(plan.lines()[1].tax, Amount::from_str("1.00").unwrap());
        assert!(!plan.is_full_reversal());
    }

    /// 验证部分税额恰为半分时使用银行家舍入到偶数分位。
    ///
    /// 无参数且返回单元值；断言失败时测试会 panic，并固定 `0.005` 税额舍入为
    /// `0.00`、净额由含税额减去舍入税额得到的金额边界行为。
    #[test]
    fn red_invoice_plan_uses_bankers_rounding_for_partial_tax() {
        let plan = RedInvoiceAllocationPlan::build(
            InvoiceDirection::Sales,
            vec![red_basis("b-1", 1, "2.00", "1.99", "0.01")],
            &[],
            Some(Amount::from_str("1.00").unwrap()),
        )
        .unwrap();

        assert_eq!(plan.lines()[0].tax, Amount::from_str("0.00").unwrap());
        assert_eq!(plan.lines()[0].net, Amount::from_str("1.00").unwrap());
    }

    /// 验证历史事实已完全耗尽原分配时拒绝生成空红冲计划。
    ///
    /// 无参数且返回单元值；错误分类不匹配时测试会 panic，并覆盖净剩余为零的
    /// exhausted 失败边界而不依赖任何持久化组件。
    #[test]
    fn red_invoice_plan_rejects_exhausted_basis() {
        let error = RedInvoiceAllocationPlan::build(
            InvoiceDirection::Sales,
            vec![red_basis("b-1", 1, "10.00", "9.00", "1.00")],
            &[red_reversal("b-1", "10.00", "9.00", "1.00")],
            None,
        )
        .unwrap_err();

        assert!(matches!(error, RedInvoiceAllocationPlanError::NoRemainingAllocation));
    }

    /// 验证历史红冲超过原分配时保留销项与进项各自的稳定错误语义。
    ///
    /// 无参数且返回单元值；错误分类不匹配时测试会 panic，并覆盖任一金额分量
    /// 超冲都必须在计划消费前失败的财务不变量。
    #[test]
    fn red_invoice_plan_rejects_historical_over_reversal() {
        let basis = vec![red_basis("b-1", 1, "10.00", "9.00", "1.00")];
        let reversals = [red_reversal("b-1", "10.01", "9.00", "1.00")];

        let sales = RedInvoiceAllocationPlan::build(InvoiceDirection::Sales, basis.clone(), &reversals, None)
            .unwrap_err();
        assert!(matches!(sales, RedInvoiceAllocationPlanError::SalesHistoricalOverReversal));

        let purchase =
            RedInvoiceAllocationPlan::build(InvoiceDirection::Purchase, basis, &reversals, None).unwrap_err();
        assert!(matches!(purchase, RedInvoiceAllocationPlanError::PurchaseHistoricalOverReversal));
    }

    /// 验证零金额与超过剩余总额的显式请求都被同一稳定规则拒绝。
    ///
    /// 无参数且返回单元值；错误分类不匹配时测试会 panic，并覆盖请求金额下界
    /// 与上界而不改变原分配事实。
    #[test]
    fn red_invoice_plan_rejects_invalid_requested_amount_boundaries() {
        for requested in ["0.00", "10.01"] {
            let error = RedInvoiceAllocationPlan::build(
                InvoiceDirection::Sales,
                vec![red_basis("b-1", 1, "10.00", "9.00", "1.00")],
                &[],
                Some(Amount::from_str(requested).unwrap()),
            )
            .unwrap_err();
            assert!(matches!(error, RedInvoiceAllocationPlanError::InvalidRequestedAmount));
        }
    }
}
