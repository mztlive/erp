//! 采购行金额与表头汇总领域规则（proc-amounts）。
//!
//! 普通提交行与变更提交行共用同一行字典（§6.6），本模块提供类型化行输入
//! [`PurchaseLineInput`]（`SavePurchaseOrderLine` 的转换目标）、单行金额计算
//! [`PurchaseLineInput::compute_amounts`] 与表头汇总 [`compute_header_totals`]，
//! 并把类型化输入构造为 [`PurchaseOrderSubmissionLineData`] 与
//! [`PurchaseChangeSubmissionLineData`]，复用行实体的字段工厂。
//!
//! 逐行金额守恒按 §4.2 铁律 1：`gross = net + tax` 精确成立，只能经
//! [`crate::money::line_amounts`] 或 [`crate::money::round_to_cent`] 舍入；
//! 本模块不触碰任何 I/O、时钟、ID 生成器或密钥。

use crate::common::time::BusinessDate;
use crate::ids::{
    ProcurementConfirmationLineId, PurchaseChangeSubmissionId, PurchaseOrderSubmissionId, SalesOrderLineId,
    SalesOrderRevisionLineId, SalesOrderSubmissionLineId, SkuId, SkuRevisionId,
};
use crate::money::{line_amounts, round_to_cent, Amount, Quantity, Rate, UnitPrice};

use super::change_order::PurchaseChangeSubmissionLineData;
use super::purchase_submission::PurchaseOrderSubmissionLineData;
use super::types::PurchaseLineType;

/// 一行采购明细的类型化输入（`SavePurchaseOrderLine` 的转换目标）。
///
/// 字符串字段已完成类型化；快照文本保持客户端原始形态，规范化与字段归属校验仍由
/// 行实体构造（`normalize_and_validate_line`）统一完成。金额只经
/// [`compute_amounts`](Self::compute_amounts) 计算，调用方不得自行推算。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurchaseLineInput {
    /// 行类型。
    pub line_type: PurchaseLineType,
    /// 商品/服务行对应的采购二次确认分行；物流费用行为空。
    pub procurement_confirmation_line_id: Option<ProcurementConfirmationLineId>,
    /// 商品行引用的 SKU；物流费用行为空。
    pub sku_id: Option<SkuId>,
    /// 商品行引用的 SKU 版本；物流费用行为空。
    pub sku_revision_id: Option<SkuRevisionId>,
    /// 商品名称快照；物流费用行为空。
    pub product_name_snapshot: Option<String>,
    /// 规格快照；物流费用行为空。
    pub specification_snapshot: Option<String>,
    /// 基础单位数量；物流费用行为空。
    pub quantity: Option<Quantity>,
    /// 单位代码；物流费用行为空。
    pub base_unit_code: Option<String>,
    /// 含税采购单价；物流费用行为空。
    pub unit_cost_gross: Option<UnitPrice>,
    /// 进项税率（未提供按 0 计税）。
    pub input_tax_rate: Option<Rate>,
    /// 预计交期。
    pub expected_delivery_date: Option<BusinessDate>,
    /// 商品行对应的销售稳定行。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 商品行对应的销售当前版本行。
    pub sales_order_revision_line_id: Option<SalesOrderRevisionLineId>,
    /// 商品行对应的历史销售提交行；仅保留旧流程追溯。
    pub sales_order_submission_line_id: Option<SalesOrderSubmissionLineId>,
    /// 商品行对应的分配数量。
    pub allocated_quantity: Option<Quantity>,
    /// 物流费用行请求的含税金额（商品行为空；与计算后的行金额区分）。
    pub gross_amount: Option<Amount>,
}

/// 采购行金额计算失败原因。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LineAmountViolation {
    /// 商品行缺少数量。
    #[error("商品行数量不能为空")]
    MissingQuantity,
    /// 商品行缺少含税单价。
    #[error("商品行含税单价不能为空")]
    MissingUnitCostGross,
    /// 物流费用行缺少含税金额。
    #[error("物流费用行含税金额不能为空")]
    MissingGrossAmount,
}

