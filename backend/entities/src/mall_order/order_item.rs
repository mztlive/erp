//! `mall_order_item`：商城订单商品明细快照（数据模型 §6.17）。
//!
//! 下单时商品、价格、供给和成本快照不可被后续基础资料变化覆盖；本实体不可变，
//! 只提供 `new()`。成本快照的税额标识规则见 `erp-phase-2.md` §12.1.1：
//! 成本含税标识必填，含税时必须同时提供进项税率，禁止用销项税率替代。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{
    MallOrderId, MallOrderItemId, ProductPublicationRevisionId, SkuId, SupplierOfferingRevisionId,
};
use crate::money::{round_to_cent, Amount, Quantity, Rate, UnitPrice};
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 来源明细 ID 最大长度。
const EXTERNAL_ITEM_ID_MAX_LEN: usize = 128;
/// 商品名称快照最大长度。
const NAME_SNAPSHOT_MAX_LEN: usize = 256;
/// 规格快照最大长度。
const SPEC_SNAPSHOT_MAX_LEN: usize = 512;

/// 商城订单明细创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallOrderItemData {
    /// 订单。
    pub mall_order_id: MallOrderId,
    /// 来源明细 ID。
    pub external_item_id: String,
    /// ERP SKU；暂未映射时可空并标记待归集。
    pub sku_id: Option<SkuId>,
    /// 下单时发布版本。
    pub product_publication_revision_id: Option<ProductPublicationRevisionId>,
    /// 下单时固定供给。
    pub supplier_offering_revision_id: Option<SupplierOfferingRevisionId>,
    /// 商品名称快照。
    pub name_snapshot: String,
    /// 规格快照。
    pub spec_snapshot: Option<String>,
    /// 数量。
    pub quantity: Quantity,
    /// 含税售价。
    pub unit_price_gross: UnitPrice,
    /// 明细原价。
    pub line_gross_amount: Amount,
    /// 分到本明细的优惠。
    pub allocated_discount_amount: Amount,
    /// 分到本明细的运费。
    pub allocated_freight_amount: Amount,
    /// 明细实付。
    pub paid_amount: Amount,
    /// 销项税率。
    pub sales_tax_rate: Rate,
    /// 商城记录的供应商单位供货成本，可空。
    pub unit_cost_snapshot: Option<UnitPrice>,
    /// 商城记录的明细供货成本合计，可空。
    pub cost_snapshot_total: Option<Amount>,
    /// 成本含税标识；有成本字段时必填。
    pub cost_tax_inclusion: Option<bool>,
    /// 成本进项税率；含税标识为含税时必填。
    pub cost_input_tax_rate: Option<Rate>,
}

/// 商城订单明细实体（数据模型 §6.17）。
///
/// 明细满足两条行内恒等式（§6.17）：
/// `line_gross_amount = round_to_cent(quantity × unit_price_gross)`，
/// `paid_amount = line_gross_amount - allocated_discount_amount + allocated_freight_amount`。
/// 行合计与列合计（明细分别汇总等于订单同名金额、来源分摊矩阵双向守恒）
/// 依赖聚合查询，由 P3 落实（P3 条目：§6.17 分摊矩阵守恒）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallOrderItem {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 订单。
    pub mall_order_id: MallOrderId,
    /// 来源明细 ID。
    pub external_item_id: String,
    /// ERP SKU。
    pub sku_id: Option<SkuId>,
    /// 下单时发布版本。
    pub product_publication_revision_id: Option<ProductPublicationRevisionId>,
    /// 下单时固定供给。
    pub supplier_offering_revision_id: Option<SupplierOfferingRevisionId>,
    /// 商品名称快照。
    pub name_snapshot: String,
    /// 规格快照。
    pub spec_snapshot: Option<String>,
    /// 数量。
    pub quantity: Quantity,
    /// 含税售价。
    pub unit_price_gross: UnitPrice,
    /// 明细原价。
    pub line_gross_amount: Amount,
    /// 分到本明细的优惠。
    pub allocated_discount_amount: Amount,
    /// 分到本明细的运费。
    pub allocated_freight_amount: Amount,
    /// 明细实付。
    pub paid_amount: Amount,
    /// 销项税率。
    pub sales_tax_rate: Rate,
    /// 商城记录的供应商单位供货成本。
    pub unit_cost_snapshot: Option<UnitPrice>,
    /// 商城记录的明细供货成本合计。
    pub cost_snapshot_total: Option<Amount>,
    /// 成本含税标识。
    pub cost_tax_inclusion: Option<bool>,
    /// 成本进项税率。
    pub cost_input_tax_rate: Option<Rate>,
}

