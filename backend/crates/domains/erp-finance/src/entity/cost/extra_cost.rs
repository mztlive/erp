//! 销售单附加费用：财务凭证登记与冲减规则（§7.5.1）。
//!
//! 附加费用是为履约一张实物与服务销售单发生、由我方承担、不对应销售明细供给
//! 数量的成本。财务登记直接形成实际发生成本；冲减追加 `REDUCTION` 阶段事实，
//! 累计冲减不得超过原费用。本模块只包含无 I/O 规则，事实读取与事务由组合层负责。

use erp_core::common::time::{BUSINESS_TZ_OFFSET_SECS, BusinessDate, Instant};
use erp_core::ids::{
    CostAllocationId, CostEntryId, FileAssetId, SalesOrderId, SalesOrderLineId, SupplierAccountId,
};
use erp_core::money::{Amount, Rate, fee_amounts};
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::cost_entry::DESCRIPTION_MAX_LEN;
use super::{CostAllocation, CostAllocationData, CostEntry, CostEntryData, CostScope, CostStage, CostType};

/// 财务登记附加费用的来源事实类型。
pub const SALES_EXTRA_COST_FACT: &str = "sales_extra_cost";
/// 附加费用冲减的来源事实类型。
pub const SALES_EXTRA_COST_REDUCTION_FACT: &str = "sales_extra_cost_reduction";
/// 采购费用行确认发生的来源事实类型。
pub const PURCHASE_EXPENSE_FACT: &str = "purchase_expense";
/// 附加费用事实的来源版本；同一来源行只登记一次。
const SOURCE_VERSION: &str = "1";

/// 附加费用类别；稳定代码与成本类型一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtraCostCategory {
    /// 物流。
    Logistics,
    /// 印刷。
    Printing,
    /// 仓储。
    Storage,
    /// 配送。
    Delivery,
    /// 平台技术。
    PlatformTech,
    /// 线下服务。
    OfflineService,
    /// 返点（仅财务登记）。
    Rebate,
    /// 其他。
    Other,
}

impl ExtraCostCategory {
    /// 返回该类别计入的成本类型。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回同代码的 [`CostType`]。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn cost_type(self) -> CostType {
        match self {
            Self::Logistics => CostType::Logistics,
            Self::Printing => CostType::Printing,
            Self::Storage => CostType::Storage,
            Self::Delivery => CostType::Delivery,
            Self::PlatformTech => CostType::PlatformTech,
            Self::OfflineService => CostType::OfflineService,
            Self::Rebate => CostType::Rebate,
            Self::Other => CostType::Other,
        }
    }

    /// 由成本类型反查附加费用类别。
    ///
    /// # 参数
    /// * `cost_type` - 成本事实类型
    ///
    /// # 返回
    /// 商品成本返回 `None`，其余返回同代码类别。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn from_cost_type(cost_type: CostType) -> Option<Self> {
        Some(match cost_type {
            CostType::Product => return None,
            CostType::Logistics => Self::Logistics,
            CostType::Printing => Self::Printing,
            CostType::Storage => Self::Storage,
            CostType::Delivery => Self::Delivery,
            CostType::PlatformTech => Self::PlatformTech,
            CostType::OfflineService => Self::OfflineService,
            CostType::Rebate => Self::Rebate,
            CostType::Other => Self::Other,
        })
    }

    /// 返回中文展示名。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回与成本类型一致的中文标签。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn label(self) -> &'static str {
        self.cost_type().label()
    }
}

/// 财务登记附加费用的原始输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtraCostInput {
    /// 费用类别。
    pub category: ExtraCostCategory,
    /// 费用说明。
    pub description: String,
    /// 含税金额。
    pub gross_amount: Amount,
    /// 进项税率。
    pub input_tax_rate: Rate,
    /// 发生日期。
    pub occurred_on: BusinessDate,
    /// 收款供应商（可空）。
    pub supplier_id: Option<SupplierAccountId>,
    /// 归属销售明细（可空，空表示整张销售单）。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 凭证文件。
    pub evidence_attachment_id: FileAssetId,
}

/// 已校验的附加费用登记。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtraCostRegistration {
    input: ExtraCostInput,
}