impl PurchaseLineInput {
    /// 计算单行金额三元组。
    ///
    /// 商品行按 `line_amounts(含税单价, 数量, 税率)` 复算；物流费用行按
    /// `gross − round(gross × 税率) = net` 拆算（§4.2 铁律 4）。金额与税率均
    /// 为类型化输入，未提供进项税率时按 0 计税。
    ///
    /// # 参数
    /// 无（读自身字段）。
    ///
    /// # 返回
    /// 返回 `(gross, net, tax)` 三元组，恒等式 `gross = net + tax` 精确成立。
    ///
    /// # 错误
    /// 商品行缺少数量或含税单价、物流费用行缺少含税金额时返回对应
    /// [`LineAmountViolation`]。
    pub fn compute_amounts(&self) -> std::result::Result<(Amount, Amount, Amount), LineAmountViolation> {
        let tax_rate = self.input_tax_rate.unwrap_or_else(zero_rate);
        match self.line_type {
            PurchaseLineType::ItemService => {
                let quantity = self.quantity.ok_or(LineAmountViolation::MissingQuantity)?;
                let unit_cost = self
                    .unit_cost_gross
                    .ok_or(LineAmountViolation::MissingUnitCostGross)?;
                Ok(line_amounts(unit_cost, quantity, tax_rate))
            }
            PurchaseLineType::LogisticsFee => {
                let gross = self.gross_amount.ok_or(LineAmountViolation::MissingGrossAmount)?;
                let tax = Amount::try_from(round_to_cent(gross.to_decimal() * tax_rate.to_decimal()))
                    .expect("舍入后小数位不超过 2 位");
                let net = Amount::try_from(gross.to_decimal() - tax.to_decimal())
                    .expect("物流行净额小数位不超过 2 位");
                Ok((gross, net, tax))
            }
        }
    }

    /// 转换为采购提交行创建数据。
    ///
    /// 内部先计算金额三元组，再复用提交行字段工厂填充全部行字典字段；快照文本
    /// 规范化与字段归属校验由 `PurchaseOrderSubmissionLine::new` 完成。
    ///
    /// # 参数
    /// * `purchase_order_submission_id` - 所属提交稳定身份
    /// * `line_no` - 行号（从 1 递增）
    ///
    /// # 返回
    /// 返回已填充金额与全部字段的提交行创建数据。
    ///
    /// # 错误
    /// 商品行缺少数量或含税单价、物流费用行缺少含税金额时返回对应
    /// [`LineAmountViolation`]。
    pub fn into_submission_line_data(
        self,
        purchase_order_submission_id: PurchaseOrderSubmissionId,
        line_no: u32,
    ) -> std::result::Result<PurchaseOrderSubmissionLineData, LineAmountViolation> {
        let (gross_amount, net_amount, tax_amount) = self.compute_amounts()?;
        Ok(PurchaseOrderSubmissionLineData {
            purchase_order_submission_id,
            line_no,
            line_type: self.line_type,
            procurement_confirmation_line_id: self.procurement_confirmation_line_id,
            sku_id: self.sku_id,
            sku_revision_id: self.sku_revision_id,
            product_name_snapshot: self.product_name_snapshot,
            specification_snapshot: self.specification_snapshot,
            quantity: self.quantity,
            base_unit_code: self.base_unit_code,
            unit_cost_gross: self.unit_cost_gross,
            gross_amount,
            net_amount,
            tax_amount,
            input_tax_rate: self.input_tax_rate,
            expected_delivery_date: self.expected_delivery_date,
            sales_order_line_id: self.sales_order_line_id,
            sales_order_revision_line_id: self.sales_order_revision_line_id,
            sales_order_submission_line_id: self.sales_order_submission_line_id,
            allocated_quantity: self.allocated_quantity,
        })
    }