impl MallOrderItem {
    /// 创建商城订单明细。
    ///
    /// 完成文本校验与规范化，并强制行内金额恒等、数量为正、税率在
    /// `[0, 1]` 区间，以及成本税额标识规则（`erp-phase-2.md` §12.1.1）：
    /// - 任一成本字段存在时 `cost_tax_inclusion` 必填；
    /// - `cost_tax_inclusion = true` 时 `cost_input_tax_rate` 必填；
    /// - `cost_tax_inclusion = false` 时不得携带进项税率。
    ///
    /// 成本是否标记 `ACTUAL` 由消费成本评估链决定（§6.17 `mall_consumption_cost_assessment`）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallOrderItemId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的商品明细实体。
    ///
    /// # 错误
    /// 当文本为空/超长、数量非正、金额恒等不成立、税率为负/超限或成本税额标识
    /// 不一致时返回错误。
    pub fn new(id: MallOrderItemId, data: MallOrderItemData) -> Result<Self> {
        let external_item_id = normalize_required_text(
            data.external_item_id,
            "来源明细ID不能为空",
            EXTERNAL_ITEM_ID_MAX_LEN,
            "来源明细ID过长",
        )?;
        let name_snapshot = normalize_required_text(
            data.name_snapshot,
            "商品名称快照不能为空",
            NAME_SNAPSHOT_MAX_LEN,
            "商品名称快照过长",
        )?;
        let spec_snapshot = normalize_optional_text(data.spec_snapshot, "规格快照", SPEC_SNAPSHOT_MAX_LEN)?;
        validate_item_amounts(
            data.quantity,
            data.unit_price_gross,
            data.line_gross_amount,
            data.allocated_discount_amount,
            data.allocated_freight_amount,
            data.paid_amount,
        )?;
        ensure_rate_in_unit_interval(data.sales_tax_rate, "销项税率")?;
        validate_cost_tax_fields(
            data.unit_cost_snapshot,
            data.cost_snapshot_total,
            data.cost_tax_inclusion,
            data.cost_input_tax_rate,
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            mall_order_id: data.mall_order_id,
            external_item_id,
            sku_id: data.sku_id,
            product_publication_revision_id: data.product_publication_revision_id,
            supplier_offering_revision_id: data.supplier_offering_revision_id,
            name_snapshot,
            spec_snapshot,
            quantity: data.quantity,
            unit_price_gross: data.unit_price_gross,
            line_gross_amount: data.line_gross_amount,
            allocated_discount_amount: data.allocated_discount_amount,
            allocated_freight_amount: data.allocated_freight_amount,
            paid_amount: data.paid_amount,
            sales_tax_rate: data.sales_tax_rate,
            unit_cost_snapshot: data.unit_cost_snapshot,
            cost_snapshot_total: data.cost_snapshot_total,
            cost_tax_inclusion: data.cost_tax_inclusion,
            cost_input_tax_rate: data.cost_input_tax_rate,
        })
    }

    /// 判断商品明细是否属于给定商城订单。
    ///
    /// # 参数
    /// * `mall_order_id` - 待校验的商城订单
    ///
    /// # 返回
    /// 明细所属订单一致时返回 `true`。
    pub fn belongs_to_order(&self, mall_order_id: &MallOrderId) -> bool {
        &self.mall_order_id == mall_order_id
    }

    /// 判断累计退款数量与金额是否仍在原支付范围内。
    ///
    /// # 参数
    /// * `refunded_quantity` - 含本次在内的累计退款数量
    /// * `refunded_amount` - 含本次在内的累计退款金额
    ///
    /// # 返回
    /// 数量不超过原购买数量且金额不超过原实付金额时返回 `true`。
    pub fn allows_cumulative_refund(&self, refunded_quantity: Quantity, refunded_amount: Amount) -> bool {
        refunded_quantity <= self.quantity && refunded_amount <= self.paid_amount
    }
}

