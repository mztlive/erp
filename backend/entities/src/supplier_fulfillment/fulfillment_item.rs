//! `supplier_fulfillment_item`（数据模型 §6.19 供应商履约明细）。
//!
//! 随子订单同事务创建，创建后不可修改；数量校验与成本快照恒等见
//! [`SupplierFulfillmentItem::new`]。本模块只持有 `SupplierFulfillmentOrderId`，
//! 不引用订单实体（避免与 `fulfillment_order` 循环依赖）。

use entity_core::BaseModel;
use entity_macros::Entity;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{
    MallOrderItemId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId, SupplierOfferingRevisionId,
};
use crate::money::{round_to_cent, Amount, Quantity, Rate, UnitPrice};
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 供应商侧商品与 SKU 编码快照最大长度。
const SUPPLIER_ITEM_CODE_MAX_LEN: usize = 128;

/// 供应商履约明细创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierFulfillmentItemData {
    /// 所属供应商子订单。
    pub supplier_fulfillment_order_id: SupplierFulfillmentOrderId,
    /// 来源商城商品明细。
    pub mall_order_item_id: MallOrderItemId,
    /// 下单时固定的供给修订。
    pub supplier_offering_revision_id: SupplierOfferingRevisionId,
    /// 下单时固定的供应商侧订货 SKU 编码。
    pub supplier_sku_code_snapshot: String,
    /// 下单时固定的供应商侧商品编码。
    pub supplier_product_code_snapshot: Option<String>,
    /// 整条明细数量（SKU 基础单位，最多 6 位小数）。
    pub quantity: Quantity,
    /// 下单含税单位成本快照（最多 4 位小数）。
    pub unit_cost_snapshot_gross: UnitPrice,
    /// 明细含税成本快照（= 单位成本 × 数量，按分舍入）。
    pub cost_snapshot_total_gross: Amount,
    /// 下单成本进项税率（最多 6 位小数）。
    pub input_tax_rate: Rate,
}

impl SupplierFulfillmentItemData {
    /// 由单位成本与数量构造含税成本快照数据。
    ///
    /// 明细含税总成本统一按分舍入，调用方不得自行重复计算。
    ///
    /// # 参数
    /// * `supplier_fulfillment_order_id` - 所属供应商子订单
    /// * `mall_order_item_id` - 来源商城订单明细
    /// * `supplier_offering_revision_id` - 下单时固定供给修订
    /// * `supplier_sku_code_snapshot` - 供应商 SKU 编码快照
    /// * `supplier_product_code_snapshot` - 供应商商品编码快照
    /// * `quantity` - 下单数量
    /// * `unit_cost_snapshot_gross` - 含税单位成本
    /// * `input_tax_rate` - 进项税率
    ///
    /// # 返回
    /// 返回已派生含税总成本的创建数据。
    #[allow(clippy::too_many_arguments)]
    pub fn from_unit_cost(
        supplier_fulfillment_order_id: SupplierFulfillmentOrderId,
        mall_order_item_id: MallOrderItemId,
        supplier_offering_revision_id: SupplierOfferingRevisionId,
        supplier_sku_code_snapshot: impl Into<String>,
        supplier_product_code_snapshot: Option<String>,
        quantity: Quantity,
        unit_cost_snapshot_gross: UnitPrice,
        input_tax_rate: Rate,
    ) -> Result<Self> {
        let cost_snapshot_total_gross = Amount::try_from(round_to_cent(
            unit_cost_snapshot_gross.to_decimal() * quantity.to_decimal(),
        ))
        .map_err(|_| Error::from("明细成本快照金额无效"))?;
        Ok(Self {
            supplier_fulfillment_order_id,
            mall_order_item_id,
            supplier_offering_revision_id,
            supplier_sku_code_snapshot: supplier_sku_code_snapshot.into(),
            supplier_product_code_snapshot,
            quantity,
            unit_cost_snapshot_gross,
            cost_snapshot_total_gross,
            input_tax_rate,
        })
    }
}

