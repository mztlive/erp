//! `SavePurchaseOrderLine` 的类型化转换与提交/变更提交行构造（proc-amounts）。
//!
//! 文本解析（输入形态）与按行类型的必填字段检查在 DTO 转换中按历史顺序完成，
//! 错误文案与首错优先级与旧 Service helper 逐字节一致；单行金额与表头汇总全部
//! 交给 `entities::purchase_order::line_amounts` 领域方法；本模块只保留行 ID
//! 分配、行号编排与实体构造。

use std::str::FromStr;

use entities::common::time::BusinessDate;
use entities::ids::{
    ProcurementConfirmationLineId, PurchaseChangeSubmissionId, PurchaseChangeSubmissionLineId,
    PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId, SalesOrderLineId, SalesOrderRevisionLineId,
    SalesOrderSubmissionLineId, SkuId, SkuRevisionId,
};
use entities::money::{Amount, Quantity, Rate, UnitPrice};
use entities::purchase_order::{
    compute_header_totals, LineAmountViolation, PurchaseChangeSubmissionLine, PurchaseLineInput,
    PurchaseLineType, PurchaseOrderSubmissionLine,
};
use id_generator::next_id;

use super::dto::SavePurchaseOrderLine;
use crate::errors::{Error, Result};

impl SavePurchaseOrderLine {
    /// 类型化转换为领域行输入。
    ///
    /// 按历史解析顺序完成文本类型化：先税率，再按行类型解析数量/含税单价
    /// （商品行）或含税金额（物流行），随后解析数量与单价（物流行）、预计交期与
    /// 分配数量。必填字段缺失与解析失败均返回 `ValidationError`，文案与旧
    /// Service helper 逐字节一致；金额计算不在此处进行。
    ///
    /// # 参数
    /// 无（读自身字段）。
    ///
    /// # 返回
    /// 返回字符串已类型化、金额字段仍为请求形态的 [`PurchaseLineInput`]。
    ///
    /// # 错误
    /// 税率、数量、含税单价、物流金额或业务日期非法，或商品行缺数量/含税单价、
    /// 物流行缺含税金额时返回 `ValidationError`。
    pub(super) fn to_line_input(&self) -> Result<PurchaseLineInput> {
        let input_tax_rate = parse_rate(self.input_tax_rate.as_deref())?;
        let (quantity, unit_cost_gross, gross_amount) = match self.line_type {
            PurchaseLineType::ItemService => {
                let quantity = parse_quantity(self.quantity.as_deref())?
                    .ok_or_else(|| Error::ValidationError("商品行数量不能为空".to_string()))?;
                let unit_cost = parse_unit_price(self.unit_cost_gross.as_deref())?
                    .ok_or_else(|| Error::ValidationError("商品行含税单价不能为空".to_string()))?;
                (Some(quantity), Some(unit_cost), None)
            }
            PurchaseLineType::LogisticsFee => {
                let gross = parse_amount(self.gross_amount.as_deref())?
                    .ok_or_else(|| Error::ValidationError("物流费用行含税金额不能为空".to_string()))?;
                (
                    parse_quantity(self.quantity.as_deref())?,
                    parse_unit_price(self.unit_cost_gross.as_deref())?,
                    Some(gross),
                )
            }
        };
        let expected_delivery_date = self
            .expected_delivery_date
            .as_deref()
            .map(parse_business_date)
            .transpose()?;
        let allocated_quantity = parse_quantity(self.allocated_quantity.as_deref())?;
        Ok(PurchaseLineInput {
            line_type: self.line_type,
            procurement_confirmation_line_id: self
                .procurement_confirmation_line_id
                .as_ref()
                .map(|value| ProcurementConfirmationLineId::new(value.clone())),
            sku_id: self.sku_id.as_ref().map(|value| SkuId::new(value.clone())),
            sku_revision_id: self
                .sku_revision_id
                .as_ref()
                .map(|value| SkuRevisionId::new(value.clone())),
            product_name_snapshot: self.product_name.clone(),
            specification_snapshot: self.specification.clone(),
            quantity,
            base_unit_code: self.base_unit_code.clone(),
            unit_cost_gross,
            input_tax_rate,
            expected_delivery_date,
            sales_order_line_id: self
                .sales_order_line_id
                .as_ref()
                .map(|value| SalesOrderLineId::new(value.clone())),
            sales_order_revision_line_id: self
                .sales_order_revision_line_id
                .as_ref()
                .map(|value| SalesOrderRevisionLineId::new(value.clone())),
            sales_order_submission_line_id: self
                .sales_order_submission_line_id
                .as_ref()
                .map(|value| SalesOrderSubmissionLineId::new(value.clone())),
            allocated_quantity,
            gross_amount,
        })
    }
}

