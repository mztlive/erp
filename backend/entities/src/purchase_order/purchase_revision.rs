//! `purchase_order_revision` / `purchase_order_revision_line`（数据模型 §6.6）。
//!
//! 采购生效版本是不可变修订：财务审核通过时由已通过提交原样复制（§8.1 第 4 条）。
//! 版本按 §4.4 内联结构化快照（供应商名称、付款条件门禁、商品名称、规格、单位），
//! 后续基础资料修改不改变历史单据。修订一经形成不得修改内容（§4.5）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::revision::RevisionBase;
use crate::common::time::{BusinessDate, Instant};
use crate::errors::{Error, Result};
use crate::ids::{
    ProcurementConfirmationLineId, PurchaseOrderId, PurchaseOrderRevisionId, PurchaseOrderRevisionLineId,
    SalesOrderLineId, SalesOrderRevisionLineId, SkuId, SkuRevisionId, SupplierCommercialProfileRevisionId,
};
use crate::money::{Amount, Quantity, Rate, UnitPrice};
use crate::purchase_order::line_common::{normalize_and_validate_line, PurchaseLineDataRef};
use crate::purchase_order::snapshot::{PaymentTermSnapshot, SupplierSnapshot};
use crate::purchase_order::types::PurchaseLineType;

/// 采购版本创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseOrderRevisionData {
    /// 所属采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 版本号（同一采购单内从 1 递增）。
    pub revision_no: u32,
    /// 供应商版本。
    pub supplier_revision_id: SupplierCommercialProfileRevisionId,
    /// 供应商快照。
    pub supplier_snapshot: SupplierSnapshot,
    /// 付款条件与先款后货门禁快照。
    pub payment_term_snapshot: PaymentTermSnapshot,
    /// 含税行汇总。
    pub gross_amount: Amount,
    /// 不含税行汇总。
    pub net_amount: Amount,
    /// 税额行汇总。
    pub tax_amount: Amount,
    /// 生效时间。
    pub effective_at: Instant,
}

/// 采购生效版本实体（不可变修订，数据模型 §6.6/§4.4）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PurchaseOrderRevision {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub revision: RevisionBase,
    /// 所属采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 供应商版本。
    pub supplier_revision_id: SupplierCommercialProfileRevisionId,
    /// 供应商名称等结构化快照。
    pub supplier_snapshot: SupplierSnapshot,
    /// 付款条件与先款后货门禁快照。
    pub payment_term_snapshot: PaymentTermSnapshot,
    /// 含税行汇总。
    pub gross_amount: Amount,
    /// 不含税行汇总。
    pub net_amount: Amount,
    /// 税额行汇总。
    pub tax_amount: Amount,
    /// 生效时间。
    pub effective_at: Instant,
}

impl PurchaseOrderRevision {
    /// 创建采购生效版本。
    ///
    /// 校验版本号从 1 开始，并强制表头金额守恒（`gross = net + tax`，§4.2 铁律 4）。
    /// 版本内容不可修改；追加变更走更高版本号的新修订。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PurchaseOrderRevisionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的版本实体。
    ///
    /// # 错误
    /// 版本号为零或表头金额三元组不守恒时返回错误。
    pub fn new(id: PurchaseOrderRevisionId, data: PurchaseOrderRevisionData) -> Result<Self> {
        ensure_revision_no(data.revision_no)?;
        ensure_header_triple(
            data.gross_amount,
            data.net_amount,
            data.tax_amount,
            &data.purchase_order_id,
        )?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            revision: RevisionBase::new(data.revision_no),
            purchase_order_id: data.purchase_order_id,
            supplier_revision_id: data.supplier_revision_id,
            supplier_snapshot: data.supplier_snapshot,
            payment_term_snapshot: data.payment_term_snapshot,
            gross_amount: data.gross_amount,
            net_amount: data.net_amount,
            tax_amount: data.tax_amount,
            effective_at: data.effective_at,
        })
    }
}

/// 采购版本行创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseOrderRevisionLineData {
    /// 所属采购版本。
    pub purchase_order_revision_id: PurchaseOrderRevisionId,
    /// 版本内行号（从 1 递增）。
    pub line_no: u32,
    /// 行类型。
    pub line_type: PurchaseLineType,
    /// 首次商品/服务采购行对应的二次确认分行；物流费用行为空；
    /// 后续采购变更行引用原行（P3 校验），不伪造新的采购确认。
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
    /// 商品行正式分配数量。
    pub allocated_quantity: Option<Quantity>,
}