/// 供应商履约明细实体（数据模型 §6.19）。
///
/// 随子订单同事务创建，创建后不可修改（后续供给关系变化不影响已支付订单）；
/// 一条商城商品明细只属于一个供应商子订单（跨记录唯一性由唯一索引保证，P3）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierFulfillmentItem {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属供应商子订单。
    pub supplier_fulfillment_order_id: SupplierFulfillmentOrderId,
    /// 来源商城商品明细。
    pub mall_order_item_id: MallOrderItemId,
    /// 下单时固定的供给修订。
    pub supplier_offering_revision_id: SupplierOfferingRevisionId,
    /// 下单时固定的供应商侧订货 SKU 编码。
    pub supplier_sku_code_snapshot: String,
    /// 下单时固定的供应商侧商品编码。
    pub supplier_product_code_snapshot: Option<String>,
    /// 整条明细数量。
    pub quantity: Quantity,
    /// 下单含税单位成本快照。
    pub unit_cost_snapshot_gross: UnitPrice,
    /// 明细含税成本快照。
    pub cost_snapshot_total_gross: Amount,
    /// 下单成本进项税率。
    pub input_tax_rate: Rate,
}

impl SupplierFulfillmentItem {
    /// 创建供应商履约明细。
    ///
    /// 校验数量大于零，并强制成本快照恒等（§4.2：明细含税成本必须等于
    /// 单位成本 × 数量后按分舍入）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierFulfillmentItemId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的明细实体。
    ///
    /// # 错误
    /// 数量非正、成本快照与单价数量不一致或金额精度非法时返回错误。
    pub fn new(id: SupplierFulfillmentItemId, data: SupplierFulfillmentItemData) -> Result<Self> {
        ensure_positive_quantity(data.quantity, "明细数量必须大于零")?;
        ensure_snapshot_consistent(
            data.quantity,
            data.unit_cost_snapshot_gross,
            data.cost_snapshot_total_gross,
        )?;
        let supplier_sku_code_snapshot = normalize_required_text(
            data.supplier_sku_code_snapshot,
            "供应商 SKU 编码快照不能为空",
            SUPPLIER_ITEM_CODE_MAX_LEN,
            "供应商 SKU 编码快照过长",
        )?;
        let supplier_product_code_snapshot = normalize_optional_text(
            data.supplier_product_code_snapshot,
            "供应商商品编码快照",
            SUPPLIER_ITEM_CODE_MAX_LEN,
        )?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            supplier_fulfillment_order_id: data.supplier_fulfillment_order_id,
            mall_order_item_id: data.mall_order_item_id,
            supplier_offering_revision_id: data.supplier_offering_revision_id,
            supplier_sku_code_snapshot,
            supplier_product_code_snapshot,
            quantity: data.quantity,
            unit_cost_snapshot_gross: data.unit_cost_snapshot_gross,
            cost_snapshot_total_gross: data.cost_snapshot_total_gross,
            input_tax_rate: data.input_tax_rate,
        })
    }

    /// 判断明细是否属于指定供应商履约订单。
    ///
    /// # 参数
    /// * `order_id` - 供应商履约订单主键
    ///
    /// # 返回
    /// 归属一致时返回 `true`。
    pub fn belongs_to_order(&self, order_id: &SupplierFulfillmentOrderId) -> bool {
        self.supplier_fulfillment_order_id == *order_id
    }
}

/// 校验数量大于零。
///
/// # 参数
/// * `value` - 数量
/// * `message` - 失败时的错误信息
///
/// # 错误
/// 数量小于等于零时返回错误。
fn ensure_positive_quantity(value: Quantity, message: &str) -> Result<()> {
    if value.to_decimal() <= Decimal::ZERO {
        return Err(Error::from(message));
    }
    Ok(())
}

