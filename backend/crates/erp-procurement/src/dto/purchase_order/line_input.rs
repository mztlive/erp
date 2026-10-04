//! 采购行请求的类型化转换；空白、字段错误及首错顺序由请求合同统一维护。

use std::str::FromStr;

use erp_core::common::time::BusinessDate;
use erp_core::ids::{
    ProcurementConfirmationLineId, SalesOrderLineId, SalesOrderRevisionLineId, SalesOrderSubmissionLineId,
    SkuId, SkuRevisionId,
};
use erp_core::money::{Amount, Quantity, Rate, UnitPrice};

use super::SavePurchaseOrderLine;
use crate::entity::purchase_order::{PurchaseLineInput, PurchaseLineType};
use crate::{Error, Result};

struct ParsedLineValues {
    quantity: Option<Quantity>,
    unit_cost_gross: Option<UnitPrice>,
    gross_amount: Option<Amount>,
    input_tax_rate: Option<Rate>,
    expected_delivery_date: Option<BusinessDate>,
    allocated_quantity: Option<Quantity>,
}

impl SavePurchaseOrderLine {
    /// 类型化转换为领域行输入。
    ///
    /// 按约定顺序完成文本类型化：先税率，再按行类型解析数量/含税单价
    /// （商品行）或含税金额（物流行），随后解析数量与单价（物流行）、预计交期与
    /// 分配数量。必填字段缺失与解析失败均返回 `ValidationError`，保留稳定错误文案；金额计算不在此处进行。
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
    pub fn to_line_input(&self) -> Result<PurchaseLineInput> {
        let input_tax_rate = parse_optional::<Rate>(self.input_tax_rate.as_deref(), "税率")?;
        let (quantity, unit_cost_gross, gross_amount) = match self.line_type {
            PurchaseLineType::ItemService => {
                let quantity = parse_optional::<Quantity>(self.quantity.as_deref(), "数量")?
                    .ok_or_else(|| Error::ValidationError("商品行数量不能为空".to_string()))?;
                let unit_cost = parse_optional::<UnitPrice>(self.unit_cost_gross.as_deref(), "含税单价")?
                    .ok_or_else(|| Error::ValidationError("商品行含税单价不能为空".to_string()))?;
                (Some(quantity), Some(unit_cost), None)
            },
            PurchaseLineType::LogisticsFee => {
                let gross = parse_optional::<Amount>(self.gross_amount.as_deref(), "金额")?
                    .ok_or_else(|| Error::ValidationError("物流费用行含税金额不能为空".to_string()))?;
                (
                    parse_optional::<Quantity>(self.quantity.as_deref(), "数量")?,
                    parse_optional::<UnitPrice>(self.unit_cost_gross.as_deref(), "含税单价")?,
                    Some(gross),
                )
            },
        };
        let expected_delivery_date =
            self.expected_delivery_date.as_deref().map(parse_business_date).transpose()?;
        let allocated_quantity = parse_optional::<Quantity>(self.allocated_quantity.as_deref(), "数量")?;
        Ok(self.line_input(ParsedLineValues {
            quantity,
            unit_cost_gross,
            gross_amount,
            input_tax_rate,
            expected_delivery_date,
            allocated_quantity,
        }))
    }

