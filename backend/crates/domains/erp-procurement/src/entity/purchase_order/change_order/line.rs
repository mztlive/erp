//! `purchase_change_submission_line` 采购变更提交行（数据模型 §6.6）。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{
    ProcurementConfirmationLineId, PurchaseChangeSubmissionId, PurchaseChangeSubmissionLineId,
    SalesOrderLineId, SalesOrderRevisionLineId, SalesOrderSubmissionLineId, SkuId, SkuRevisionId,
};
use erp_core::money::{Amount, Quantity, Rate, UnitPrice};
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use crate::entity::purchase_order::line_common::{
    PurchaseLineDataRef, normalize_and_validate_line, normalize_expense,
};
use crate::entity::purchase_order::offering_source::ensure_offering_source;
use crate::entity::purchase_order::types::PurchaseLineType;
use crate::entity::purchase_order::{
    PurchaseDeliveryAllocation, PurchaseExpenseDetail, PurchaseOfferingSource,
};

/// 采购变更提交行创建数据（不含系统字段）。
///
/// 保存拟变更后的完整采购行及销售分配，字段与 `purchase_order_submission_line` 相同
/// （§6.6）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseChangeSubmissionLineData {
    /// 所属采购变更提交。
    pub purchase_change_submission_id: PurchaseChangeSubmissionId,
    /// 行号（从 1 递增）。
    pub line_no: u32,
    /// 行类型。
    pub line_type: PurchaseLineType,
    /// 商品/服务行对应的采购二次确认分行；物流费用行为空。
    pub procurement_confirmation_line_id: Option<ProcurementConfirmationLineId>,
    /// 商品行引用的 SKU；物流费用行为空。
    pub sku_id: Option<SkuId>,
    /// 商品行引用的 SKU 版本；物流费用行为空。
    pub sku_revision_id: Option<SkuRevisionId>,
    /// 服务端冻结的正式供给选源；旧资料或物流费用行为空。
    #[serde(default)]
    pub supplier_offering_source: Option<PurchaseOfferingSource>,
    /// 本采购行承担的客户交付安排。
    #[serde(default)]
    pub delivery_allocations: Vec<PurchaseDeliveryAllocation>,
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
    /// 含税行金额。
    pub gross_amount: Amount,
    /// 不含税行金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 进项税率。
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
    /// 费用行内容（稳定身份、类别、说明、归属）；商品行为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expense: Option<PurchaseExpenseDetail>,
}

/// 采购变更提交行实体（数据模型 §6.6）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PurchaseChangeSubmissionLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属采购变更提交。
    pub purchase_change_submission_id: PurchaseChangeSubmissionId,
    /// 行号。
    pub line_no: u32,
    /// 行类型。
    pub line_type: PurchaseLineType,
    /// 商品/服务行对应的采购二次确认分行。
    pub procurement_confirmation_line_id: Option<ProcurementConfirmationLineId>,
    /// 商品行引用的 SKU。
    pub sku_id: Option<SkuId>,
    /// 商品行引用的 SKU 版本。
    pub sku_revision_id: Option<SkuRevisionId>,
    /// 服务端冻结的正式供给选源；旧资料或物流费用行为空。
    #[serde(default)]
    pub supplier_offering_source: Option<PurchaseOfferingSource>,
    /// 本采购行承担的客户交付安排。
    #[serde(default)]
    pub delivery_allocations: Vec<PurchaseDeliveryAllocation>,
    /// 商品名称快照。
    pub product_name_snapshot: Option<String>,
    /// 规格快照。
    pub specification_snapshot: Option<String>,
    /// 基础单位数量。
    pub quantity: Option<Quantity>,
    /// 单位代码。
    pub base_unit_code: Option<String>,
    /// 含税采购单价。
    pub unit_cost_gross: Option<UnitPrice>,
    /// 含税行金额。
    pub gross_amount: Amount,
    /// 不含税行金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 进项税率。
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
    /// 费用行内容（稳定身份、类别、说明、归属）；商品行为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expense: Option<PurchaseExpenseDetail>,
}

impl PurchaseLineDataRef for PurchaseChangeSubmissionLineData {
    fn line_type(&self) -> PurchaseLineType {
        self.line_type
    }

    fn procurement_confirmation_line_id(&self) -> &Option<ProcurementConfirmationLineId> {
        &self.procurement_confirmation_line_id
    }

    fn sku_id(&self) -> &Option<SkuId> {
        &self.sku_id
    }

    fn product_name_snapshot(&self) -> &Option<String> {
        &self.product_name_snapshot
    }