    /// 转换为采购变更提交行创建数据。
    ///
    /// 内部先计算金额三元组，再复用变更提交行字段工厂填充全部行字典字段；快照
    /// 文本规范化与字段归属校验由 `PurchaseChangeSubmissionLine::new` 完成。
    ///
    /// # 参数
    /// * `purchase_change_submission_id` - 所属变更提交稳定身份
    /// * `line_no` - 行号（从 1 递增）
    ///
    /// # 返回
    /// 返回已填充金额与全部字段的变更提交行创建数据。
    ///
    /// # 错误
    /// 商品行缺少数量或含税单价、物流费用行缺少含税金额时返回对应
    /// [`LineAmountViolation`]。
    pub fn into_change_submission_line_data(
        self,
        purchase_change_submission_id: PurchaseChangeSubmissionId,
        line_no: u32,
    ) -> std::result::Result<PurchaseChangeSubmissionLineData, LineAmountViolation> {
        let (gross_amount, net_amount, tax_amount) = self.compute_amounts()?;
        Ok(PurchaseChangeSubmissionLineData {
            purchase_change_submission_id,
            line_no,
            line_type: self.line_type,
            procurement_confirmation_line_id: self.procurement_confirmation_line_id,
            sku_id: self.sku_id,
            sku_revision_id: self.sku_revision_id,
            product_name_snapshot: self.product_name_snapshot,
            specification_snapshot: self.specification_snapshot,
            quantity: self.quantity,
            base_unit_code: self.base_unit_code,
            unit_cost_gross: self.unit_cost_gross,
            gross_amount,
            net_amount,
            tax_amount,
            input_tax_rate: self.input_tax_rate,
            expected_delivery_date: self.expected_delivery_date,
            sales_order_line_id: self.sales_order_line_id,
            sales_order_revision_line_id: self.sales_order_revision_line_id,
            sales_order_submission_line_id: self.sales_order_submission_line_id,
            allocated_quantity: self.allocated_quantity,
        })
    }
}

/// 汇总表头金额三元组。
///
/// 逐行计算金额后精确相加（`Amount` 加法不触发舍入），与
/// `PurchaseOrderSubmission::ensure_line_totals` 的汇总算法一致。
///
/// # 参数
/// * `lines` - 类型化行输入集合；空集合返回零三元组
///
/// # 返回
/// 返回 `(gross, net, tax)` 表头汇总。
///
/// # 错误
/// 任一行金额计算失败（缺数量/单价/物流金额）时返回对应
/// [`LineAmountViolation`]。
pub fn compute_header_totals(
    lines: &[PurchaseLineInput],
) -> std::result::Result<(Amount, Amount, Amount), LineAmountViolation> {
    let mut gross = zero_amount();
    let mut net = zero_amount();
    let mut tax = zero_amount();
    for line in lines {
        let (gross_line, net_line, tax_line) = line.compute_amounts()?;
        gross = gross.checked_add(gross_line);
        net = net.checked_add(net_line);
        tax = tax.checked_add(tax_line);
    }
    Ok((gross, net, tax))
}

/// 零金额。
fn zero_amount() -> Amount {
    Amount::try_from(rust_decimal::Decimal::ZERO).expect("零金额合法")
}

