//! 商城订单明细行派生工厂（INT-E02 领域所有权）。
//!
//! 支付载荷中的数量、单价解析与行金额派生只归属本模块；Service 只做字符串
//! 传输与已解析值注入，不复制舍入规则。

use std::str::FromStr;

use crate::errors::Result;
use crate::ids::{MallOrderId, MallOrderItemId};
use crate::mall_order::MallOrderItem;
use crate::mall_order::MallOrderItemData;
use crate::money::{round_to_cent, Amount, Quantity, Rate, UnitPrice};

/// 从付款载荷建立规范化明细行的原始输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MallOrderItemLineInput {
    /// 来源明细身份。
    pub external_item_id: String,
    /// ERP SKU（未映射时为空）。
    pub sku_id: Option<crate::ids::SkuId>,
    /// 下单时发布版本。
    pub product_publication_revision_id: Option<crate::ids::ProductPublicationRevisionId>,
    /// 下单时固定供给。
    pub supplier_offering_revision_id: Option<crate::ids::SupplierOfferingRevisionId>,
    /// 商品名称快照。
    pub name_snapshot: String,
    /// 规格快照。
    pub spec_snapshot: Option<String>,
    /// 数量（已按 wire 精度校验）。
    pub quantity: Quantity,
    /// 含税售价（已按 wire 精度校验）。
    pub unit_price_gross: UnitPrice,
    /// 分摊优惠。
    pub allocated_discount_amount: Amount,
    /// 分摊运费。
    pub allocated_freight_amount: Amount,
    /// 销项税率。
    pub sales_tax_rate: Rate,
    /// 单位供货成本快照。
    pub unit_cost_snapshot: Option<UnitPrice>,
    /// 明细供货成本合计。
    pub cost_snapshot_total: Option<Amount>,
    /// 成本含税标识。
    pub cost_tax_inclusion: Option<bool>,
    /// 成本进项税率。
    pub cost_input_tax_rate: Option<Rate>,
}

impl MallOrderItem {
    /// 从已解析 primitives 建立规范化明细行（INT-E02 唯一派生点）。
    ///
    /// # 用途
    /// 固定 `line_gross = round_half_even(quantity × unit_price_gross, 2)`、
    /// `paid = line_gross - discount + freight`；只对乘积舍入一次，paid 不二次舍入。
    ///
    /// # 参数
    /// * `id` - 新明细主键，由调用方（Service）显式注入
    /// * `order_id` - 所属商城订单，由调用方显式注入
    /// * `input` - 已按 wire 精度校验的数量、单价、金额与税率
    ///
    /// # 返回
    /// 返回通过 [`MallOrderItem::new`] 不变式校验的明细实体。
    ///
    /// # 错误
    /// 行内恒等、数量、税率或成本税额标识非法时返回领域错误。
    ///
    /// # 关键约束
    /// 不做字符串解析、不访问 I/O、不使用全局时钟或 ID 生成器；舍入仅一次银行家舍入。
    pub fn from_line_primitives(
        id: MallOrderItemId,
        order_id: MallOrderId,
        input: MallOrderItemLineInput,
    ) -> Result<Self> {
        let line_gross = Amount::try_from(round_to_cent(
            input.quantity.to_decimal() * input.unit_price_gross.to_decimal(),
        ))?;
        let paid = line_gross
            .checked_sub(input.allocated_discount_amount)
            .checked_add(input.allocated_freight_amount);
        MallOrderItem::new(
            id,
            MallOrderItemData {
                mall_order_id: order_id,
                external_item_id: input.external_item_id,
                sku_id: input.sku_id,
                product_publication_revision_id: input.product_publication_revision_id,
                supplier_offering_revision_id: input.supplier_offering_revision_id,
                name_snapshot: input.name_snapshot,
                spec_snapshot: input.spec_snapshot,
                quantity: input.quantity,
                unit_price_gross: input.unit_price_gross,
                line_gross_amount: line_gross,
                allocated_discount_amount: input.allocated_discount_amount,
                allocated_freight_amount: input.allocated_freight_amount,
                paid_amount: paid,
                sales_tax_rate: input.sales_tax_rate,
                unit_cost_snapshot: input.unit_cost_snapshot,
                cost_snapshot_total: input.cost_snapshot_total,
                cost_tax_inclusion: input.cost_tax_inclusion,
                cost_input_tax_rate: input.cost_input_tax_rate,
            },
        )
    }