    fn line_input(&self, parsed: ParsedLineValues) -> PurchaseLineInput {
        PurchaseLineInput {
            line_type: self.line_type,
            procurement_confirmation_line_id: self
                .procurement_confirmation_line_id
                .as_ref()
                .map(|value| ProcurementConfirmationLineId::new(value.clone())),
            sku_id: self.sku_id.as_ref().map(|value| SkuId::new(value.clone())),
            sku_revision_id: self.sku_revision_id.as_ref().map(|value| SkuRevisionId::new(value.clone())),
            product_name_snapshot: self.product_name.clone(),
            specification_snapshot: self.specification.clone(),
            quantity: parsed.quantity,
            base_unit_code: self.base_unit_code.clone(),
            unit_cost_gross: parsed.unit_cost_gross,
            input_tax_rate: parsed.input_tax_rate,
            expected_delivery_date: parsed.expected_delivery_date,
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
            allocated_quantity: parsed.allocated_quantity,
            gross_amount: parsed.gross_amount,
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
    pub fn to_line_inputs(lines: &[Self]) -> Result<Vec<PurchaseLineInput>> {
        lines.iter().map(Self::to_line_input).collect()
    }
}

/// 空白可选数值视为缺省；错误保留未经 trim 的原始输入。
fn parse_optional<T: FromStr>(value: Option<&str>, field: &str) -> Result<Option<T>> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            value.trim().parse().map_err(|_| Error::ValidationError(format!("非法{field}: {value}")))
        })
        .transpose()
}