impl ExtraCostRegistration {
    /// 校验登记输入。
    ///
    /// # 参数
    /// * `input` - 原始输入
    /// * `today` - 业务当日，发生日期不得晚于当日
    ///
    /// # 返回
    /// 返回说明已规范化的登记。
    ///
    /// # 错误
    /// 说明为空或超长、金额不为正、税率为负或不小于 1、发生日期晚于当日时返回错误。
    pub fn new(mut input: ExtraCostInput, today: BusinessDate) -> Result<Self> {
        input.description = normalize_description(input.description)?;
        ensure_positive(input.gross_amount)?;
        ensure_rate(input.input_tax_rate)?;
        ensure_not_future(input.occurred_on, today)?;
        Ok(Self { input })
    }

    /// 返回归属销售明细。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回登记时选择的销售明细，未选择时为 `None`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn sales_order_line_id(&self) -> Option<&SalesOrderLineId> {
        self.input.sales_order_line_id.as_ref()
    }

    /// 返回收款供应商。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回登记时选择的供应商。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn supplier_id(&self) -> Option<&SupplierAccountId> {
        self.input.supplier_id.as_ref()
    }

    /// 返回凭证文件。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回登记凭证文件主键。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn evidence_attachment_id(&self) -> &FileAssetId {
        &self.input.evidence_attachment_id
    }

    /// 生成实际发生成本事实及其销售单分配。
    ///
    /// # 参数
    /// * `entry_id` - 由命令身份确定的成本事实主键
    /// * `sales_order_id` - 归属销售单
    ///
    /// # 返回
    /// 返回成本事实与唯一一条分配。
    ///
    /// # 错误
    /// 成本或分配实体不变式失败时返回错误。
    pub fn into_facts(
        self,
        entry_id: CostEntryId,
        sales_order_id: SalesOrderId,
    ) -> Result<(CostEntry, CostAllocation)> {
        let input = self.input;
        let (gross, net, tax) = fee_amounts(input.gross_amount, input.input_tax_rate);
        let entry = CostEntry::new(
            entry_id.clone(),
            CostEntryData {
                cost_type: input.category.cost_type(),
                cost_stage: CostStage::Actual,
                cost_scope: CostScope::NonVoucherFulfillment,
                cost_basis: None,
                supplier_id: input.supplier_id,
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
                tax_inclusion: true,
                input_tax_rate: input.input_tax_rate,
                occurred_at: business_day_start(input.occurred_on),
                source_fact_type: SALES_EXTRA_COST_FACT.to_string(),
                source_document_id: sales_order_id.to_string(),
                source_line_id: entry_id.to_string(),
                source_version: SOURCE_VERSION.to_string(),
                adjusts_cost_entry_id: None,
                evidence_attachment_id: Some(input.evidence_attachment_id),
                description: Some(input.description),
            },
        )?;
        let allocation = allocation_for(&entry, sales_order_id, input.sales_order_line_id, gross, net)?;
        Ok((entry, allocation))
    }
}

/// 附加费用冲减的原始输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtraCostReductionInput {
    /// 冲减原因。
    pub description: String,
    /// 冲减含税金额。
    pub gross_amount: Amount,
    /// 冲减发生日期。
    pub occurred_on: BusinessDate,
    /// 凭证文件。
    pub evidence_attachment_id: FileAssetId,
}

/// 原附加费用已累计冲减的金额。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReducedAmounts {
    /// 已冲减笔数。
    pub count: usize,
    /// 已冲减含税金额。
    pub gross: Amount,
    /// 已冲减不含税金额。
    pub net: Amount,
}

impl ReducedAmounts {
    /// 汇总原费用的已有冲减事实。
    ///
    /// # 参数
    /// * `original` - 原附加费用
    /// * `entries` - 候选成本事实，只统计调整原费用的冲减
    ///
    /// # 返回
    /// 返回已冲减笔数及金额。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn of<'a>(original: &CostEntry, entries: impl IntoIterator<Item = &'a CostEntry>) -> Self {
        let mut reduced = Self { count: 0, gross: Amount::zero(), net: Amount::zero() };
        for entry in entries {
            if entry.cost_stage == CostStage::Reduction
                && entry.adjusts_cost_entry_id.as_ref().is_some_and(|id| id.as_ref() == original.base.id)
            {
                reduced.count += 1;
                reduced.gross = reduced.gross.checked_add(entry.gross_amount);
                reduced.net = reduced.net.checked_add(entry.net_amount);
            }
        }
        reduced
    }
}