/// 采购版本行实体（数据模型 §6.6/§4.4 结构化快照）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PurchaseOrderRevisionLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属采购版本。
    pub purchase_order_revision_id: PurchaseOrderRevisionId,
    /// 版本内行号。
    pub line_no: u32,
    /// 行类型。
    pub line_type: PurchaseLineType,
    /// 首次商品/服务采购行对应的二次确认分行。
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
    /// 商品行正式分配数量。
    pub allocated_quantity: Option<Quantity>,
}

impl PurchaseLineDataRef for PurchaseOrderRevisionLineData {
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
                    return Err(Error::from("商品/服务版本行必须引用销售稳定行与当前版本行"));
                }
                let quantity = self.allocated_quantity.ok_or("商品/服务版本行必须填写分配数量")?;
                if quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
                    return Err(Error::from("商品/服务版本行分配数量必须为正"));
                }
            }
            PurchaseLineType::LogisticsFee => {
                if self.sales_order_line_id.is_some()
                    || self.sales_order_revision_line_id.is_some()
                    || self.allocated_quantity.is_some()
                {
                    return Err(Error::from("物流费用版本行不得携带销售分配"));
                }
            }
        }
        Ok(())
    }
}

impl PurchaseOrderRevisionLine {
    /// 创建采购版本行。
    ///
    /// 完成快照文本的规范化，并按行类型强制字段归属与金额三元组守恒（§6.6）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PurchaseOrderRevisionLineId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的版本行实体。
    ///
    /// # 错误
    /// 行号为零、字段归属与行类型不符、快照超长、数量/单价/税率越界或
    /// 金额三元组不守恒时返回错误。
    pub fn new(id: PurchaseOrderRevisionLineId, data: PurchaseOrderRevisionLineData) -> Result<Self> {
        ensure_line_no(data.line_no)?;
        let (product_name, specification, base_unit_code) = normalize_and_validate_line(&data)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            purchase_order_revision_id: data.purchase_order_revision_id,
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
            allocated_quantity: data.allocated_quantity,
        })
    }
}

