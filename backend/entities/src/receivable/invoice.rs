//! `invoice` 发票（销项/进项统一表，数据模型 §6.8、§6.9）。
//!
//! 合同 §4.3 签署为 `NO_APPROVAL`：实体只保留业务状态，不得新增审批绑定字段
//! 或审批状态机。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::ids::{InvoiceId, PartyId};
use crate::money::{round_to_cent, Amount};
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 发票代码最大长度。
const INVOICE_CODE_MAX_LEN: usize = 32;
/// 发票号码最大长度。
const INVOICE_NO_MAX_LEN: usize = 32;
/// 尾差原因最大长度。
const ROUNDING_REASON_MAX_LEN: usize = 256;

/// 发票方向（数据模型 §6.8：销项或进项）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceDirection {
    /// 销项发票。
    Sales,
    /// 进项发票。
    Purchase,
}

impl InvoiceDirection {
    /// 返回方向的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Sales => "销项",
            Self::Purchase => "进项",
        }
    }

    /// 返回方向的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sales => "sales",
            Self::Purchase => "purchase",
        }
    }
}

/// 发票蓝红类型（数据模型 §6.8：`BLUE` 或 `RED`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceKind {
    /// 蓝票。
    Blue,
    /// 红票（冲销原蓝票）。
    Red,
}

impl InvoiceKind {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Blue => "蓝票",
            Self::Red => "红票",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Blue => "blue",
            Self::Red => "red",
        }
    }
}

/// 记账方向（数据模型 §6.8：由发票方向与蓝红类型确定）。
///
/// 销项蓝票/进项蓝票增加净已开/净收票金额，红票减少。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountingDirection {
    /// 增加。
    Increase,
    /// 减少。
    Decrease,
}

impl AccountingDirection {
    /// 返回方向的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Increase => "增加",
            Self::Decrease => "减少",
        }
    }

    /// 返回方向的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Increase => "increase",
            Self::Decrease => "decrease",
        }
    }
}

/// 发票状态（数据模型 §6.8：草稿、已登记、已红冲；第 7 章未定义发票状态机）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceStatus {
    /// 草稿。
    #[default]
    Draft,
    /// 已登记。
    Registered,
    /// 已红冲（红票过账后原蓝票置此状态）。
    RedInvoiced,
}

impl InvoiceStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::Registered => "已登记",
            Self::RedInvoiced => "已红冲",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Registered => "registered",
            Self::RedInvoiced => "red_invoiced",
        }
    }
}

/// 发票创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvoiceData {
    /// 发票方向。
    pub invoice_direction: InvoiceDirection,
    /// 蓝红类型。
    pub invoice_kind: InvoiceKind,
    /// 客户或供应商。
    pub party_id: PartyId,
    /// 发票代码（无代码数电票为空）。
    pub invoice_code: Option<String>,
    /// 发票号码。
    pub invoice_no: String,
    /// 开票日期。
    pub invoice_date: BusinessDate,
    /// 含税金额。
    pub gross_amount: Amount,
    /// 不含税金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 发票尾差（可正可负，含原因）。
    pub rounding_adjustment_amount: Amount,
    /// 尾差原因。
    pub rounding_reason: Option<String>,
    /// 红票必填的原蓝票。
    pub original_invoice_id: Option<InvoiceId>,
}

