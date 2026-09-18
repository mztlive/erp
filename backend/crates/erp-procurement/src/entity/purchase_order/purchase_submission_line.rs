//! `purchase_order_submission_line` 采购提交行（数据模型 §6.6）。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{
    ProcurementConfirmationLineId, PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId,
    SalesOrderLineId, SalesOrderRevisionLineId, SalesOrderSubmissionLineId, SkuId, SkuRevisionId,
};
use erp_core::money::{Amount, Quantity, Rate, UnitPrice};
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use crate::entity::purchase_order::line_common::{PurchaseLineDataRef, normalize_and_validate_line};
use crate::entity::purchase_order::types::PurchaseLineType;

/// 采购提交行创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseOrderSubmissionLineData {
    /// 所属提交。
    pub purchase_order_submission_id: PurchaseOrderSubmissionId,
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
}

impl PurchaseLineDataRef for PurchaseOrderSubmissionLineData {
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

    fn ensure_allocation(&self) -> Result<()> {
        match self.line_type {
            PurchaseLineType::ItemService => {
                if self.sales_order_line_id.is_none() || self.sales_order_revision_line_id.is_none() {
                    return Err(Error::from("商品/服务行必须引用销售稳定行与当前版本行"));
                }
                let quantity = self.allocated_quantity.ok_or("商品/服务行必须填写分配数量")?;
                if quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
                    return Err(Error::from("商品/服务行分配数量必须为正"));
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
}

/// 采购提交行实体（数据模型 §6.6）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PurchaseOrderSubmissionLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属提交。
    pub purchase_order_submission_id: PurchaseOrderSubmissionId,
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
}

impl PurchaseOrderSubmissionLine {
    /// 创建采购提交行。
    ///
    /// 完成快照文本的规范化，并按行类型强制字段归属与金额三元组守恒（§6.6）；
    /// 商品行必须携带销售提交行引用与分配数量。
    ///
    /// # 参数
    /// * `id` - 实体主键（`erp_core::ids::PurchaseOrderSubmissionLineId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的提交行实体。
    ///
    /// # 错误
    /// 行号为零、字段归属与行类型不符、快照超长、数量/单价/税率越界或
    /// 金额三元组不守恒时返回错误。
    pub fn new(id: PurchaseOrderSubmissionLineId, data: PurchaseOrderSubmissionLineData) -> Result<Self> {
        ensure_line_no(data.line_no)?;
        let (product_name, specification, base_unit_code) = normalize_and_validate_line(&data)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            purchase_order_submission_id: data.purchase_order_submission_id,
            line_no: data.line_no,
            line_type: data.line_type,
            procurement_confirmation_line_id: data.procurement_confirmation_line_id,
            sku_id: data.sku_id.clone(),
            sku_revision_id: data.sku_revision_id,
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
        })
    }

    /// 把草稿行复制到新的冻结提交。
    ///
    /// # 参数
    /// * `id` - 新提交行稳定身份
    /// * `submission_id` - 新正式提交稳定身份
    /// * `draft_line` - 当前草稿行
    ///
    /// # 返回
    /// 返回业务内容与草稿行一致、重新挂接到正式提交的新行。
    ///
    /// # 错误
    /// 草稿行本身不满足当前采购行不变式时返回领域错误。
    pub fn freeze_from_draft(
        id: PurchaseOrderSubmissionLineId,
        submission_id: PurchaseOrderSubmissionId,
        draft_line: &Self,
    ) -> Result<Self> {
        Self::new(
            id,
            PurchaseOrderSubmissionLineData {
                purchase_order_submission_id: submission_id,
                line_no: draft_line.line_no,
                line_type: draft_line.line_type,
                procurement_confirmation_line_id: draft_line.procurement_confirmation_line_id.clone(),
                sku_id: draft_line.sku_id.clone(),
                sku_revision_id: draft_line.sku_revision_id.clone(),
                product_name_snapshot: draft_line.product_name_snapshot.clone(),
                specification_snapshot: draft_line.specification_snapshot.clone(),
                quantity: draft_line.quantity,
                base_unit_code: draft_line.base_unit_code.clone(),
                unit_cost_gross: draft_line.unit_cost_gross,
                gross_amount: draft_line.gross_amount,
                net_amount: draft_line.net_amount,
                tax_amount: draft_line.tax_amount,
                input_tax_rate: draft_line.input_tax_rate,
                expected_delivery_date: draft_line.expected_delivery_date,
                sales_order_line_id: draft_line.sales_order_line_id.clone(),
                sales_order_revision_line_id: draft_line.sales_order_revision_line_id.clone(),
                sales_order_submission_line_id: draft_line.sales_order_submission_line_id.clone(),
                allocated_quantity: draft_line.allocated_quantity,
            },
        )
    }