/// 校验明细行内金额恒等式与非负性。
///
/// 恒等式（§6.17）：
/// `line_gross_amount = round_to_cent(quantity × unit_price_gross)`，
/// `paid_amount = line_gross_amount - allocated_discount_amount + allocated_freight_amount`。
///
/// # 参数
/// * `quantity` - 数量
/// * `unit_price_gross` - 含税售价
/// * `line_gross_amount` - 明细原价
/// * `allocated_discount_amount` - 分摊优惠
/// * `allocated_freight_amount` - 分摊运费
/// * `paid_amount` - 明细实付
///
/// # 返回
/// 恒等成立且数量为正、各金额非负返回 `Ok(())`。
///
/// # 错误
/// 数量非正、金额为负或恒等不成立时返回错误。
fn validate_item_amounts(
    quantity: Quantity,
    unit_price_gross: UnitPrice,
    line_gross_amount: Amount,
    allocated_discount_amount: Amount,
    allocated_freight_amount: Amount,
    paid_amount: Amount,
) -> Result<()> {
    if quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
        return Err(Error::from("明细数量必须大于零"));
    }
    if unit_price_gross.to_decimal().is_sign_negative() {
        return Err(Error::from("含税售价不能为负"));
    }
    for (amount, label) in [
        (line_gross_amount, "明细原价"),
        (allocated_discount_amount, "分摊优惠"),
        (allocated_freight_amount, "分摊运费"),
        (paid_amount, "明细实付"),
    ] {
        if amount.to_decimal().is_sign_negative() {
            return Err(Error::from(format!("{label}金额不能为负")));
        }
    }
    let expected_gross = round_to_cent(quantity.to_decimal() * unit_price_gross.to_decimal());
    if line_gross_amount.to_decimal() != expected_gross {
        return Err(Error::from("明细原价必须等于数量乘含税售价并舍入到分"));
    }
    let expected_paid = line_gross_amount.to_decimal() - allocated_discount_amount.to_decimal()
        + allocated_freight_amount.to_decimal();
    if paid_amount.to_decimal() != expected_paid {
        return Err(Error::from("明细实付必须等于原价减分摊优惠加分摊运费"));
    }
    Ok(())
}

/// 校验税率在 `[0, 1]` 区间。
///
/// # 参数
/// * `rate` - 税率
/// * `label` - 税率中文名（错误提示用）
///
/// # 返回
/// 税率在区间内返回 `Ok(())`。
///
/// # 错误
/// 税率为负或大于 100% 时返回错误。
fn ensure_rate_in_unit_interval(rate: Rate, label: &str) -> Result<()> {
    let decimal = rate.to_decimal();
    if decimal.is_sign_negative() || decimal > rust_decimal::Decimal::ONE {
        return Err(Error::from(format!("{label}必须在 0 到 100% 之间")));
    }
    Ok(())
}

/// 校验成本税额标识一致性（`erp-phase-2.md` §12.1.1）。
///
/// # 参数
/// * `unit_cost_snapshot` - 单位供货成本
/// * `cost_snapshot_total` - 明细成本合计
/// * `cost_tax_inclusion` - 成本含税标识
/// * `cost_input_tax_rate` - 进项税率
///
/// # 返回
/// 成本字段与税额标识一致返回 `Ok(())`。
///
/// # 错误
/// 有成本字段但缺含税标识、含税但缺进项税率，或不含税却携带进项税率时返回错误。
fn validate_cost_tax_fields(
    unit_cost_snapshot: Option<UnitPrice>,
    cost_snapshot_total: Option<Amount>,
    cost_tax_inclusion: Option<bool>,
    cost_input_tax_rate: Option<Rate>,
) -> Result<()> {
    let has_cost_amount = unit_cost_snapshot.is_some() || cost_snapshot_total.is_some();
    match (has_cost_amount, cost_tax_inclusion, cost_input_tax_rate) {
        (false, None, None) => Ok(()),
        (false, _, _) => Err(Error::from("无成本金额时不得携带成本税额标识")),
        (true, None, _) => Err(Error::from("有成本字段时成本含税标识必填")),
        (true, Some(false), None) => Ok(()),
        (true, Some(false), Some(_)) => Err(Error::from("成本不含税时不得携带进项税率")),
        (true, Some(true), None) => Err(Error::from("成本含税时进项税率必填")),
        (true, Some(_), Some(rate)) => ensure_rate_in_unit_interval(rate, "进项税率"),
    }
}