/// 校验版本号从 1 开始。
///
/// # 参数
/// * `revision_no` - 版本号
///
/// # 错误
/// 版本号为零时返回错误。
fn ensure_revision_no(revision_no: u32) -> Result<()> {
    if revision_no == 0 {
        return Err(Error::from("版本号必须从 1 开始"));
    }
    Ok(())
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

/// 校验表头金额三元组守恒。
///
/// # 参数
/// * `gross_amount` / `net_amount` / `tax_amount` - 表头汇总
/// * `purchase_order_id` - 所属采购单（错误提示上下文）
///
/// # 错误
/// `gross ≠ net + tax` 或任一分量为负时返回错误。
fn ensure_header_triple(
    gross_amount: Amount,
    net_amount: Amount,
    tax_amount: Amount,
    purchase_order_id: &PurchaseOrderId,
) -> Result<()> {
    if gross_amount.to_decimal() != net_amount.to_decimal() + tax_amount.to_decimal()
        || gross_amount.to_decimal() < rust_decimal::Decimal::ZERO
        || net_amount.to_decimal() < rust_decimal::Decimal::ZERO
        || tax_amount.to_decimal() < rust_decimal::Decimal::ZERO
    {
        return Err(Error::from(format!(
            "采购版本表头金额三元组不守恒（采购单 {purchase_order_id}）"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        PurchaseOrderRevision, PurchaseOrderRevisionData, PurchaseOrderRevisionLine,
        PurchaseOrderRevisionLineData,
    };
    use crate::common::time::{BusinessDate, Instant};
    use crate::ids::{
        ProcurementConfirmationLineId, PurchaseOrderId, PurchaseOrderRevisionId, PurchaseOrderRevisionLineId,
        SalesOrderLineId, SalesOrderRevisionLineId, SkuId, SupplierCommercialProfileRevisionId,
    };
    use crate::money::{line_amounts, Amount, Quantity, Rate, UnitPrice};
    use crate::purchase_order::snapshot::{PaymentTermSnapshot, SupplierSnapshot};
    use crate::purchase_order::types::PurchaseLineType;
    use std::str::FromStr;

    fn snapshot() -> SupplierSnapshot {
        SupplierSnapshot::new("北京华联供应商".to_string()).unwrap()
    }

    fn payment_term() -> PaymentTermSnapshot {
        PaymentTermSnapshot::new("NET-30".to_string(), false, None, None).unwrap()
    }

    fn revision_data() -> PurchaseOrderRevisionData {
        PurchaseOrderRevisionData {
            purchase_order_id: PurchaseOrderId::new("po-1"),
            revision_no: 1,
            supplier_revision_id: SupplierCommercialProfileRevisionId::new("spr-1"),
            supplier_snapshot: snapshot(),
            payment_term_snapshot: payment_term(),
            gross_amount: Amount::from_str("29.97").unwrap(),
            net_amount: Amount::from_str("26.07").unwrap(),
            tax_amount: Amount::from_str("3.90").unwrap(),
            effective_at: Instant::from_unix_secs(1_700_000_000),
        }
    }

    fn line_data() -> PurchaseOrderRevisionLineData {
        let (gross, net, tax) = line_amounts(
            UnitPrice::from_str("9.9900").unwrap(),
            Quantity::from_str("3.000000").unwrap(),
            Rate::from_str("0.130000").unwrap(),
        );
        PurchaseOrderRevisionLineData {
            purchase_order_revision_id: PurchaseOrderRevisionId::new("por-1"),
            line_no: 1,
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("pcl-1")),
            sku_id: Some(SkuId::new("sku-1")),
            sku_revision_id: Some(crate::ids::SkuRevisionId::new("skur-1")),
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
            allocated_quantity: Some(Quantity::from_str("3.000000").unwrap()),
        }
    }

    #[test]
    fn revision_new_validates_revision_no_and_header_triple() {
        let revision =
            PurchaseOrderRevision::new(PurchaseOrderRevisionId::new("por-1"), revision_data()).unwrap();
        assert_eq!(revision.revision.revision_no, 1);
        assert_eq!(revision.supplier_snapshot.supplier_name, "北京华联供应商");

        let zero = PurchaseOrderRevisionData {
            revision_no: 0,
            ..revision_data()
        };
        assert!(PurchaseOrderRevision::new(PurchaseOrderRevisionId::new("por-2"), zero).is_err());

        let inconsistent = PurchaseOrderRevisionData {
            gross_amount: Amount::from_str("30.00").unwrap(),
            ..revision_data()
        };
        assert!(PurchaseOrderRevision::new(PurchaseOrderRevisionId::new("por-3"), inconsistent).is_err());
    }

    #[test]
    fn revision_line_goods_keeps_snapshots_and_amounts() {
        let line =
            PurchaseOrderRevisionLine::new(PurchaseOrderRevisionLineId::new("porl-1"), line_data()).unwrap();
        assert_eq!(line.product_name_snapshot.as_deref(), Some("慰问礼包"));
        assert_eq!(line.base_unit_code.as_deref(), Some("箱"));
        assert_eq!(line.quantity, Some(Quantity::from_str("3.000000").unwrap()));
        assert_eq!(line.gross_amount, Amount::from_str("29.97").unwrap());
    }

    #[test]
    fn revision_line_rejects_mismatched_amounts_and_zero_line_no() {
        let bad_amounts = PurchaseOrderRevisionLineData {
            gross_amount: Amount::from_str("29.98").unwrap(),
            ..line_data()
        };
        assert!(
            PurchaseOrderRevisionLine::new(PurchaseOrderRevisionLineId::new("porl-2"), bad_amounts).is_err()
        );

        let zero_line = PurchaseOrderRevisionLineData {
            line_no: 0,
            ..line_data()
        };
        assert!(
            PurchaseOrderRevisionLine::new(PurchaseOrderRevisionLineId::new("porl-3"), zero_line).is_err()
        );

        let fee_with_quantity = PurchaseOrderRevisionLineData {
            line_type: PurchaseLineType::LogisticsFee,
            quantity: Some(Quantity::from_str("3.000000").unwrap()),
            ..line_data()
        };
        assert!(PurchaseOrderRevisionLine::new(
            PurchaseOrderRevisionLineId::new("porl-4"),
            fee_with_quantity
        )
        .is_err());
    }

    #[test]
    fn revision_line_logistics_fee_is_separately_taxed() {
        let gross = Amount::from_str("50.00").unwrap();
        let tax = Amount::from_str("6.50").unwrap();
        let net = Amount::from_str("43.50").unwrap();
        let data = PurchaseOrderRevisionLineData {
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
            allocated_quantity: None,
            ..line_data()
        };
        let line = PurchaseOrderRevisionLine::new(PurchaseOrderRevisionLineId::new("porl-5"), data).unwrap();
        assert_eq!(line.gross_amount, gross);
        assert_eq!(line.quantity, None);
    }
}