    /// 校验客户端没有改写服务端冻结的销售与 SKU 来源引用。
    ///
    /// # 参数
    /// * `requested` - 待保存请求行的草稿编辑请求
    ///
    /// # 返回
    /// 所有来源引用一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 任一稳定身份或版本引用变化时返回
    /// [`DraftLineEditViolation::RewrittenSourceReference`]。
    pub fn ensure_source_references_unchanged(
        &self,
        requested: &super::draft_edit::DraftLineEdit,
    ) -> std::result::Result<(), super::draft_edit::DraftLineEditViolation> {
        let unchanged = [
            (
                requested.procurement_confirmation_line_id.as_deref(),
                self.procurement_confirmation_line_id.as_ref().map(ToString::to_string),
            ),
            (requested.sku_id.as_deref(), self.sku_id.as_ref().map(ToString::to_string)),
            (requested.sku_revision_id.as_deref(), self.sku_revision_id.as_ref().map(ToString::to_string)),
            (
                requested.sales_order_line_id.as_deref(),
                self.sales_order_line_id.as_ref().map(ToString::to_string),
            ),
            (
                requested.sales_order_revision_line_id.as_deref(),
                self.sales_order_revision_line_id.as_ref().map(ToString::to_string),
            ),
            (
                requested.sales_order_submission_line_id.as_deref(),
                self.sales_order_submission_line_id.as_ref().map(ToString::to_string),
            ),
        ]
        .into_iter()
        .all(|(requested, existing)| super::draft_edit::normalized_optional_id(requested) == existing);
        if !unchanged {
            return Err(super::draft_edit::DraftLineEditViolation::RewrittenSourceReference);
        }
        Ok(())
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
        ProcurementConfirmationLineId, PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId,
        SalesOrderLineId, SalesOrderRevisionLineId, SalesOrderSubmissionLineId, SkuId,
    };
    use erp_core::money::{Amount, Quantity, Rate, UnitPrice, line_amounts};

    use super::{PurchaseOrderSubmissionLine, PurchaseOrderSubmissionLineData};
    use crate::entity::purchase_order::types::PurchaseLineType;

    fn goods_line_data() -> PurchaseOrderSubmissionLineData {
        let (gross, net, tax) = line_amounts(
            UnitPrice::from_str("9.9900").unwrap(),
            Quantity::from_str("3.000000").unwrap(),
            Rate::from_str("0.130000").unwrap(),
        );
        PurchaseOrderSubmissionLineData {
            purchase_order_submission_id: PurchaseOrderSubmissionId::new("sub-1"),
            line_no: 1,
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("pcl-1")),
            sku_id: Some(SkuId::new("sku-1")),
            sku_revision_id: Some(erp_core::ids::SkuRevisionId::new("skur-1")),
            product_name_snapshot: Some(" 慰问礼包 ".to_string()),
            specification_snapshot: Some(" 500g×2 ".to_string()),
            quantity: Some(Quantity::from_str("3.000000").unwrap()),
            base_unit_code: Some(" 箱 ".to_string()),
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
        }
    }

    #[test]
    fn submission_line_goods_happy_path() {
        let line =
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("sl-1"), goods_line_data())
                .unwrap();
        assert_eq!(line.product_name_snapshot.as_deref(), Some("慰问礼包"));
        assert_eq!(line.base_unit_code.as_deref(), Some("箱"));
        assert_eq!(line.line_type, PurchaseLineType::ItemService);
    }

    #[test]
    fn submission_line_logistics_fee_amounts_consistent() {
        let gross = Amount::from_str("50.00").unwrap();
        let tax = Amount::from_str("6.50").unwrap();
        let net = Amount::from_str("43.50").unwrap();
        let data = PurchaseOrderSubmissionLineData {
            line_type: PurchaseLineType::LogisticsFee,
            procurement_confirmation_line_id: None,
            sku_id: None,
            sku_revision_id: None,
            product_name_snapshot: None,
            specification_snapshot: None,
            quantity: None,
            base_unit_code: None,
            unit_cost_gross: None,
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
            input_tax_rate: Some(Rate::from_str("0.130000").unwrap()),
            expected_delivery_date: None,
            sales_order_line_id: None,
            sales_order_revision_line_id: None,
            sales_order_submission_line_id: None,
            allocated_quantity: None,
            ..goods_line_data()
        };
        let line =
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("sl-2"), data).unwrap();
        assert_eq!(line.quantity, None);
        assert_eq!(line.gross_amount, gross);
    }

    #[test]
    fn submission_line_rejects_failures() {
        // 商品行缺少销售当前版本行
        let no_allocation =
            PurchaseOrderSubmissionLineData { sales_order_revision_line_id: None, ..goods_line_data() };
        assert!(
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("sl-3"), no_allocation,)
                .is_err()
        );

        // 物流费用行携带 SKU
        let fee_with_sku = PurchaseOrderSubmissionLineData {
            line_type: PurchaseLineType::LogisticsFee,
            sku_id: Some(SkuId::new("sku-1")),
            ..goods_line_data()
        };
        assert!(
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("sl-4"), fee_with_sku)
                .is_err()
        );

        // 商品行金额三元组不守恒
        let bad_amounts = PurchaseOrderSubmissionLineData {
            gross_amount: Amount::from_str("30.00").unwrap(),
            ..goods_line_data()
        };
        assert!(
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("sl-5"), bad_amounts)
                .is_err()
        );

        // 行号为零
        let zero_line = PurchaseOrderSubmissionLineData { line_no: 0, ..goods_line_data() };
        assert!(
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("sl-6"), zero_line).is_err()
        );

        // 超长规格快照
        let overlong = PurchaseOrderSubmissionLineData {
            specification_snapshot: Some("s".repeat(513)),
            ..goods_line_data()
        };
        assert!(
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("sl-7"), overlong).is_err()
        );
    }
}