/// 已校验的附加费用冲减。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtraCostReduction {
    input: ExtraCostReductionInput,
}

impl ExtraCostReduction {
    /// 校验冲减输入。
    ///
    /// # 参数
    /// * `input` - 原始输入
    /// * `today` - 业务当日
    ///
    /// # 返回
    /// 返回说明已规范化的冲减。
    ///
    /// # 错误
    /// 原因为空或超长、金额不为正或日期晚于当日时返回错误。
    pub fn new(mut input: ExtraCostReductionInput, today: BusinessDate) -> Result<Self> {
        input.description = normalize_description(input.description)?;
        ensure_positive(input.gross_amount)?;
        ensure_not_future(input.occurred_on, today)?;
        Ok(Self { input })
    }

    /// 返回凭证文件。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回冲减凭证文件主键。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn evidence_attachment_id(&self) -> &FileAssetId {
        &self.input.evidence_attachment_id
    }

    /// 对原附加费用生成冲减事实及分配。
    ///
    /// 冲减沿用原费用的类别、税率、供应商与归属；全额冲完时不含税金额取原费用剩余值，
    /// 保证原费用与全部冲减精确抵消。来源行按冲减序号编排，并发冲减由唯一索引串行化。
    ///
    /// # 参数
    /// * `entry_id` - 由命令身份确定的冲减事实主键
    /// * `original` - 原附加费用
    /// * `allocation` - 原附加费用的唯一分配
    /// * `reduced` - 原费用已累计冲减金额
    ///
    /// # 返回
    /// 返回冲减成本事实与分配。
    ///
    /// # 错误
    /// 原事实不是财务登记的实际发生附加费用、冲减金额超过剩余金额或实体不变式失败时返回错误。
    pub fn into_facts(
        self,
        entry_id: CostEntryId,
        original: &CostEntry,
        allocation: &CostAllocation,
        reduced: ReducedAmounts,
    ) -> Result<(CostEntry, CostAllocation)> {
        ensure_reducible(original)?;
        let remaining_gross = original.gross_amount.checked_sub(reduced.gross);
        if self.input.gross_amount > remaining_gross {
            return Err(Error::from(format!("冲减金额不能超过原费用剩余金额 {remaining_gross}")));
        }
        let (gross, net, tax) = if self.input.gross_amount == remaining_gross {
            let net = original.net_amount.checked_sub(reduced.net);
            (remaining_gross, net, remaining_gross.checked_sub(net))
        } else {
            fee_amounts(self.input.gross_amount, original.input_tax_rate)
        };
        let entry = CostEntry::new(
            entry_id,
            CostEntryData {
                cost_type: original.cost_type,
                cost_stage: CostStage::Reduction,
                cost_scope: original.cost_scope,
                cost_basis: None,
                supplier_id: original.supplier_id.clone(),
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
                tax_inclusion: true,
                input_tax_rate: original.input_tax_rate,
                occurred_at: business_day_start(self.input.occurred_on),
                source_fact_type: SALES_EXTRA_COST_REDUCTION_FACT.to_string(),
                source_document_id: original.source_document_id.clone(),
                source_line_id: format!("{}#{}", original.base.id, reduced.count + 1),
                source_version: SOURCE_VERSION.to_string(),
                adjusts_cost_entry_id: Some(CostEntryId::new(original.base.id.clone())),
                evidence_attachment_id: Some(self.input.evidence_attachment_id),
                description: Some(self.input.description),
            },
        )?;
        let sales_order_id = allocation.sales_order_id.clone().ok_or("附加费用缺少归属销售单")?;
        let allocation =
            allocation_for(&entry, sales_order_id, allocation.sales_order_line_id.clone(), gross, net)?;
        Ok((entry, allocation))
    }
}

/// 采购费用行确认发生时的成本来源事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurchaseExpenseCostFact {
    /// 成本事实主键。
    pub entry_id: CostEntryId,
    /// 来源采购单。
    pub purchase_order_id: String,
    /// 稳定费用行身份。
    pub expense_line_id: String,
    /// 费用类别。
    pub category: ExtraCostCategory,
    /// 费用说明。
    pub description: String,
    /// 收款供应商。
    pub supplier_id: SupplierAccountId,
    /// 含税金额。
    pub gross_amount: Amount,
    /// 不含税金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 进项税率。
    pub input_tax_rate: Rate,
    /// 发生日期。
    pub occurred_on: BusinessDate,
    /// 归属销售单。
    pub sales_order_id: SalesOrderId,
    /// 归属销售明细。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 凭证文件。
    pub evidence_attachment_id: FileAssetId,
}