/// 类型化转换一批请求行。
///
/// 保持请求行顺序；任一行转换失败立即返回，首错语义与逐行转换一致。
///
/// # 参数
/// * `lines` - 待转换的请求行
///
/// # 返回
/// 返回与输入顺序一致的类型化行输入集合。
///
/// # 错误
/// 任一行文本非法或必填字段缺失时返回 `ValidationError`。
pub(super) fn to_line_inputs(lines: &[SavePurchaseOrderLine]) -> Result<Vec<PurchaseLineInput>> {
    lines.iter().map(SavePurchaseOrderLine::to_line_input).collect()
}

/// 从类型化输入构建采购提交行。
///
/// Service 仅分配行 ID 与行号；金额由领域方法计算，行实体构造完成快照规范化、
/// 字段归属与金额三元组守恒校验。
///
/// # 参数
/// * `submission_id` - 所属提交稳定身份
/// * `inputs` - 类型化行输入集合
///
/// # 返回
/// 返回行号从 1 递增的提交行实体集合。
///
/// # 错误
/// 行金额输入非法或行实体校验失败时返回对应错误。
pub(super) fn build_submission_lines(
    submission_id: &PurchaseOrderSubmissionId,
    inputs: &[PurchaseLineInput],
) -> Result<Vec<PurchaseOrderSubmissionLine>> {
    let mut result = Vec::with_capacity(inputs.len());
    for (index, input) in inputs.iter().enumerate() {
        let data = input
            .clone()
            .into_submission_line_data(submission_id.clone(), (index + 1) as u32)
            .map_err(map_line_amount_violation)?;
        result.push(PurchaseOrderSubmissionLine::new(
            PurchaseOrderSubmissionLineId::new(next_id()),
            data,
        )?);
    }
    Ok(result)
}

/// 从类型化输入构建采购变更提交行。
///
/// Service 仅分配行 ID 与行号；金额由领域方法计算，行实体构造完成快照规范化、
/// 字段归属与金额三元组守恒校验。
///
/// # 参数
/// * `submission_id` - 所属变更提交稳定身份（字符串形态）
/// * `inputs` - 类型化行输入集合
///
/// # 返回
/// 返回行号从 1 递增的变更提交行实体集合。
///
/// # 错误
/// 行金额输入非法或行实体校验失败时返回对应错误。
pub(super) fn build_change_submission_lines(
    submission_id: &str,
    inputs: &[PurchaseLineInput],
) -> Result<Vec<PurchaseChangeSubmissionLine>> {
    let mut result = Vec::with_capacity(inputs.len());
    for (index, input) in inputs.iter().enumerate() {
        let data = input
            .clone()
            .into_change_submission_line_data(
                PurchaseChangeSubmissionId::new(submission_id.to_string()),
                (index + 1) as u32,
            )
            .map_err(map_line_amount_violation)?;
        result.push(PurchaseChangeSubmissionLine::new(
            PurchaseChangeSubmissionLineId::new(next_id()),
            data,
        )?);
    }
    Ok(result)
}

/// 计算请求行的表头金额汇总。
///
/// # 参数
/// * `inputs` - 类型化行输入集合；空集合返回零三元组
///
/// # 返回
/// 返回 `(gross, net, tax)` 表头汇总。
///
/// # 错误
/// 任一行金额输入非法（缺数量/单价/物流金额）时返回 `ValidationError`。
pub(super) fn compute_request_totals(inputs: &[PurchaseLineInput]) -> Result<(Amount, Amount, Amount)> {
    compute_header_totals(inputs).map_err(map_line_amount_violation)
}

/// 映射行金额领域校验失败为服务错误。
///
/// # 参数
/// * `violation` - 领域金额校验失败原因
///
/// # 返回
/// 返回参数验证错误，文案与既有实现一致。
pub(super) fn map_line_amount_violation(violation: LineAmountViolation) -> Error {
    match violation {
        LineAmountViolation::MissingQuantity
        | LineAmountViolation::MissingUnitCostGross
        | LineAmountViolation::MissingGrossAmount => Error::ValidationError(violation.to_string()),
    }
}

/// 解析数量；空白视为未提供。
fn parse_quantity(value: Option<&str>) -> Result<Option<Quantity>> {
    match value {
        Some(value) if !value.trim().is_empty() => Quantity::from_str(value.trim())
            .map(Some)
            .map_err(|_| Error::ValidationError(format!("非法数量: {value}"))),
        _ => Ok(None),
    }
}

/// 解析含税单价；空白视为未提供。
fn parse_unit_price(value: Option<&str>) -> Result<Option<UnitPrice>> {
    match value {
        Some(value) if !value.trim().is_empty() => UnitPrice::from_str(value.trim())
            .map(Some)
            .map_err(|_| Error::ValidationError(format!("非法含税单价: {value}"))),
        _ => Ok(None),
    }
}