#[cfg(test)]
mod tests {
    use super::{MallOrderItem, MallOrderItemData};
    use crate::ids::{MallOrderId, MallOrderItemId, SkuId};
    use crate::money::{Amount, Quantity, Rate, UnitPrice};
    use std::str::FromStr;

    fn data() -> MallOrderItemData {
        MallOrderItemData {
            mall_order_id: MallOrderId::new("order-1"),
            external_item_id: " item-9 ".to_string(),
            sku_id: Some(SkuId::new("sku-1")),
            product_publication_revision_id: None,
            supplier_offering_revision_id: None,
            name_snapshot: " 咖啡豆 1kg ".to_string(),
            spec_snapshot: Some(" 中烘 ".to_string()),
            quantity: Quantity::from_str("2.000000").unwrap(),
            unit_price_gross: UnitPrice::from_str("9.9900").unwrap(),
            line_gross_amount: Amount::from_str("19.98").unwrap(),
            allocated_discount_amount: Amount::from_str("0.98").unwrap(),
            allocated_freight_amount: Amount::from_str("1.00").unwrap(),
            paid_amount: Amount::from_str("20.00").unwrap(),
            sales_tax_rate: Rate::from_str("0.130000").unwrap(),
            unit_cost_snapshot: Some(UnitPrice::from_str("6.0000").unwrap()),
            cost_snapshot_total: Some(Amount::from_str("12.00").unwrap()),
            cost_tax_inclusion: Some(true),
            cost_input_tax_rate: Some(Rate::from_str("0.060000").unwrap()),
        }
    }

    /// happy path：文本规范化与行内金额恒等（含税成本 + 进项税率）。
    #[test]
    fn new_trims_fields_and_keeps_line_identity() {
        let item = MallOrderItem::new(MallOrderItemId::new("item-1"), data()).unwrap();

        assert_eq!(item.external_item_id, "item-9");
        assert_eq!(item.name_snapshot, "咖啡豆 1kg");
        assert_eq!(item.spec_snapshot.as_deref(), Some("中烘"));
        assert_eq!(item.sku_id, Some(SkuId::new("sku-1")));
        assert_eq!(item.line_gross_amount, Amount::from_str("19.98").unwrap());
        assert_eq!(item.paid_amount, Amount::from_str("20.00").unwrap());
        assert_eq!(
            item.cost_input_tax_rate,
            Some(Rate::from_str("0.060000").unwrap())
        );
    }

    #[test]
    fn order_relationship_and_refund_capacity_are_entity_owned() {
        let item = MallOrderItem::new(MallOrderItemId::new("item-1"), data()).unwrap();
        assert!(item.belongs_to_order(&MallOrderId::new("order-1")));
        assert!(!item.belongs_to_order(&MallOrderId::new("order-2")));
        assert!(item.allows_cumulative_refund(
            Quantity::from_str("2.000000").unwrap(),
            Amount::from_str("20.00").unwrap(),
        ));
        assert!(!item.allows_cumulative_refund(
            Quantity::from_str("2.000001").unwrap(),
            Amount::from_str("20.00").unwrap(),
        ));
        assert!(!item.allows_cumulative_refund(
            Quantity::from_str("2.000000").unwrap(),
            Amount::from_str("20.01").unwrap(),
        ));
    }