impl PurchaseExpenseCostFact {
    /// 生成采购费用行的实际发生成本及销售单分配。
    ///
    /// 来源唯一键为（采购费用、采购单、费用行），同一费用行只能形成一次实际成本。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回实际发生成本与唯一分配。
    ///
    /// # 错误
    /// 返点类别（只允许财务登记）或成本、分配实体不变式失败时返回错误。
    pub fn into_facts(self) -> Result<(CostEntry, CostAllocation)> {
        if self.category == ExtraCostCategory::Rebate {
            return Err(Error::from("返点只能由财务登记"));
        }
        let entry = CostEntry::new(
            self.entry_id,
            CostEntryData {
                cost_type: self.category.cost_type(),
                cost_stage: CostStage::Actual,
                cost_scope: CostScope::NonVoucherFulfillment,
                cost_basis: None,
                supplier_id: Some(self.supplier_id),
                gross_amount: self.gross_amount,
                net_amount: self.net_amount,
                tax_amount: self.tax_amount,
                tax_inclusion: true,
                input_tax_rate: self.input_tax_rate,
                occurred_at: business_day_start(self.occurred_on),
                source_fact_type: PURCHASE_EXPENSE_FACT.to_string(),
                source_document_id: self.purchase_order_id,
                source_line_id: self.expense_line_id,
                source_version: SOURCE_VERSION.to_string(),
                adjusts_cost_entry_id: None,
                evidence_attachment_id: Some(self.evidence_attachment_id),
                description: Some(self.description),
            },
        )?;
        let (gross, net) = (entry.gross_amount, entry.net_amount);
        let allocation = allocation_for(&entry, self.sales_order_id, self.sales_order_line_id, gross, net)?;
        Ok((entry, allocation))
    }
}

/// 判断成本事实是否为财务登记的附加费用。
///
/// # 参数
/// * `entry` - 成本事实
///
/// # 返回
/// 来源为附加费用登记且阶段为实际发生时返回 `true`。
///
/// # 错误
/// 不返回错误。
pub fn is_registered_extra_cost(entry: &CostEntry) -> bool {
    entry.source_fact_type == SALES_EXTRA_COST_FACT && entry.cost_stage == CostStage::Actual
}