/// 发票更新数据（仅草稿可编辑）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct InvoiceUpdate {
    /// 开票日期；`None` 表示不修改。
    pub invoice_date: Option<BusinessDate>,
    /// 含税金额；`None` 表示不修改。
    pub gross_amount: Option<Amount>,
    /// 不含税金额；`None` 表示不修改。
    pub net_amount: Option<Amount>,
    /// 税额；`None` 表示不修改。
    pub tax_amount: Option<Amount>,
    /// 发票尾差；`None` 表示不修改。
    pub rounding_adjustment_amount: Option<Amount>,
    /// 尾差原因；`None` 表示不修改，`Some("")` 清除。
    pub rounding_reason: Option<String>,
    /// 发票代码；`None` 表示不修改，`Some("")` 清除。
    pub invoice_code: Option<String>,
    /// 发票号码；`None` 表示不修改。
    pub invoice_no: Option<String>,
}

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
        remaining.retain(|line| line.gross > zero_amount());
        let remaining_total = remaining
            .iter()
            .fold(zero_amount(), |sum, line| sum.checked_add(line.gross));
        if remaining_total <= zero_amount() {
            return Err(RedInvoiceAllocationPlanError::NoRemainingAllocation);
        }
        let requested = validate_red_invoice_requested_amount(requested, remaining_total)?;
        let lines = build_red_invoice_allocation_lines(remaining, requested)?;
        Ok(Self {
            lines,
            fully_reversed: requested == remaining_total,
        })
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
        self.lines.iter().fold(
            (zero_amount(), zero_amount(), zero_amount()),
            |(gross, net, tax), line| {
                (
                    gross.checked_add(line.gross),
                    net.checked_add(line.net),
                    tax.checked_add(line.tax),
                )
            },
        )
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
    basis
        .into_iter()
        .map(|line| remaining_red_invoice_basis(direction, line, reversals))
        .collect()
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
        .fold(
            (zero_amount(), zero_amount(), zero_amount()),
            |(gross, net, tax), reversal| {
                (
                    gross.checked_add(reversal.gross),
                    net.checked_add(reversal.net),
                    tax.checked_add(reversal.tax),
                )
            },
        );
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
    if requested <= zero_amount() || requested > remaining_total {
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
        if unplanned == zero_amount() {
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
    if unplanned != zero_amount() {
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

/// 返回红票分配规划使用的固定零金额。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回可精确参与 Amount 比较与加减的 `0.00`。
///
/// # 错误
/// 不返回错误；固定常量若失效会触发不可恢复的程序错误。
///
/// # 约束
/// 不进行舍入或读取外部状态。
fn zero_amount() -> Amount {
    "0.00".parse().expect("固定零金额必须可解析")
}

/// 发票实体（主表类，数据模型 §6.8）。
///
/// 金额一律存正数；`accounting_direction` 由方向与蓝红类型派生；有代码发票按
/// `(invoice_direction, normalized_code, normalized_no)` 唯一、无代码数电票按
/// `(invoice_direction, normalized_no)` 唯一，由唯一索引保证（P3 登记事务先做
/// 规范化号码去重，§8.3）。红票新建独立发票并关联原蓝票，不覆盖原票。
/// 第 7 章未定义发票状态机，`mark_registered` / `mark_red_invoiced` 是受控状态
/// 变迁（§13.3 不发明状态机）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct Invoice {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<InvoiceStatus>,
    /// 发票方向。
    pub invoice_direction: InvoiceDirection,
    /// 蓝红类型。
    pub invoice_kind: InvoiceKind,
    /// 记账方向（派生）。
    pub accounting_direction: AccountingDirection,
    /// 客户或供应商。
    pub party_id: PartyId,
    /// 发票代码。
    pub invoice_code: Option<String>,
    /// 发票号码。
    pub invoice_no: String,
    /// 规范化发票代码（去空白转大写，去重键）。
    pub normalized_code: Option<String>,
    /// 规范化发票号码（去空白转大写，去重键）。
    pub normalized_no: String,
    /// 开票日期。
    pub invoice_date: BusinessDate,
    /// 含税金额。
    pub gross_amount: Amount,
    /// 不含税金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 发票尾差。
    pub rounding_adjustment_amount: Amount,
    /// 尾差原因。
    pub rounding_reason: Option<String>,
    /// 红票原蓝票。
    pub original_invoice_id: Option<InvoiceId>,
}

impl PartialEq for Invoice {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.invoice_direction == other.invoice_direction
            && self.invoice_kind == other.invoice_kind
            && self.accounting_direction == other.accounting_direction
            && self.party_id == other.party_id
            && self.invoice_code == other.invoice_code
            && self.invoice_no == other.invoice_no
            && self.normalized_code == other.normalized_code
            && self.normalized_no == other.normalized_no
            && self.invoice_date == other.invoice_date
            && self.gross_amount == other.gross_amount
            && self.net_amount == other.net_amount
            && self.tax_amount == other.tax_amount
            && self.rounding_adjustment_amount == other.rounding_adjustment_amount
            && self.rounding_reason == other.rounding_reason
            && self.original_invoice_id == other.original_invoice_id
    }
}

impl Eq for Invoice {}

impl Invoice {
    /// 创建发票。
    ///
    /// 完成金额恒等（gross = net + tax）、蓝红与 `original_invoice_id` 一致性
    /// （红票必填、蓝票禁填）、发票代码/号码的 trim/非空/长度校验，并派生
    /// `accounting_direction` 与规范化号码。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::InvoiceId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的发票实体（状态为草稿）。
    ///
    /// # 错误
    /// 当金额三元组不一致、蓝红与引用关系矛盾或代码/号码为空/超长时返回错误。
    pub fn new(id: InvoiceId, data: InvoiceData, created_by: impl Into<String>) -> Result<Self> {
        validate_amounts(data.gross_amount, data.net_amount, data.tax_amount)?;
        match data.invoice_kind {
            InvoiceKind::Red if data.original_invoice_id.is_none() => {
                return Err(Error::from("红票必须引用原蓝票"));
            }
            InvoiceKind::Blue if data.original_invoice_id.is_some() => {
                return Err(Error::from("蓝票不得引用原发票"));
            }
            _ => {}
        }
        let invoice_code = normalize_optional_text(data.invoice_code, "发票代码", INVOICE_CODE_MAX_LEN)?;
        let invoice_no = normalize_required_text(
            data.invoice_no,
            "发票号码不能为空",
            INVOICE_NO_MAX_LEN,
            "发票号码过长",
        )?;
        let normalized_code = invoice_code.as_ref().map(|code| code.to_uppercase());
        let rounding_reason =
            normalize_optional_text(data.rounding_reason, "尾差原因", ROUNDING_REASON_MAX_LEN)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(InvoiceStatus::Draft, created_by),
            invoice_direction: data.invoice_direction,
            invoice_kind: data.invoice_kind,
            accounting_direction: derive_accounting_direction(data.invoice_direction, data.invoice_kind),
            party_id: data.party_id,
            invoice_code: invoice_code.clone(),
            invoice_no: invoice_no.clone(),
            normalized_code,
            normalized_no: invoice_no.to_uppercase(),
            invoice_date: data.invoice_date,
            gross_amount: data.gross_amount,
            net_amount: data.net_amount,
            tax_amount: data.tax_amount,
            rounding_adjustment_amount: data.rounding_adjustment_amount,
            rounding_reason,
            original_invoice_id: data.original_invoice_id,
        })
    }

    /// 更新发票草稿。
    ///
    /// 复用 `new` 的校验规则并重新派生规范化号码；发票方向、蓝红类型、往来
    /// 主体与红票原票引用是固定字段，不允许修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当状态非草稿或金额三元组不一致/字段超长时返回错误。
    pub fn update(&mut self, update: InvoiceUpdate, updated_by: impl Into<String>) -> Result<()> {
        if self.stable.status() != InvoiceStatus::Draft {
            return Err(Error::from("已登记或已红冲的发票不可编辑"));
        }
        let gross = update.gross_amount.unwrap_or(self.gross_amount);
        let net = update.net_amount.unwrap_or(self.net_amount);
        let tax = update.tax_amount.unwrap_or(self.tax_amount);
        validate_amounts(gross, net, tax)?;
        if let Some(gross) = update.gross_amount {
            self.gross_amount = gross;
        }
        if let Some(net) = update.net_amount {
            self.net_amount = net;
        }
        if let Some(tax) = update.tax_amount {
            self.tax_amount = tax;
        }
        if let Some(date) = update.invoice_date {
            self.invoice_date = date;
        }
        if let Some(adjustment) = update.rounding_adjustment_amount {
            self.rounding_adjustment_amount = adjustment;
        }
        if let Some(reason) = update.rounding_reason {
            self.rounding_reason =
                normalize_optional_text(Some(reason), "尾差原因", ROUNDING_REASON_MAX_LEN)?;
        }
        if let Some(code) = update.invoice_code {
            self.invoice_code = normalize_optional_text(Some(code), "发票代码", INVOICE_CODE_MAX_LEN)?;
            self.normalized_code = self.invoice_code.as_ref().map(|code| code.to_uppercase());
        }
        if let Some(no) = update.invoice_no {
            self.invoice_no =
                normalize_required_text(no, "发票号码不能为空", INVOICE_NO_MAX_LEN, "发票号码过长")?;
            self.normalized_no = self.invoice_no.to_uppercase();
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 登记发票（草稿 → 已登记）。
    ///
    /// # 参数
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 登记成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态非草稿时返回错误。
    pub fn mark_registered(&mut self, updated_by: impl Into<String>) -> Result<()> {
        if self.stable.status() != InvoiceStatus::Draft {
            return Err(Error::from("只有草稿发票可以登记"));
        }
        self.stable.status = InvoiceStatus::Registered;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 标记红票已红冲（已登记 → 已红冲，仅蓝票）。
    ///
    /// # 参数
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 标记成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态非已登记或发票不是蓝票时返回错误。
    pub fn mark_red_invoiced(&mut self, updated_by: impl Into<String>) -> Result<()> {
        if self.stable.status() != InvoiceStatus::Registered {
            return Err(Error::from("只有已登记的发票可以被红冲"));
        }
        if self.invoice_kind != InvoiceKind::Blue {
            return Err(Error::from("红票本身不被再次红冲"));
        }
        self.stable.status = InvoiceStatus::RedInvoiced;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 判断发票是否已登记。
    ///
    /// # 返回
    /// 状态为 `Registered` 时返回 `true`。
    pub fn is_registered(&self) -> bool {
        self.stable.status() == InvoiceStatus::Registered
    }
}

/// 校验发票金额三元组恒等。
///
/// 规则（数据模型 §4.2）：含税金额必须精确等于不含税金额加税额，各分量非负。
///
/// # 参数
/// * `gross` - 含税金额
/// * `net` - 不含税金额
/// * `tax` - 税额
///
/// # 返回
/// 恒等成立返回 `Ok(())`。
///
/// # 错误
/// 分量非负或恒等不成立时返回错误。
fn validate_amounts(gross: Amount, net: Amount, tax: Amount) -> Result<()> {
    if gross.to_decimal().is_sign_negative()
        || net.to_decimal().is_sign_negative()
        || tax.to_decimal().is_sign_negative()
    {
        return Err(Error::from("发票金额不得为负"));
    }
    if gross != net.checked_add(tax) {
        return Err(Error::from("含税金额必须等于不含税金额加税额"));
    }
    Ok(())
}

/// 由发票方向与蓝红类型派生记账方向。
///
/// # 参数
/// * `direction` - 发票方向
/// * `kind` - 蓝红类型
///
/// # 返回
/// 蓝票为增加，红票为减少。
fn derive_accounting_direction(direction: InvoiceDirection, kind: InvoiceKind) -> AccountingDirection {
    match (direction, kind) {
        (InvoiceDirection::Sales, InvoiceKind::Blue) | (InvoiceDirection::Purchase, InvoiceKind::Blue) => {
            AccountingDirection::Increase
        }
        (InvoiceDirection::Sales, InvoiceKind::Red) | (InvoiceDirection::Purchase, InvoiceKind::Red) => {
            AccountingDirection::Decrease
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn data() -> InvoiceData {
        InvoiceData {
            invoice_direction: InvoiceDirection::Sales,
            invoice_kind: InvoiceKind::Blue,
            party_id: PartyId::new("party-1"),
            invoice_code: Some(" 1100199999 ".to_string()),
            invoice_no: " 01234567 ".to_string(),
            invoice_date: BusinessDate::from_ymd(2026, 8, 6).unwrap(),
            gross_amount: Amount::from_str("1000.00").unwrap(),
            net_amount: Amount::from_str("884.96").unwrap(),
            tax_amount: Amount::from_str("115.04").unwrap(),
            rounding_adjustment_amount: Amount::from_str("0.00").unwrap(),
            rounding_reason: None,
            original_invoice_id: None,
        }
    }

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

        assert!(matches!(
            error,
            RedInvoiceAllocationPlanError::NoRemainingAllocation
        ));
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
        assert!(matches!(
            sales,
            RedInvoiceAllocationPlanError::SalesHistoricalOverReversal
        ));

        let purchase =
            RedInvoiceAllocationPlan::build(InvoiceDirection::Purchase, basis, &reversals, None).unwrap_err();
        assert!(matches!(
            purchase,
            RedInvoiceAllocationPlanError::PurchaseHistoricalOverReversal
        ));
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
            assert!(matches!(
                error,
                RedInvoiceAllocationPlanError::InvalidRequestedAmount
            ));
        }
    }

    #[test]
    fn new_normalizes_and_derives_accounting_direction() {
        let invoice = Invoice::new(InvoiceId::new("inv-1"), data(), "admin-1").unwrap();

        assert_eq!(invoice.invoice_no, "01234567");
        assert_eq!(invoice.invoice_code.as_deref(), Some("1100199999"));
        assert_eq!(invoice.normalized_no, "01234567");
        assert_eq!(invoice.normalized_code.as_deref(), Some("1100199999"));
        assert_eq!(invoice.accounting_direction, AccountingDirection::Increase);
        assert_eq!(invoice.stable.status(), InvoiceStatus::Draft);
        assert!(!invoice.is_registered());
    }

    #[test]
    fn new_rejects_amount_mismatch_and_red_blue_relation() {
        let mismatch = InvoiceData {
            net_amount: Amount::from_str("800.00").unwrap(),
            ..data()
        };
        assert!(Invoice::new(InvoiceId::new("inv-2"), mismatch, "admin").is_err());

        let red_without_original = InvoiceData {
            invoice_kind: InvoiceKind::Red,
            original_invoice_id: None,
            ..data()
        };
        assert!(Invoice::new(InvoiceId::new("inv-3"), red_without_original, "admin").is_err());

        let blue_with_original = InvoiceData {
            invoice_kind: InvoiceKind::Blue,
            original_invoice_id: Some(InvoiceId::new("inv-1")),
            ..data()
        };
        assert!(Invoice::new(InvoiceId::new("inv-4"), blue_with_original, "admin").is_err());

        let red_with_original = InvoiceData {
            invoice_kind: InvoiceKind::Red,
            original_invoice_id: Some(InvoiceId::new("inv-1")),
            ..data()
        };
        let red = Invoice::new(InvoiceId::new("inv-5"), red_with_original, "admin").unwrap();
        assert_eq!(red.accounting_direction, AccountingDirection::Decrease);
    }

    #[test]
    fn update_edits_draft_and_rejects_registered() {
        let mut invoice = Invoice::new(InvoiceId::new("inv-1"), data(), "admin-1").unwrap();

        invoice
            .update(
                InvoiceUpdate {
                    gross_amount: Some(Amount::from_str("2000.00").unwrap()),
                    net_amount: Some(Amount::from_str("1769.91").unwrap()),
                    tax_amount: Some(Amount::from_str("230.09").unwrap()),
                    invoice_no: Some(" 99887766 ".to_string()),
                    ..Default::default()
                },
                "admin-2",
            )
            .unwrap();
        assert_eq!(invoice.gross_amount, Amount::from_str("2000.00").unwrap());
        assert_eq!(invoice.invoice_no, "99887766");
        assert_eq!(invoice.normalized_no, "99887766");
        assert_eq!(invoice.stable.updated_by, "admin-2");

        invoice.mark_registered("admin-2").unwrap();
        assert!(invoice.is_registered());
        assert!(invoice
            .update(
                InvoiceUpdate {
                    invoice_date: Some(BusinessDate::from_ymd(2026, 8, 7).unwrap()),
                    ..Default::default()
                },
                "admin-3",
            )
            .is_err());
    }

    #[test]
    fn status_markers_are_guarded() {
        let mut blue = Invoice::new(InvoiceId::new("inv-1"), data(), "admin-1").unwrap();
        assert!(blue.mark_red_invoiced("admin").is_err(), "草稿不可直接红冲");
        blue.mark_registered("admin").unwrap();
        assert!(blue.mark_registered("admin").is_err(), "重复登记被拒");
        blue.mark_red_invoiced("admin").unwrap();
        assert_eq!(blue.stable.status(), InvoiceStatus::RedInvoiced);

        let mut red = Invoice::new(
            InvoiceId::new("inv-2"),
            InvoiceData {
                invoice_kind: InvoiceKind::Red,
                original_invoice_id: Some(InvoiceId::new("inv-1")),
                ..data()
            },
            "admin-1",
        )
        .unwrap();
        red.mark_registered("admin").unwrap();
        assert!(red.mark_red_invoiced("admin").is_err(), "红票不被再次红冲");
    }

    #[test]
    fn invoice_bson_roundtrip_preserves_fields() {
        let invoice = Invoice::new(InvoiceId::new("inv-1"), data(), "admin-1").unwrap();
        let back: Invoice =
            bson::deserialize_from_document(bson::serialize_to_document(&invoice).unwrap()).unwrap();
        assert_eq!(back, invoice);
    }

    /// 发票无审批约束：不得出现绑定字段或审批状态机。
    #[test]
    fn invoice_has_no_approval_binding_or_state_machine() {
        let invoice = Invoice::new(InvoiceId::new("inv-1"), data(), "admin-1").unwrap();
        let value = serde_json::to_value(&invoice).unwrap();
        let object = value.as_object().expect("发票序列化为对象");
        assert!(!object.contains_key("approval_binding"));
        assert!(!object.contains_key("approval_subject_version"));
        assert!(!object.contains_key("pending_allocations"));
        assert_eq!(invoice.stable.status(), InvoiceStatus::Draft);
        assert_eq!(InvoiceStatus::Draft.as_str(), "draft");
        assert_eq!(InvoiceStatus::Registered.as_str(), "registered");
        assert_eq!(InvoiceStatus::RedInvoiced.as_str(), "red_invoiced");

        let production = include_str!("invoice.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(!production.contains("IN_APPROVAL"));
        assert!(!production.contains("fn start_approval"));
        assert!(!production.contains("approval_subject_version"));
        assert!(!production.contains("ApprovalDefinitionBinding"));
        assert!(!production.contains("PENDING_REVIEW"));
    }

    #[test]
    fn enums_serialize_with_stable_codes_and_labels() {
        assert_eq!(
            serde_json::to_string(&InvoiceDirection::Purchase).unwrap(),
            "\"purchase\""
        );
        assert_eq!(serde_json::to_string(&InvoiceKind::Red).unwrap(), "\"red\"");
        assert_eq!(
            serde_json::to_string(&InvoiceStatus::RedInvoiced).unwrap(),
            "\"red_invoiced\""
        );
        assert_eq!(InvoiceDirection::Sales.label(), "销项");
        assert_eq!(InvoiceKind::Blue.label(), "蓝票");
        assert_eq!(InvoiceStatus::Registered.label(), "已登记");
        assert_eq!(AccountingDirection::Decrease.as_str(), "decrease");
    }
}