/// 校验明细成本快照恒等：`round_to_cent(单位成本 × 数量) == 明细成本`（§4.2 铁律 1）。
///
/// # 参数
/// * `quantity` - 数量
/// * `unit_cost` - 含税单位成本
/// * `total_gross` - 明细含税成本
///
/// # 错误
/// 恒等式不成立或舍入结果无法表示为 `Amount` 时返回错误。
fn ensure_snapshot_consistent(quantity: Quantity, unit_cost: UnitPrice, total_gross: Amount) -> Result<()> {
    let expected = Amount::try_from(round_to_cent(unit_cost.to_decimal() * quantity.to_decimal()))
        .map_err(|_| Error::from("明细成本快照金额无效"))?;
    if expected != total_gross {
        return Err(Error::from("明细成本快照必须等于下单单位成本 × 数量并舍入到分"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn sample_item_data() -> SupplierFulfillmentItemData {
        SupplierFulfillmentItemData {
            supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new("order-1"),
            mall_order_item_id: MallOrderItemId::new("mall-item-1"),
            supplier_offering_revision_id: SupplierOfferingRevisionId::new("offering-rev-1"),
            supplier_sku_code_snapshot: "SUP-SKU-1".to_string(),
            supplier_product_code_snapshot: Some("SUP-SPU-1".to_string()),
            quantity: Quantity::from_str("3.000000").unwrap(),
            unit_cost_snapshot_gross: UnitPrice::from_str("9.9900").unwrap(),
            cost_snapshot_total_gross: Amount::from_str("29.97").unwrap(),
            input_tax_rate: Rate::from_str("0.130000").unwrap(),
        }
    }

    #[test]
    fn item_new_accepts_valid_item_and_keeps_snapshot() {
        let item = SupplierFulfillmentItem::new(SupplierFulfillmentItemId::new("item-1"), sample_item_data())
            .unwrap();

        assert_eq!(item.quantity, Quantity::from_str("3.000000").unwrap());
        assert_eq!(item.cost_snapshot_total_gross, Amount::from_str("29.97").unwrap());
        assert_eq!(item.input_tax_rate, Rate::from_str("0.130000").unwrap());
        assert_eq!(item.supplier_sku_code_snapshot, "SUP-SKU-1");
        assert_eq!(item.supplier_product_code_snapshot.as_deref(), Some("SUP-SPU-1"));
        assert!(item.belongs_to_order(&SupplierFulfillmentOrderId::new("order-1")));
        assert!(!item.belongs_to_order(&SupplierFulfillmentOrderId::new("order-2")));
    }

    #[test]
    fn data_factory_calculates_total_cost_snapshot() {
        let data = SupplierFulfillmentItemData::from_unit_cost(
            SupplierFulfillmentOrderId::new("order-1"),
            MallOrderItemId::new("mall-item-1"),
            SupplierOfferingRevisionId::new("offering-rev-1"),
            "SUP-SKU-1",
            None,
            Quantity::from_str("3").unwrap(),
            UnitPrice::from_str("9.99").unwrap(),
            Rate::from_str("0.13").unwrap(),
        )
        .unwrap();
        assert_eq!(data.cost_snapshot_total_gross, Amount::from_str("29.97").unwrap());
    }

    #[test]
    fn item_new_rejects_blank_supplier_sku_snapshot() {
        let data = SupplierFulfillmentItemData {
            supplier_sku_code_snapshot: "   ".to_string(),
            ..sample_item_data()
        };
        assert!(SupplierFulfillmentItem::new(SupplierFulfillmentItemId::new("item-blank"), data).is_err());
    }

    #[test]
    fn item_new_rejects_non_positive_quantity() {
        for quantity in [
            Quantity::from_str("0.000000").unwrap(),
            Quantity::from_str("-1.000000").unwrap(),
        ] {
            let data = SupplierFulfillmentItemData {
                quantity,
                ..sample_item_data()
            };
            assert!(SupplierFulfillmentItem::new(SupplierFulfillmentItemId::new("item-2"), data).is_err());
        }
    }

    #[test]
    fn item_new_rejects_inconsistent_cost_snapshot() {
        let data = SupplierFulfillmentItemData {
            cost_snapshot_total_gross: Amount::from_str("30.00").unwrap(),
            ..sample_item_data()
        };
        assert!(SupplierFulfillmentItem::new(SupplierFulfillmentItemId::new("item-3"), data).is_err());
    }

    #[test]
    fn item_new_rejects_over_scale_money() {
        assert!(
            UnitPrice::from_str("9.99999").is_err(),
            "单价最多 4 位小数，超位必须拒绝"
        );
        assert!(
            Quantity::from_str("1.0000001").is_err(),
            "数量最多 6 位小数，超位必须拒绝"
        );
    }
}