/// 由操作人、动作和幂等键派生稳定的成本事实主键，重试命中同一事实。
///
/// # 参数
/// * `actor_id` - 操作人
/// * `action` - 稳定动作名
/// * `idempotency_key` - 客户端幂等键
///
/// # 返回
/// 返回 `xc-` 前缀的十六进制主键。
///
/// # 错误
/// 不返回错误。
pub fn extra_cost_entry_id(actor_id: &str, action: &str, idempotency_key: &str) -> CostEntryId {
    let mut hasher = Sha256::new();
    for part in [actor_id, action, idempotency_key] {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    CostEntryId::new(format!("xc-{}", &hex::encode(hasher.finalize())[..32]))
}

/// 业务日期零点（东八区）对应的时间点。
///
/// # 参数
/// * `date` - 业务日期
///
/// # 返回
/// 返回该日东八区零点。
///
/// # 错误
/// 不返回错误。
///
/// # Panics
/// 合法日期的零点必然存在；构造失败表示日期库不变式被破坏。
pub fn business_day_start(date: BusinessDate) -> Instant {
    let midnight = date.as_naive_date().and_hms_opt(0, 0, 0).expect("合法日期必有零点");
    Instant::from_unix_secs(midnight.and_utc().timestamp() - i64::from(BUSINESS_TZ_OFFSET_SECS))
}

fn allocation_for(
    entry: &CostEntry,
    sales_order_id: SalesOrderId,
    sales_order_line_id: Option<SalesOrderLineId>,
    gross: Amount,
    net: Amount,
) -> Result<CostAllocation> {
    CostAllocation::new(
        CostAllocationId::new(format!("{}-a", entry.base.id)),
        CostAllocationData {
            cost_entry_id: CostEntryId::new(entry.base.id.clone()),
            sales_order_id: Some(sales_order_id),
            sales_order_line_id,
            allocated_gross_amount: gross,
            allocated_net_amount: net,
            rounding_residual_flag: false,
        },
    )
}

fn ensure_reducible(original: &CostEntry) -> Result<()> {
    if !is_registered_extra_cost(original) {
        return Err(Error::from("只能冲减财务登记的附加费用"));
    }
    Ok(())
}

fn normalize_description(value: String) -> Result<String> {
    normalize_required_text(value, "费用说明不能为空", DESCRIPTION_MAX_LEN, "费用说明不能超过 200 字")
}

fn ensure_positive(amount: Amount) -> Result<()> {
    if amount.to_decimal() <= rust_decimal::Decimal::ZERO {
        return Err(Error::from("金额必须大于 0"));
    }
    Ok(())
}

fn ensure_rate(rate: Rate) -> Result<()> {
    let value = rate.to_decimal();
    if value.is_sign_negative() || value >= rust_decimal::Decimal::ONE {
        return Err(Error::from("税率必须在 0 到 1 之间"));
    }
    Ok(())
}

fn ensure_not_future(date: BusinessDate, today: BusinessDate) -> Result<()> {
    if date > today {
        return Err(Error::from("发生日期不能晚于今天"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn today() -> BusinessDate {
        BusinessDate::from_ymd(2026, 10, 14).unwrap()
    }

    fn input() -> ExtraCostInput {
        ExtraCostInput {
            category: ExtraCostCategory::Logistics,
            description: "  第三方加急快递  ".into(),
            gross_amount: Amount::from_str("106.00").unwrap(),
            input_tax_rate: Rate::from_str("0.06").unwrap(),
            occurred_on: BusinessDate::from_ymd(2026, 10, 13).unwrap(),
            supplier_id: None,
            sales_order_line_id: Some(SalesOrderLineId::new("sol-1")),
            evidence_attachment_id: FileAssetId::new("file-1"),
        }
    }

    fn registered() -> (CostEntry, CostAllocation) {
        ExtraCostRegistration::new(input(), today())
            .unwrap()
            .into_facts(CostEntryId::new("xc-1"), SalesOrderId::new("so-1"))
            .unwrap()
    }

    fn reduction(gross: &str) -> ExtraCostReduction {
        ExtraCostReduction::new(
            ExtraCostReductionInput {
                description: "供应商退回部分运费".into(),
                gross_amount: Amount::from_str(gross).unwrap(),
                occurred_on: today(),
                evidence_attachment_id: FileAssetId::new("file-2"),
            },
            today(),
        )
        .unwrap()
    }

    #[test]
    fn registration_posts_actual_cost_allocated_to_order_and_line() {
        let (entry, allocation) = registered();
        assert_eq!(entry.cost_type, CostType::Logistics);
        assert_eq!(entry.cost_stage, CostStage::Actual);
        assert_eq!(entry.description.as_deref(), Some("第三方加急快递"));
        assert_eq!(entry.source_fact_type, SALES_EXTRA_COST_FACT);
        assert_eq!(entry.source_document_id, "so-1");
        assert_eq!(
            (entry.net_amount.to_string(), entry.tax_amount.to_string()),
            ("99.64".into(), "6.36".into())
        );
        assert_eq!(allocation.sales_order_id, Some(SalesOrderId::new("so-1")));
        assert_eq!(allocation.sales_order_line_id, Some(SalesOrderLineId::new("sol-1")));
        assert_eq!(allocation.allocated_net_amount, entry.net_amount);
        assert!(is_registered_extra_cost(&entry));
    }

    #[test]
    fn registration_rejects_invalid_input() {
        let blank = ExtraCostInput { description: "  ".into(), ..input() };
        assert!(ExtraCostRegistration::new(blank, today()).is_err());
        let long = ExtraCostInput { description: "费".repeat(201), ..input() };
        assert!(ExtraCostRegistration::new(long, today()).is_err());
        let zero = ExtraCostInput { gross_amount: Amount::zero(), ..input() };
        assert!(ExtraCostRegistration::new(zero, today()).is_err());
        let rate = ExtraCostInput { input_tax_rate: Rate::from_str("1").unwrap(), ..input() };
        assert!(ExtraCostRegistration::new(rate, today()).is_err());
        let future = ExtraCostInput { occurred_on: BusinessDate::from_ymd(2026, 10, 15).unwrap(), ..input() };
        assert!(ExtraCostRegistration::new(future, today()).is_err());
    }

    #[test]
    fn partial_then_full_reduction_cancels_original_exactly() {
        let (original, allocation) = registered();
        let none = ReducedAmounts::of(&original, std::iter::empty());
        let (first, first_allocation) =
            reduction("50.00").into_facts(CostEntryId::new("xc-2"), &original, &allocation, none).unwrap();
        assert_eq!(first.cost_stage, CostStage::Reduction);
        assert_eq!(first.source_line_id, "xc-1#1");
        assert_eq!(first.adjusts_cost_entry_id, Some(CostEntryId::new("xc-1")));
        assert_eq!(first_allocation.sales_order_line_id, Some(SalesOrderLineId::new("sol-1")));

        let reduced = ReducedAmounts::of(&original, [&first]);
        assert_eq!(reduced.count, 1);
        assert!(
            reduction("56.01").into_facts(CostEntryId::new("xc-3"), &original, &allocation, reduced).is_err()
        );
        let (last, _) =
            reduction("56.00").into_facts(CostEntryId::new("xc-3"), &original, &allocation, reduced).unwrap();
        assert_eq!(last.source_line_id, "xc-1#2");
        assert_eq!(first.net_amount.checked_add(last.net_amount), original.net_amount);
        assert_eq!(first.tax_amount.checked_add(last.tax_amount), original.tax_amount);
    }

    #[test]
    fn reduction_only_applies_to_registered_extra_cost() {
        let (mut original, allocation) = registered();
        original.source_fact_type = PURCHASE_EXPENSE_FACT.into();
        let none = ReducedAmounts::of(&original, std::iter::empty());
        assert!(
            reduction("1.00").into_facts(CostEntryId::new("xc-2"), &original, &allocation, none).is_err()
        );
    }

    #[test]
    fn purchase_expense_posts_actual_cost_keyed_by_fee_line() {
        let fact = PurchaseExpenseCostFact {
            entry_id: CostEntryId::new("pe-1"),
            purchase_order_id: "po-1".into(),
            expense_line_id: "exp-1".into(),
            category: ExtraCostCategory::Printing,
            description: "定制印刷".into(),
            supplier_id: SupplierAccountId::new("sup-1"),
            gross_amount: Amount::from_str("106.00").unwrap(),
            net_amount: Amount::from_str("99.64").unwrap(),
            tax_amount: Amount::from_str("6.36").unwrap(),
            input_tax_rate: Rate::from_str("0.06").unwrap(),
            occurred_on: today(),
            sales_order_id: SalesOrderId::new("so-1"),
            sales_order_line_id: None,
            evidence_attachment_id: FileAssetId::new("file-1"),
        };
        let (entry, allocation) = fact.clone().into_facts().unwrap();
        assert_eq!(entry.cost_type, CostType::Printing);
        assert_eq!(entry.cost_stage, CostStage::Actual);
        assert_eq!(
            (entry.source_fact_type.as_str(), entry.source_line_id.as_str()),
            (PURCHASE_EXPENSE_FACT, "exp-1")
        );
        assert_eq!(allocation.sales_order_id, Some(SalesOrderId::new("so-1")));
        assert!(!is_registered_extra_cost(&entry));
        let rebate = PurchaseExpenseCostFact { category: ExtraCostCategory::Rebate, ..fact };
        assert!(rebate.into_facts().is_err());
    }

    #[test]
    fn entry_id_is_stable_per_actor_action_and_key() {
        let first = extra_cost_entry_id("u-1", "register", "k-1");
        assert_eq!(first, extra_cost_entry_id("u-1", "register", "k-1"));
        assert_ne!(first, extra_cost_entry_id("u-2", "register", "k-1"));
        assert_ne!(first, extra_cost_entry_id("u-1", "reduce", "k-1"));
        assert!(first.as_ref().starts_with("xc-"));
    }

    #[test]
    fn category_maps_to_cost_type_both_ways() {
        assert_eq!(ExtraCostCategory::Rebate.cost_type(), CostType::Rebate);
        assert_eq!(ExtraCostCategory::from_cost_type(CostType::Delivery), Some(ExtraCostCategory::Delivery));
        assert_eq!(ExtraCostCategory::from_cost_type(CostType::Product), None);
        assert_eq!(business_day_start(today()).unix_secs(), 1_791_907_200);
    }
}