/// 解析税率；空白视为未提供（缺省按 0 计税）。
fn parse_rate(value: Option<&str>) -> Result<Option<Rate>> {
    match value {
        Some(value) if !value.trim().is_empty() => Rate::from_str(value.trim())
            .map(Some)
            .map_err(|_| Error::ValidationError(format!("非法税率: {value}"))),
        _ => Ok(None),
    }
}

/// 解析金额；空白视为未提供。
fn parse_amount(value: Option<&str>) -> Result<Option<Amount>> {
    match value {
        Some(value) if !value.trim().is_empty() => Amount::from_str(value.trim())
            .map(Some)
            .map_err(|_| Error::ValidationError(format!("非法金额: {value}"))),
        _ => Ok(None),
    }
}

/// 解析业务日期字符串。
fn parse_business_date(value: &str) -> Result<BusinessDate> {
    BusinessDate::from_str(value.trim()).map_err(|_| Error::ValidationError(format!("非法业务日期: {value}")))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use entities::common::time::BusinessDate;
    use entities::money::{Amount, Quantity, Rate, UnitPrice};
    use entities::purchase_order::PurchaseLineType;

    use super::{compute_request_totals, to_line_inputs, SavePurchaseOrderLine};
    use crate::errors::Error;

    /// 构造完整商品行请求。
    fn goods_line() -> SavePurchaseOrderLine {
        SavePurchaseOrderLine {
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some("pcl-1".to_string()),
            sku_id: Some("sku-1".to_string()),
            sku_revision_id: Some("skur-1".to_string()),
            product_name: Some("慰问礼包".to_string()),
            specification: Some("500g×2".to_string()),
            quantity: Some(" 3.000000 ".to_string()),
            base_unit_code: Some("箱".to_string()),
            unit_cost_gross: Some("9.9900".to_string()),
            input_tax_rate: Some("0.130000".to_string()),
            expected_delivery_date: Some("2026-08-06".to_string()),
            sales_order_line_id: Some("sol-1".to_string()),
            sales_order_revision_line_id: Some("sorl-1".to_string()),
            sales_order_submission_line_id: Some("ssl-1".to_string()),
            allocated_quantity: Some("3.000000".to_string()),
            gross_amount: None,
        }
    }

    /// 构造完整物流费用行请求。
    fn logistics_line() -> SavePurchaseOrderLine {
        SavePurchaseOrderLine {
            line_type: PurchaseLineType::LogisticsFee,
            procurement_confirmation_line_id: None,
            sku_id: None,
            sku_revision_id: None,
            product_name: None,
            specification: None,
            quantity: None,
            base_unit_code: None,
            unit_cost_gross: None,
            input_tax_rate: Some("0.130000".to_string()),
            expected_delivery_date: Some("2026-08-07".to_string()),
            sales_order_line_id: None,
            sales_order_revision_line_id: None,
            sales_order_submission_line_id: None,
            allocated_quantity: None,
            gross_amount: Some("100.00".to_string()),
        }
    }

    /// 完整商品行完成类型化：ID、数量、单价、税率与业务日期。
    #[test]
    fn goods_line_converts_to_typed_input() {
        let input = goods_line().to_line_input().unwrap();
        assert_eq!(input.line_type, PurchaseLineType::ItemService);
        assert_eq!(
            input
                .procurement_confirmation_line_id
                .as_ref()
                .map(ToString::to_string),
            Some("pcl-1".to_string())
        );
        assert_eq!(
            input.sku_id.as_ref().map(ToString::to_string),
            Some("sku-1".to_string())
        );
        assert_eq!(
            input.sku_revision_id.as_ref().map(ToString::to_string),
            Some("skur-1".to_string())
        );
        assert_eq!(input.quantity, Some(Quantity::from_str("3.000000").unwrap()));
        assert_eq!(
            input.unit_cost_gross,
            Some(UnitPrice::from_str("9.9900").unwrap())
        );
        assert_eq!(input.input_tax_rate, Some(Rate::from_str("0.130000").unwrap()));
        assert_eq!(
            input.expected_delivery_date,
            Some(BusinessDate::from_ymd(2026, 8, 6).unwrap())
        );
        assert_eq!(
            input.sales_order_line_id.as_ref().map(ToString::to_string),
            Some("sol-1".to_string())
        );
        assert_eq!(
            input.allocated_quantity,
            Some(Quantity::from_str("3.000000").unwrap())
        );
        assert_eq!(input.gross_amount, None);
    }

    /// 物流行把含税金额类型化，其余字段保持空。
    #[test]
    fn logistics_line_converts_with_gross_amount() {
        let input = logistics_line().to_line_input().unwrap();
        assert_eq!(input.quantity, None);
        assert_eq!(input.unit_cost_gross, None);
        assert_eq!(input.gross_amount, Some(Amount::from_str("100.00").unwrap()));
    }

    /// 空白可选字段转换为 `None`，不视为非法。
    #[test]
    fn blank_optional_fields_become_none() {
        let line = SavePurchaseOrderLine {
            quantity: Some("   ".to_string()),
            allocated_quantity: Some("".to_string()),
            input_tax_rate: Some("  ".to_string()),
            expected_delivery_date: None,
            ..logistics_line()
        };
        let input = line.to_line_input().unwrap();
        assert_eq!(input.quantity, None);
        assert_eq!(input.allocated_quantity, None);
        assert_eq!(input.input_tax_rate, None);
        assert_eq!(input.expected_delivery_date, None);
    }

    /// 非法数量、单价、税率、金额与业务日期返回既有文案。
    #[test]
    fn illegal_values_keep_exact_messages() {
        let line = SavePurchaseOrderLine {
            quantity: Some("abc".to_string()),
            ..goods_line()
        };
        assert_eq!(
            line.to_line_input().unwrap_err().to_string(),
            "参数验证失败: 非法数量: abc"
        );

        let line = SavePurchaseOrderLine {
            unit_cost_gross: Some("x".to_string()),
            ..goods_line()
        };
        assert_eq!(
            line.to_line_input().unwrap_err().to_string(),
            "参数验证失败: 非法含税单价: x"
        );

        let line = SavePurchaseOrderLine {
            input_tax_rate: Some("y".to_string()),
            ..goods_line()
        };
        assert_eq!(
            line.to_line_input().unwrap_err().to_string(),
            "参数验证失败: 非法税率: y"
        );

        let line = SavePurchaseOrderLine {
            gross_amount: Some("z".to_string()),
            ..logistics_line()
        };
        assert_eq!(
            line.to_line_input().unwrap_err().to_string(),
            "参数验证失败: 非法金额: z"
        );

        let line = SavePurchaseOrderLine {
            expected_delivery_date: Some("not-a-date".to_string()),
            ..goods_line()
        };
        assert_eq!(
            line.to_line_input().unwrap_err().to_string(),
            "参数验证失败: 非法业务日期: not-a-date"
        );
    }

    /// 首错优先级与旧实现一致：税率最先，商品行数量在单价前，物流金额在数量前。
    #[test]
    fn first_error_priority_matches_legacy_order() {
        let line = SavePurchaseOrderLine {
            input_tax_rate: Some("y".to_string()),
            quantity: Some("abc".to_string()),
            ..goods_line()
        };
        assert_eq!(
            line.to_line_input().unwrap_err().to_string(),
            "参数验证失败: 非法税率: y"
        );

        let line = SavePurchaseOrderLine {
            quantity: None,
            unit_cost_gross: Some("x".to_string()),
            ..goods_line()
        };
        assert_eq!(
            line.to_line_input().unwrap_err().to_string(),
            "参数验证失败: 商品行数量不能为空"
        );

        let line = SavePurchaseOrderLine {
            quantity: None,
            expected_delivery_date: Some("not-a-date".to_string()),
            ..goods_line()
        };
        assert_eq!(
            line.to_line_input().unwrap_err().to_string(),
            "参数验证失败: 商品行数量不能为空"
        );

        let line = SavePurchaseOrderLine {
            gross_amount: None,
            quantity: Some("abc".to_string()),
            ..logistics_line()
        };
        assert_eq!(
            line.to_line_input().unwrap_err().to_string(),
            "参数验证失败: 物流费用行含税金额不能为空"
        );

        let line = SavePurchaseOrderLine {
            gross_amount: Some("z".to_string()),
            quantity: Some("abc".to_string()),
            ..logistics_line()
        };
        assert_eq!(
            line.to_line_input().unwrap_err().to_string(),
            "参数验证失败: 非法金额: z"
        );
    }

    /// 表头汇总与逐行领域计算一致。
    #[test]
    fn request_totals_match_domain_computation() {
        let inputs = to_line_inputs(&[goods_line(), logistics_line()]).unwrap();
        let (gross, net, tax) = compute_request_totals(&inputs).unwrap();
        assert_eq!(gross, Amount::from_str("129.97").unwrap());
        assert_eq!(net, Amount::from_str("113.07").unwrap());
        assert_eq!(tax, Amount::from_str("16.90").unwrap());
    }

    /// 转换失败时返回 `ValidationError` 分类（HTTP 400 语义）。
    #[test]
    fn conversion_errors_are_validation_errors() {
        let line = SavePurchaseOrderLine {
            quantity: None,
            ..goods_line()
        };
        assert!(matches!(
            line.to_line_input().unwrap_err(),
            Error::ValidationError(_)
        ));
    }
}