/// 零税率。
fn zero_rate() -> Rate {
    Rate::try_from(rust_decimal::Decimal::ZERO).expect("零税率合法")
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::common::time::BusinessDate;
    use crate::ids::{
        ProcurementConfirmationLineId, PurchaseChangeSubmissionId, PurchaseOrderSubmissionId,
        SalesOrderLineId, SalesOrderRevisionLineId, SalesOrderSubmissionLineId, SkuId, SkuRevisionId,
    };
    use crate::money::{Amount, Quantity, Rate, UnitPrice};

    use super::{compute_header_totals, LineAmountViolation, PurchaseLineInput};
    use crate::purchase_order::types::PurchaseLineType;

    fn goods_input() -> PurchaseLineInput {
        PurchaseLineInput {
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("pcl-1")),
            sku_id: Some(SkuId::new("sku-1")),
            sku_revision_id: Some(SkuRevisionId::new("skur-1")),
            product_name_snapshot: Some("慰问礼包".to_string()),
            specification_snapshot: Some("500g×2".to_string()),
            quantity: Some(Quantity::from_str("3.000000").unwrap()),
            base_unit_code: Some("箱".to_string()),
            unit_cost_gross: Some(UnitPrice::from_str("9.9900").unwrap()),
            input_tax_rate: Some(Rate::from_str("0.130000").unwrap()),
            expected_delivery_date: Some(BusinessDate::from_ymd(2026, 8, 6).unwrap()),
            sales_order_line_id: Some(SalesOrderLineId::new("sol-1")),
            sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new("sorl-1")),
            sales_order_submission_line_id: Some(SalesOrderSubmissionLineId::new("ssl-1")),
            allocated_quantity: Some(Quantity::from_str("3.000000").unwrap()),
            gross_amount: None,
        }
    }

    fn logistics_input() -> PurchaseLineInput {
        PurchaseLineInput {
            line_type: PurchaseLineType::LogisticsFee,
            procurement_confirmation_line_id: None,
            sku_id: None,
            sku_revision_id: None,
            product_name_snapshot: None,
            specification_snapshot: None,
            quantity: None,
            base_unit_code: None,
            unit_cost_gross: None,
            input_tax_rate: Some(Rate::from_str("0.130000").unwrap()),
            expected_delivery_date: Some(BusinessDate::from_ymd(2026, 8, 7).unwrap()),
            sales_order_line_id: None,
            sales_order_revision_line_id: None,
            sales_order_submission_line_id: None,
            allocated_quantity: None,
            gross_amount: Some(Amount::from_str("100.00").unwrap()),
        }
    }

    /// 商品行金额必须与既有样例一致（9.9900 × 3，13% 进项税）。
    #[test]
    fn goods_line_amounts_match_locked_samples() {
        let (gross, net, tax) = goods_input().compute_amounts().unwrap();
        assert_eq!(gross, Amount::from_str("29.97").unwrap());
        assert_eq!(net, Amount::from_str("26.07").unwrap());
        assert_eq!(tax, Amount::from_str("3.90").unwrap());
    }

    /// 物流费用行金额必须与既有样例一致（100.00，13% 进项税）。
    #[test]
    fn logistics_line_amounts_match_locked_samples() {
        let (gross, net, tax) = logistics_input().compute_amounts().unwrap();
        assert_eq!(gross, Amount::from_str("100.00").unwrap());
        assert_eq!(net, Amount::from_str("87.00").unwrap());
        assert_eq!(tax, Amount::from_str("13.00").unwrap());
    }

    /// 分舍入按银行家规则锁定：0.005 边界取偶数分，非边界正常舍入。
    #[test]
    fn cent_rounding_is_locked() {
        let goods = PurchaseLineInput {
            quantity: Some(Quantity::from_str("1.000000").unwrap()),
            unit_cost_gross: Some(UnitPrice::from_str("1.0050").unwrap()),
            input_tax_rate: Some(Rate::from_str("0.130000").unwrap()),
            ..goods_input()
        };
        let (gross, net, tax) = goods.compute_amounts().unwrap();
        assert_eq!(gross, Amount::from_str("1.00").unwrap());
        assert_eq!(net, Amount::from_str("0.87").unwrap());
        assert_eq!(tax, Amount::from_str("0.13").unwrap());

        let logistics = PurchaseLineInput {
            gross_amount: Some(Amount::from_str("0.05").unwrap()),
            input_tax_rate: Some(Rate::from_str("0.130000").unwrap()),
            ..logistics_input()
        };
        let (gross, net, tax) = logistics.compute_amounts().unwrap();
        assert_eq!(gross, Amount::from_str("0.05").unwrap());
        assert_eq!(net, Amount::from_str("0.04").unwrap());
        assert_eq!(tax, Amount::from_str("0.01").unwrap());

        let logistics_even = PurchaseLineInput {
            gross_amount: Some(Amount::from_str("0.10").unwrap()),
            input_tax_rate: Some(Rate::from_str("0.050000").unwrap()),
            ..logistics_input()
        };
        let (_, net, tax) = logistics_even.compute_amounts().unwrap();
        assert_eq!(net, Amount::from_str("0.10").unwrap());
        assert_eq!(tax, Amount::from_str("0.00").unwrap());
    }

    /// 商品行缺数量/单价、物流行缺金额分别返回对应违规。
    #[test]
    fn missing_required_amount_inputs_are_rejected() {
        let input = PurchaseLineInput {
            quantity: None,
            ..goods_input()
        };
        assert_eq!(
            input.compute_amounts().unwrap_err(),
            LineAmountViolation::MissingQuantity
        );

        let input = PurchaseLineInput {
            unit_cost_gross: None,
            ..goods_input()
        };
        assert_eq!(
            input.compute_amounts().unwrap_err(),
            LineAmountViolation::MissingUnitCostGross
        );

        let input = PurchaseLineInput {
            gross_amount: None,
            ..logistics_input()
        };
        assert_eq!(
            input.compute_amounts().unwrap_err(),
            LineAmountViolation::MissingGrossAmount
        );
    }

    /// 未提供进项税率时按 0 计税。
    #[test]
    fn missing_tax_rate_taxes_at_zero() {
        let input = PurchaseLineInput {
            input_tax_rate: None,
            ..goods_input()
        };
        let (gross, net, tax) = input.compute_amounts().unwrap();
        assert_eq!(gross, Amount::from_str("29.97").unwrap());
        assert_eq!(net, Amount::from_str("29.97").unwrap());
        assert_eq!(tax, Amount::from_str("0.00").unwrap());
    }

    /// 表头汇总逐行精确相加，空集合为零三元组。
    #[test]
    fn header_totals_sum_lines_and_handle_empty() {
        let (gross, net, tax) = compute_header_totals(&[goods_input(), logistics_input()]).unwrap();
        assert_eq!(gross, Amount::from_str("129.97").unwrap());
        assert_eq!(net, Amount::from_str("113.07").unwrap());
        assert_eq!(tax, Amount::from_str("16.90").unwrap());

        let (gross, net, tax) = compute_header_totals(&[]).unwrap();
        assert_eq!(gross, Amount::from_str("0.00").unwrap());
        assert_eq!(net, Amount::from_str("0.00").unwrap());
        assert_eq!(tax, Amount::from_str("0.00").unwrap());
    }

    /// 提交行数据工厂逐字段映射，行号透传且金额由领域计算。
    #[test]
    fn submission_line_data_maps_all_fields_and_computes_amounts() {
        let data = goods_input()
            .into_submission_line_data(PurchaseOrderSubmissionId::new("sub-1"), 2)
            .unwrap();
        assert_eq!(data.purchase_order_submission_id.as_ref(), "sub-1");
        assert_eq!(data.line_no, 2);
        assert_eq!(data.line_type, PurchaseLineType::ItemService);
        assert_eq!(
            data.procurement_confirmation_line_id
                .as_ref()
                .map(ToString::to_string),
            Some("pcl-1".to_string())
        );
        assert_eq!(
            data.sku_id.as_ref().map(ToString::to_string),
            Some("sku-1".to_string())
        );
        assert_eq!(data.quantity, Some(Quantity::from_str("3.000000").unwrap()));
        assert_eq!(data.gross_amount, Amount::from_str("29.97").unwrap());
        assert_eq!(data.net_amount, Amount::from_str("26.07").unwrap());
        assert_eq!(data.tax_amount, Amount::from_str("3.90").unwrap());
        assert_eq!(
            data.expected_delivery_date,
            Some(BusinessDate::from_ymd(2026, 8, 6).unwrap())
        );
    }

    /// 空白可选字段在数据工厂中原样保留为 `None`。
    #[test]
    fn blank_optional_fields_stay_none_in_line_data() {
        let data = logistics_input()
            .into_change_submission_line_data(PurchaseChangeSubmissionId::new("cs-1"), 1)
            .unwrap();
        assert_eq!(data.purchase_change_submission_id.as_ref(), "cs-1");
        assert_eq!(data.line_no, 1);
        assert_eq!(data.quantity, None);
        assert_eq!(data.unit_cost_gross, None);
        assert_eq!(data.base_unit_code, None);
        assert_eq!(data.sales_order_line_id, None);
        assert_eq!(data.allocated_quantity, None);
        assert_eq!(data.gross_amount, Amount::from_str("100.00").unwrap());
        assert_eq!(data.net_amount, Amount::from_str("87.00").unwrap());
        assert_eq!(data.tax_amount, Amount::from_str("13.00").unwrap());
    }
}