    /// 失败路径：必填空、超长、数量越界、金额恒等不成立。
    #[test]
    fn new_rejects_blank_overlong_zero_quantity_and_broken_identity() {
        let blank = MallOrderItemData {
            name_snapshot: "  ".to_string(),
            ..data()
        };
        assert!(MallOrderItem::new(MallOrderItemId::new("i2"), blank).is_err());

        let overlong = MallOrderItemData {
            external_item_id: "x".repeat(129),
            ..data()
        };
        assert!(MallOrderItem::new(MallOrderItemId::new("i3"), overlong).is_err());

        let zero_quantity = MallOrderItemData {
            quantity: Quantity::from_str("0.000000").unwrap(),
            ..data()
        };
        assert!(MallOrderItem::new(MallOrderItemId::new("i4"), zero_quantity).is_err());

        let broken_gross = MallOrderItemData {
            line_gross_amount: Amount::from_str("19.99").unwrap(),
            ..data()
        };
        assert!(MallOrderItem::new(MallOrderItemId::new("i5"), broken_gross).is_err());

        let broken_paid = MallOrderItemData {
            paid_amount: Amount::from_str("19.99").unwrap(),
            ..data()
        };
        assert!(MallOrderItem::new(MallOrderItemId::new("i6"), broken_paid).is_err());
    }

    /// 金额：行内恒等按分舍入（银行家舍入，14.985 → 14.98）。
    #[test]
    fn line_amount_identity_holds_for_rounded_gross() {
        let fractional = MallOrderItemData {
            quantity: Quantity::from_str("1.500000").unwrap(),
            unit_price_gross: UnitPrice::from_str("9.9900").unwrap(),
            line_gross_amount: Amount::from_str("14.98").unwrap(),
            allocated_discount_amount: Amount::from_str("0.00").unwrap(),
            allocated_freight_amount: Amount::from_str("0.00").unwrap(),
            paid_amount: Amount::from_str("14.98").unwrap(),
            ..data()
        };
        assert!(MallOrderItem::new(MallOrderItemId::new("i7"), fractional).is_ok());
    }

    /// 关联不一致：成本税额标识（§12.1.1）——有成本缺标识、含税缺税率、不含税带税率。
    #[test]
    fn new_rejects_inconsistent_cost_tax_fields() {
        let missing_inclusion = MallOrderItemData {
            cost_tax_inclusion: None,
            ..data()
        };
        assert!(MallOrderItem::new(MallOrderItemId::new("i8"), missing_inclusion).is_err());

        let missing_rate = MallOrderItemData {
            cost_input_tax_rate: None,
            ..data()
        };
        assert!(MallOrderItem::new(MallOrderItemId::new("i9"), missing_rate).is_err());

        let exclusive_with_rate = MallOrderItemData {
            cost_tax_inclusion: Some(false),
            cost_input_tax_rate: Some(Rate::from_str("0.060000").unwrap()),
            ..data()
        };
        assert!(MallOrderItem::new(MallOrderItemId::new("i10"), exclusive_with_rate).is_err());

        let no_cost_but_tax_flag = MallOrderItemData {
            unit_cost_snapshot: None,
            cost_snapshot_total: None,
            cost_tax_inclusion: Some(true),
            cost_input_tax_rate: Some(Rate::from_str("0.060000").unwrap()),
            ..data()
        };
        assert!(MallOrderItem::new(MallOrderItemId::new("i11"), no_cost_but_tax_flag).is_err());

        let exclusive_ok = MallOrderItemData {
            cost_tax_inclusion: Some(false),
            cost_input_tax_rate: None,
            ..data()
        };
        assert!(MallOrderItem::new(MallOrderItemId::new("i12"), exclusive_ok).is_ok());
    }

    /// 失败路径：销项税率越界。
    #[test]
    fn new_rejects_out_of_range_sales_tax_rate() {
        let negative = MallOrderItemData {
            sales_tax_rate: Rate::from_str("-0.010000").unwrap(),
            ..data()
        };
        assert!(MallOrderItem::new(MallOrderItemId::new("i13"), negative).is_err());

        let over_unit = MallOrderItemData {
            sales_tax_rate: Rate::from_str("1.010000").unwrap(),
            ..data()
        };
        assert!(MallOrderItem::new(MallOrderItemId::new("i14"), over_unit).is_err());
    }
}