fn parse_business_date(value: &str) -> Result<BusinessDate> {
    BusinessDate::from_str(value.trim()).map_err(|_| Error::ValidationError(format!("非法业务日期: {value}")))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::BusinessDate;
    use erp_core::money::{Amount, Quantity, Rate, UnitPrice};

    use super::SavePurchaseOrderLine;
    use crate::Error;
    use crate::entity::purchase_order::{PurchaseLineType, compute_header_totals};

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
            input.procurement_confirmation_line_id.as_ref().map(ToString::to_string),
            Some("pcl-1".to_string())
        );
        assert_eq!(input.sku_id.as_ref().map(ToString::to_string), Some("sku-1".to_string()));
        assert_eq!(input.sku_revision_id.as_ref().map(ToString::to_string), Some("skur-1".to_string()));
        assert_eq!(input.quantity, Some(Quantity::from_str("3.000000").unwrap()));
        assert_eq!(input.unit_cost_gross, Some(UnitPrice::from_str("9.9900").unwrap()));
        assert_eq!(input.input_tax_rate, Some(Rate::from_str("0.130000").unwrap()));
        assert_eq!(input.expected_delivery_date, Some(BusinessDate::from_ymd(2026, 8, 6).unwrap()));
        assert_eq!(input.sales_order_line_id.as_ref().map(ToString::to_string), Some("sol-1".to_string()));
        assert_eq!(input.allocated_quantity, Some(Quantity::from_str("3.000000").unwrap()));
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
        let line = SavePurchaseOrderLine { quantity: Some("abc".to_string()), ..goods_line() };
        assert_eq!(line.to_line_input().unwrap_err().to_string(), "参数验证失败: 非法数量: abc");

        let line = SavePurchaseOrderLine { unit_cost_gross: Some("x".to_string()), ..goods_line() };
        assert_eq!(line.to_line_input().unwrap_err().to_string(), "参数验证失败: 非法含税单价: x");

        let line = SavePurchaseOrderLine { input_tax_rate: Some("y".to_string()), ..goods_line() };
        assert_eq!(line.to_line_input().unwrap_err().to_string(), "参数验证失败: 非法税率: y");

        let line = SavePurchaseOrderLine { gross_amount: Some("z".to_string()), ..logistics_line() };
        assert_eq!(line.to_line_input().unwrap_err().to_string(), "参数验证失败: 非法金额: z");

        let line =
            SavePurchaseOrderLine { expected_delivery_date: Some("not-a-date".to_string()), ..goods_line() };
        assert_eq!(line.to_line_input().unwrap_err().to_string(), "参数验证失败: 非法业务日期: not-a-date");
    }

    /// 首错优先级：税率最先，商品行数量在单价前，物流金额在数量前。
    #[test]
    fn first_error_priority_keeps_contract_order() {
        let line = SavePurchaseOrderLine {
            input_tax_rate: Some("y".to_string()),
            quantity: Some("abc".to_string()),
            ..goods_line()
        };
        assert_eq!(line.to_line_input().unwrap_err().to_string(), "参数验证失败: 非法税率: y");

        let line =
            SavePurchaseOrderLine { quantity: None, unit_cost_gross: Some("x".to_string()), ..goods_line() };
        assert_eq!(line.to_line_input().unwrap_err().to_string(), "参数验证失败: 商品行数量不能为空");

        let line = SavePurchaseOrderLine {
            quantity: None,
            expected_delivery_date: Some("not-a-date".to_string()),
            ..goods_line()
        };
        assert_eq!(line.to_line_input().unwrap_err().to_string(), "参数验证失败: 商品行数量不能为空");

        let line = SavePurchaseOrderLine {
            gross_amount: None,
            quantity: Some("abc".to_string()),
            ..logistics_line()
        };
        assert_eq!(line.to_line_input().unwrap_err().to_string(), "参数验证失败: 物流费用行含税金额不能为空");

        let line = SavePurchaseOrderLine {
            gross_amount: Some("z".to_string()),
            quantity: Some("abc".to_string()),
            ..logistics_line()
        };
        assert_eq!(line.to_line_input().unwrap_err().to_string(), "参数验证失败: 非法金额: z");
    }

    /// 表头汇总与逐行领域计算一致。
    #[test]
    fn request_totals_match_domain_computation() {
        let inputs = SavePurchaseOrderLine::to_line_inputs(&[goods_line(), logistics_line()]).unwrap();
        let (gross, net, tax) = compute_header_totals(&inputs).unwrap();
        assert_eq!(gross, Amount::from_str("129.97").unwrap());
        assert_eq!(net, Amount::from_str("113.07").unwrap());
        assert_eq!(tax, Amount::from_str("16.90").unwrap());
    }

    /// 转换失败时返回 `ValidationError` 分类（HTTP 400 语义）。
    #[test]
    fn conversion_errors_are_validation_errors() {
        let line = SavePurchaseOrderLine { quantity: None, ..goods_line() };
        assert!(matches!(line.to_line_input().unwrap_err(), Error::ValidationError(_)));
    }

    #[test]
    fn optional_numeric_values_keep_raw_errors_and_blank_semantics() {
        let line = SavePurchaseOrderLine {
            unit_cost_gross: Some(" \t ".to_string()),
            quantity: Some(" \t ".to_string()),
            allocated_quantity: Some(" \t ".to_string()),
            input_tax_rate: Some(" \t ".to_string()),
            ..logistics_line()
        };
        let input = line.to_line_input().unwrap();
        assert!(input.unit_cost_gross.is_none());
        assert!(input.quantity.is_none());
        assert!(input.allocated_quantity.is_none());
        assert!(input.input_tax_rate.is_none());

        let line = SavePurchaseOrderLine { quantity: Some(" abc ".to_string()), ..goods_line() };
        assert_eq!(line.to_line_input().unwrap_err().to_string(), "参数验证失败: 非法数量:  abc ");
        let line = SavePurchaseOrderLine { gross_amount: Some(" \t ".to_string()), ..logistics_line() };
        assert_eq!(line.to_line_input().unwrap_err().to_string(), "参数验证失败: 物流费用行含税金额不能为空");
    }

    #[test]
    fn batch_conversion_keeps_order_empty_input_and_first_failure() {
        assert!(SavePurchaseOrderLine::to_line_inputs(&[]).unwrap().is_empty());
        let inputs = SavePurchaseOrderLine::to_line_inputs(&[logistics_line(), goods_line()]).unwrap();
        assert_eq!(inputs[0].line_type, PurchaseLineType::LogisticsFee);
        assert_eq!(inputs[1].line_type, PurchaseLineType::ItemService);
        let invalid_first = SavePurchaseOrderLine { quantity: Some(" first ".to_string()), ..goods_line() };
        let invalid_later =
            SavePurchaseOrderLine { input_tax_rate: Some("later".to_string()), ..goods_line() };
        assert_eq!(
            SavePurchaseOrderLine::to_line_inputs(&[logistics_line(), invalid_first, invalid_later])
                .unwrap_err()
                .to_string(),
            "参数验证失败: 非法数量:  first ",
        );
    }
}