    fn specification_snapshot(&self) -> &Option<String> {
        &self.specification_snapshot
    }

    fn quantity(&self) -> Option<Quantity> {
        self.quantity
    }

    fn base_unit_code(&self) -> &Option<String> {
        &self.base_unit_code
    }

    fn unit_cost_gross(&self) -> Option<UnitPrice> {
        self.unit_cost_gross
    }

    fn gross_amount(&self) -> Amount {
        self.gross_amount
    }

    fn net_amount(&self) -> Amount {
        self.net_amount
    }

    fn tax_amount(&self) -> Amount {
        self.tax_amount
    }

    fn input_tax_rate(&self) -> Option<Rate> {
        self.input_tax_rate
    }

    /// 按行类型校验销售分配字段。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 商品行已引用销售稳定行与当前版本行且分配数量不为负（零数量表示已移除），或物流费用行不携带销售分配时返回 `Ok(())`。
    ///
    /// # 错误
    /// 商品行缺少销售行引用、分配数量缺失或为负，或物流费用行携带销售行、历史提交行或分配数量时返回领域错误。
    fn ensure_allocation(&self) -> Result<()> {
        match self.line_type {
            PurchaseLineType::ItemService => {
                if self.sales_order_line_id.is_none() || self.sales_order_revision_line_id.is_none() {
                    return Err(Error::from("商品/服务行必须引用销售稳定行与当前版本行"));
                }
                let quantity = self.allocated_quantity.ok_or("商品/服务行必须填写分配数量")?;
                if quantity.to_decimal() < rust_decimal::Decimal::ZERO {
                    return Err(Error::from("商品/服务行分配数量不能为负"));
                }
            },
            PurchaseLineType::LogisticsFee => {
                if self.sales_order_line_id.is_some()
                    || self.sales_order_revision_line_id.is_some()
                    || self.sales_order_submission_line_id.is_some()
                    || self.allocated_quantity.is_some()
                {
                    return Err(Error::from("物流费用行不得携带销售分配"));
                }
            },
        }
        Ok(())
    }

    /// 变更提交行与版本行允许零数量，表示该行已移除但保留原行身份。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 恒返回 `true`。
    ///
    /// # 错误
    /// 不返回错误。
    fn allows_zero_quantity(&self) -> bool {
        true
    }
}

impl PurchaseChangeSubmissionLine {
    /// 创建采购变更提交行。
    ///
    /// 完成快照文本的规范化，并按行类型强制字段归属与金额三元组守恒（§6.6）；
    /// 商品行必须携带销售提交行引用与分配数量。
    ///
    /// # 参数
    /// * `id` - 实体主键（`erp_core::ids::PurchaseChangeSubmissionLineId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的提交行实体。
    ///
    /// # 错误
    /// 行号为零、字段归属与行类型不符、快照超长、数量/单价/税率越界或
    /// 金额三元组不守恒时返回错误。
    pub fn new(
        id: PurchaseChangeSubmissionLineId,
        mut data: PurchaseChangeSubmissionLineData,
    ) -> Result<Self> {
        ensure_line_no(data.line_no)?;
        let (product_name, specification, base_unit_code) = normalize_and_validate_line(&data)?;
        ensure_offering_source(
            data.line_type,
            data.supplier_offering_source.as_ref(),
            data.unit_cost_gross,
            data.input_tax_rate,
        )?;
        PurchaseDeliveryAllocation::validate(&mut data.delivery_allocations, data.line_type, data.quantity)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            purchase_change_submission_id: data.purchase_change_submission_id,
            line_no: data.line_no,
            line_type: data.line_type,
            procurement_confirmation_line_id: data.procurement_confirmation_line_id,
            sku_id: data.sku_id,
            sku_revision_id: data.sku_revision_id,
            supplier_offering_source: data.supplier_offering_source,
            delivery_allocations: data.delivery_allocations,
            product_name_snapshot: product_name,
            specification_snapshot: specification,
            quantity: data.quantity,
            base_unit_code,
            unit_cost_gross: data.unit_cost_gross,
            gross_amount: data.gross_amount,
            net_amount: data.net_amount,
            tax_amount: data.tax_amount,
            input_tax_rate: data.input_tax_rate,
            expected_delivery_date: data.expected_delivery_date,
            sales_order_line_id: data.sales_order_line_id,
            sales_order_revision_line_id: data.sales_order_revision_line_id,
            sales_order_submission_line_id: data.sales_order_submission_line_id,
            allocated_quantity: data.allocated_quantity,
            expense: normalize_expense(data.line_type, data.expense)?,
        })
    }
}