    /// 解析 wire 字符串并建立规范化明细行（精度拒绝点）。
    ///
    /// # 用途
    /// 为单测与调用方提供字符串到定点类型的显式转换边界；超精度直接拒绝。
    ///
    /// # 参数
    /// * `id` - 新明细主键，由调用方显式注入
    /// * `order_id` - 所属商城订单，由调用方显式注入
    /// * `external_item_id` - 来源明细身份
    /// * `name_snapshot` - 商品名称快照
    /// * `quantity` - 数量字符串（最多 6 位小数）
    /// * `unit_price_gross` - 含税售价字符串（最多 4 位小数）
    /// * `allocated_discount_amount` - 分摊优惠字符串（最多 2 位）
    /// * `allocated_freight_amount` - 分摊运费字符串（最多 2 位）
    /// * `sales_tax_rate` - 销项税率字符串（最多 6 位）
    ///
    /// # 返回
    /// 返回规范化明细实体。
    ///
    /// # 错误
    /// 任一字符串非法或超精度时返回领域错误。
    ///
    /// # 关键约束
    /// 只做一次乘积舍入；paid 由精确加减派生，不二次舍入。
    #[allow(clippy::too_many_arguments)]
    pub fn from_wire_strings(
        id: MallOrderItemId,
        order_id: MallOrderId,
        external_item_id: String,
        name_snapshot: String,
        quantity: &str,
        unit_price_gross: &str,
        allocated_discount_amount: &str,
        allocated_freight_amount: &str,
        sales_tax_rate: &str,
    ) -> Result<Self> {
        Self::from_line_primitives(
            id,
            order_id,
            MallOrderItemLineInput {
                external_item_id,
                sku_id: None,
                product_publication_revision_id: None,
                supplier_offering_revision_id: None,
                name_snapshot,
                spec_snapshot: None,
                quantity: Quantity::from_str(quantity)?,
                unit_price_gross: UnitPrice::from_str(unit_price_gross)?,
                allocated_discount_amount: Amount::from_str(allocated_discount_amount)?,
                allocated_freight_amount: Amount::from_str(allocated_freight_amount)?,
                sales_tax_rate: Rate::from_str(sales_tax_rate)?,
                unit_cost_snapshot: None,
                cost_snapshot_total: None,
                cost_tax_inclusion: None,
                cost_input_tax_rate: None,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{MallOrderItem, MallOrderItemLineInput};
    use crate::ids::{MallOrderId, MallOrderItemId};
    use crate::money::{Amount, Quantity, Rate, UnitPrice};
    use std::str::FromStr;

    /// 构造最小行输入。
    ///
    /// # 参数
    /// * `quantity` - 数量字符串
    /// * `unit_price` - 含税售价字符串
    ///
    /// # 返回
    /// 返回零优惠运费的测试输入。
    fn line_input(quantity: &str, unit_price: &str) -> MallOrderItemLineInput {
        MallOrderItemLineInput {
            external_item_id: "line-1".to_string(),
            sku_id: None,
            product_publication_revision_id: None,
            supplier_offering_revision_id: None,
            name_snapshot: "item".to_string(),
            spec_snapshot: None,
            quantity: Quantity::from_str(quantity).unwrap(),
            unit_price_gross: UnitPrice::from_str(unit_price).unwrap(),
            allocated_discount_amount: Amount::from_str("0.00").unwrap(),
            allocated_freight_amount: Amount::from_str("0.00").unwrap(),
            sales_tax_rate: Rate::from_str("0.130000").unwrap(),
            unit_cost_snapshot: None,
            cost_snapshot_total: None,
            cost_tax_inclusion: None,
            cost_input_tax_rate: None,
        }
    }

    /// 工厂对乘积只舍入一次，paid 不二次舍入；覆盖 .005/.015/.025 银行家边界。
    #[test]
    fn factory_rounds_product_once_with_half_even() {
        let cases = [
            ("1.000000", "0.0050", "0.00"),
            ("1.000000", "0.0150", "0.02"),
            ("1.000000", "0.0250", "0.02"),
        ];
        for (quantity, price, expected) in cases {
            let item = MallOrderItem::from_line_primitives(
                MallOrderItemId::new(format!("i-{price}")),
                MallOrderId::new("order-1"),
                line_input(quantity, price),
            )
            .unwrap();
            assert_eq!(item.line_gross_amount, Amount::from_str(expected).unwrap());
            assert_eq!(item.paid_amount, Amount::from_str(expected).unwrap());
        }
    }

    /// 折扣、运费、零值按 `paid = gross - discount + freight` 精确派生。
    #[test]
    fn factory_derives_paid_without_second_rounding() {
        let mut input = line_input("2.000000", "9.9900");
        input.allocated_discount_amount = Amount::from_str("0.98").unwrap();
        input.allocated_freight_amount = Amount::from_str("1.00").unwrap();
        let item = MallOrderItem::from_line_primitives(
            MallOrderItemId::new("i-paid"),
            MallOrderId::new("order-1"),
            input,
        )
        .unwrap();
        assert_eq!(item.line_gross_amount, Amount::from_str("19.98").unwrap());
        assert_eq!(item.paid_amount, Amount::from_str("20.00").unwrap());

        let zero = MallOrderItem::from_line_primitives(
            MallOrderItemId::new("i-zero"),
            MallOrderId::new("order-1"),
            line_input("1.000000", "0.0000"),
        )
        .unwrap();
        assert_eq!(zero.line_gross_amount, Amount::from_str("0.00").unwrap());
        assert_eq!(zero.paid_amount, Amount::from_str("0.00").unwrap());
    }

    /// 逐行舍入后表头精确汇总：两行 14.985+14.985 逐行 14.98+14.98=29.96。
    #[test]
    fn per_line_rounding_sums_exactly_at_header() {
        let first = MallOrderItem::from_line_primitives(
            MallOrderItemId::new("i-a"),
            MallOrderId::new("order-1"),
            line_input("1.500000", "9.9900"),
        )
        .unwrap();
        let second = MallOrderItem::from_line_primitives(
            MallOrderItemId::new("i-b"),
            MallOrderId::new("order-1"),
            line_input("1.500000", "9.9900"),
        )
        .unwrap();
        assert_eq!(first.line_gross_amount, Amount::from_str("14.98").unwrap());
        let total = first.paid_amount.checked_add(second.paid_amount);
        assert_eq!(total, Amount::from_str("29.96").unwrap());
    }

    /// 负值 fail-closed：负数量、负单价、负优惠、负运费与优惠超额（实付为负）全部拒绝。
    ///
    /// 行内恒等由 [`MallOrderItem::new`] 拥有；工厂不做容差放行，负值直接返回领域错误。
    #[test]
    fn factory_rejects_negative_values_fail_closed() {
        let negative_quantity = MallOrderItemLineInput {
            quantity: Quantity::from_str("-1.000000").unwrap(),
            ..line_input("1.000000", "9.9900")
        };
        assert!(MallOrderItem::from_line_primitives(
            MallOrderItemId::new("i-neg-qty"),
            MallOrderId::new("order-1"),
            negative_quantity,
        )
        .is_err());

        let negative_price = MallOrderItemLineInput {
            unit_price_gross: UnitPrice::from_str("-9.9900").unwrap(),
            ..line_input("1.000000", "9.9900")
        };
        assert!(MallOrderItem::from_line_primitives(
            MallOrderItemId::new("i-neg-price"),
            MallOrderId::new("order-1"),
            negative_price,
        )
        .is_err());

        let negative_discount = MallOrderItemLineInput {
            allocated_discount_amount: Amount::from_str("-0.01").unwrap(),
            ..line_input("1.000000", "9.9900")
        };
        assert!(MallOrderItem::from_line_primitives(
            MallOrderItemId::new("i-neg-disc"),
            MallOrderId::new("order-1"),
            negative_discount,
        )
        .is_err());

        let negative_freight = MallOrderItemLineInput {
            allocated_freight_amount: Amount::from_str("-0.01").unwrap(),
            ..line_input("1.000000", "9.9900")
        };
        assert!(MallOrderItem::from_line_primitives(
            MallOrderItemId::new("i-neg-freight"),
            MallOrderId::new("order-1"),
            negative_freight,
        )
        .is_err());

        let mut over_discount = line_input("2.000000", "9.9900");
        over_discount.allocated_discount_amount = Amount::from_str("20.00").unwrap();
        assert!(MallOrderItem::from_line_primitives(
            MallOrderItemId::new("i-over-disc"),
            MallOrderId::new("order-1"),
            over_discount,
        )
        .is_err());
    }

    /// 负数乘积同样只舍入一次：`-0.0150 × 1` 按银行家规则得 `-0.02`，行原价为负即拒绝。
    #[test]
    fn factory_rejects_negative_half_even_product() {
        let item = MallOrderItem::from_line_primitives(
            MallOrderItemId::new("i-neg-half-even"),
            MallOrderId::new("order-1"),
            line_input("1.000000", "-0.0150"),
        );
        assert!(item.is_err());
    }

    /// 超精度拒绝：Amount 最多 2 位、UnitPrice 最多 4 位、Quantity/Rate 最多 6 位。
    #[test]
    fn factory_rejects_over_precision_wire_strings() {
        assert!(MallOrderItem::from_wire_strings(
            MallOrderItemId::new("i-e1"),
            MallOrderId::new("order-1"),
            "line-1".to_string(),
            "item".to_string(),
            "1.0000001",
            "9.9900",
            "0.00",
            "0.00",
            "0.130000",
        )
        .is_err());
        assert!(MallOrderItem::from_wire_strings(
            MallOrderItemId::new("i-e2"),
            MallOrderId::new("order-1"),
            "line-1".to_string(),
            "item".to_string(),
            "1.000000",
            "9.99001",
            "0.00",
            "0.00",
            "0.130000",
        )
        .is_err());
        assert!(MallOrderItem::from_wire_strings(
            MallOrderItemId::new("i-e3"),
            MallOrderId::new("order-1"),
            "line-1".to_string(),
            "item".to_string(),
            "1.000000",
            "9.9900",
            "0.001",
            "0.00",
            "0.130000",
        )
        .is_err());
        assert!(MallOrderItem::from_wire_strings(
            MallOrderItemId::new("i-e4"),
            MallOrderId::new("order-1"),
            "line-1".to_string(),
            "item".to_string(),
            "1.000000",
            "9.9900",
            "0.00",
            "0.00",
            "0.1300001",
        )
        .is_err());
    }
}