/// 校验行号从 1 开始。
///
/// # 参数
/// * `line_no` - 行号
///
/// # 错误
/// 行号为零时返回错误。
fn ensure_line_no(line_no: u32) -> Result<()> {
    if line_no == 0 {
        return Err(Error::from("行号必须从 1 开始"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::BusinessDate;
    use erp_core::ids::{
        ProcurementConfirmationLineId, PurchaseChangeSubmissionId, PurchaseChangeSubmissionLineId,
        SalesOrderLineId, SalesOrderRevisionLineId, SalesOrderSubmissionLineId, SkuId,
    };
    use erp_core::money::{Amount, Quantity, Rate, UnitPrice, line_amounts};

    use super::{PurchaseChangeSubmissionLine, PurchaseChangeSubmissionLineData};
    use crate::entity::purchase_order::types::PurchaseLineType;

    fn change_line_data() -> PurchaseChangeSubmissionLineData {
        let (gross, net, tax) = line_amounts(
            UnitPrice::from_str("9.9900").unwrap(),
            Quantity::from_str("3.000000").unwrap(),
            Rate::from_str("0.130000").unwrap(),
        );
        PurchaseChangeSubmissionLineData {
            purchase_change_submission_id: PurchaseChangeSubmissionId::new("pcs-1"),
            line_no: 1,
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("pcl-1")),
            sku_id: Some(SkuId::new("sku-1")),
            sku_revision_id: Some(erp_core::ids::SkuRevisionId::new("skur-1")),
            supplier_offering_source: None,
            delivery_allocations: Vec::new(),
            product_name_snapshot: Some("慰问礼包".to_string()),
            specification_snapshot: Some("500g×2".to_string()),
            quantity: Some(Quantity::from_str("3.000000").unwrap()),
            base_unit_code: Some("箱".to_string()),
            unit_cost_gross: Some(UnitPrice::from_str("9.9900").unwrap()),
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
            input_tax_rate: Some(Rate::from_str("0.130000").unwrap()),
            expected_delivery_date: Some(BusinessDate::from_ymd(2026, 8, 6).unwrap()),
            sales_order_line_id: Some(SalesOrderLineId::new("sol-1")),
            sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new("sorl-1")),
            sales_order_submission_line_id: Some(SalesOrderSubmissionLineId::new("ssl-1")),
            allocated_quantity: Some(Quantity::from_str("3.000000").unwrap()),
            expense: None,
        }
    }

    #[test]
    fn change_submission_line_happy_and_failure() {
        let line = PurchaseChangeSubmissionLine::new(
            PurchaseChangeSubmissionLineId::new("pcsl-1"),
            change_line_data(),
        )
        .unwrap();
        assert_eq!(line.line_type, PurchaseLineType::ItemService);

        let bad_amounts = PurchaseChangeSubmissionLineData {
            gross_amount: Amount::from_str("29.98").unwrap(),
            ..change_line_data()
        };
        assert!(
            PurchaseChangeSubmissionLine::new(PurchaseChangeSubmissionLineId::new("pcsl-2"), bad_amounts,)
                .is_err()
        );

        let fee_with_quantity = PurchaseChangeSubmissionLineData {
            line_type: PurchaseLineType::LogisticsFee,
            quantity: Some(Quantity::from_str("3.000000").unwrap()),
            ..change_line_data()
        };
        assert!(
            PurchaseChangeSubmissionLine::new(
                PurchaseChangeSubmissionLineId::new("pcsl-3"),
                fee_with_quantity,
            )
            .is_err()
        );
    }

    /// 改单移除的商品行以零数量和零金额保留原行身份；负数量仍被拒绝。
    #[test]
    fn change_line_keeps_removed_item_with_zero_quantity() {
        let zero = Quantity::from_str("0").unwrap();
        let rate = Rate::from_str("0.130000").unwrap();
        let (gross, net, tax) = line_amounts(UnitPrice::from_str("9.9900").unwrap(), zero, rate);
        let removed = PurchaseChangeSubmissionLineData {
            quantity: Some(zero),
            allocated_quantity: Some(zero),
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
            ..change_line_data()
        };

        let line = PurchaseChangeSubmissionLine::new(PurchaseChangeSubmissionLineId::new("pcsl-4"), removed)
            .unwrap();

        assert_eq!(line.quantity, Some(zero));
        assert!(line.gross_amount.to_decimal().is_zero());
    }
}
