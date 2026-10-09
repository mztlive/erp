//! 将正式公共行与同一行的不可变子类型快照投影为完整成交明细。

use super::SalesOrderWorkingCopyLineView;
use crate::entity::sales_order::{
    LineType, SalesOrderGoodsServiceLineRevision, SalesOrderRevisionLine, SalesOrderVoucherLineRevision,
};
use crate::{Error, Result};

impl SalesOrderWorkingCopyLineView {
    /// 从同版正式行快照构造成交明细；不查商品当前资料或重新计算金额。
    ///
    /// # 参数
    /// * `line` - 不可变公共行
    /// * `goods` - 与公共行一对一的实物服务快照
    /// * `voucher` - 与公共行一对一的卡券快照
    ///
    /// # 返回
    /// 返回保留稳定明细身份、成交金额、数量、单价与履约期限的视图。
    ///
    /// # 错误
    /// 对应子类型缺失、类型冲突或引用其他公共行时拒绝。
    pub fn from_revision_line(
        line: &SalesOrderRevisionLine,
        goods: Option<&SalesOrderGoodsServiceLineRevision>,
        voucher: Option<&SalesOrderVoucherLineRevision>,
    ) -> Result<Self> {
        let mut view = Self::revision_common_fields(line);
        match (line.line_type, goods, voucher) {
            (LineType::GoodsService, Some(goods), None)
                if goods.revision_line_id.as_ref() == line.base.id =>
            {
                view.apply_goods_snapshot(goods);
            },
            (LineType::Voucher, None, Some(voucher)) if voucher.revision_line_id.as_ref() == line.base.id => {
                view.apply_voucher_snapshot(voucher);
            },
            _ => {
                return Err(Error::ValidationError(format!(
                    "销售版本第 {} 行的成交快照不完整或不匹配",
                    line.line_no
                )));
            },
        }
        Ok(view)
    }

    /// 构造正式公共行的冻结字段与尚未补齐的子类型字段。
    fn revision_common_fields(line: &SalesOrderRevisionLine) -> Self {
        Self {
            id: line.base.id.clone(),
            sales_order_line_id: line.sales_order_line_id.to_string(),
            line_no: line.line_no,
            line_type: line.line_type,
            gross_amount: line.gross_amount,
            net_amount: line.net_amount,
            tax_amount: line.tax_amount,
            sales_tax_rate: line.sales_tax_rate,
            item_name_snapshot: line.item_name_snapshot.clone(),
            spec_snapshot: line.spec_snapshot.clone(),
            unit_snapshot: line.unit_snapshot.clone(),
            sku_id: None,
            sku_revision_id: None,
            welfare_scenario: None,
            service_region: None,
            fulfillment_due_at: None,
            quantity: None,
            base_unit_code: None,
            unit_price_gross: None,
            pricing_mode: Default::default(),
            reference_prices: None,
            face_value: None,
            card_count: None,
            transaction_amount: None,
            card_form: None,
        }
    }

    /// 将同一公共行的实物服务冻结字段写入视图。
    fn apply_goods_snapshot(&mut self, goods: &SalesOrderGoodsServiceLineRevision) {
        self.sku_id = Some(goods.sku_id.clone());
        self.sku_revision_id = Some(goods.sku_revision_id.clone());
        self.welfare_scenario = goods.welfare_scenario;
        self.service_region = goods.service_region.clone();
        self.fulfillment_due_at = Some(goods.fulfillment_due_at.unix_secs() as u64);
        self.quantity = Some(goods.quantity);
        self.base_unit_code = Some(goods.base_unit_code.clone());
        self.unit_price_gross = Some(goods.unit_price_gross);
        self.pricing_mode = goods.pricing_mode;
    }

    /// 将同一公共行的卡券冻结字段写入视图。
    fn apply_voucher_snapshot(&mut self, voucher: &SalesOrderVoucherLineRevision) {
        self.unit_price_gross = Some(voucher.unit_price_gross);
        self.face_value = Some(voucher.face_value);
        self.card_count = Some(voucher.card_count);
        self.transaction_amount = Some(voucher.transaction_amount);
        self.card_form = Some(voucher.card_form);
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use entity_core::BaseModel;
    use erp_core::common::time::Instant;
    use erp_core::ids::{
        SalesOrderLineId, SalesOrderRevisionId, SalesOrderRevisionLineId, SkuId, SkuRevisionId,
    };
    use erp_core::money::{Amount, Quantity, Rate, UnitPrice};

    use super::*;
    use crate::entity::sales_order::{CardForm, SalesPricingMode, WelfareScenario};

    /// 构造金额与成交单价均来自旧 SKU 修订的公共行快照。
    fn line(line_type: LineType) -> SalesOrderRevisionLine {
        let mut base = BaseModel::fake();
        base.id = "revision-line-2".into();
        SalesOrderRevisionLine {
            base,
            sales_order_revision_id: SalesOrderRevisionId::new("revision-2"),
            sales_order_line_id: SalesOrderLineId::new("stable-line-1"),
            line_no: 1,
            line_type,
            gross_amount: Amount::from_str("5200").unwrap(),
            net_amount: Amount::from_str("4601.77").unwrap(),
            tax_amount: Amount::from_str("598.23").unwrap(),
            sales_tax_rate: Rate::from_str("0.13").unwrap(),
            item_name_snapshot: "冻结商品名称".into(),
            spec_snapshot: Some("冻结规格".into()),
            unit_snapshot: Some("盒".into()),
        }
    }

    /// 构造同版实物快照，数量和单价与原提交不同。
    fn goods() -> SalesOrderGoodsServiceLineRevision {
        SalesOrderGoodsServiceLineRevision {
            base: BaseModel::fake(),
            revision_line_id: SalesOrderRevisionLineId::new("revision-line-2"),
            sku_id: SkuId::new("sku-1"),
            sku_revision_id: SkuRevisionId::new("sku-revision-locked"),
            welfare_scenario: Some(WelfareScenario::AnnualGiftBag),
            service_region: Some("EAST".into()),
            fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
            quantity: Quantity::from_str("4").unwrap(),
            base_unit_code: "BOX".into(),
            unit_price_gross: UnitPrice::from_str("1300").unwrap(),
            pricing_mode: SalesPricingMode::Manual,
        }
    }

    /// 构造同版卡券快照。
    fn voucher() -> SalesOrderVoucherLineRevision {
        SalesOrderVoucherLineRevision {
            base: BaseModel::fake(),
            revision_line_id: SalesOrderRevisionLineId::new("revision-line-2"),
            face_value: Amount::from_str("2000").unwrap(),
            card_count: 4,
            unit_price_gross: UnitPrice::from_str("1300").unwrap(),
            face_value_total: Amount::from_str("8000").unwrap(),
            transaction_amount: Amount::from_str("5200").unwrap(),
            gift_amount: Amount::from_str("2800").unwrap(),
            gift_rate: Rate::from_str("0.538462").unwrap(),
            card_form: CardForm::Electronic,
        }
    }

    #[test]
    fn formal_goods_view_preserves_frozen_quantity_price_amount_and_identity() {
        let line = line(LineType::GoodsService);
        let goods = goods();
        let view = SalesOrderWorkingCopyLineView::from_revision_line(&line, Some(&goods), None).unwrap();
        assert_eq!(view.sales_order_line_id, "stable-line-1");
        assert_eq!(view.gross_amount, line.gross_amount);
        assert_eq!(view.quantity, Some(goods.quantity));
        assert_eq!(view.unit_price_gross, Some(goods.unit_price_gross));
        assert_eq!(view.sku_revision_id, Some(goods.sku_revision_id));
        assert_eq!(view.fulfillment_due_at, Some(1_800_000_000));
        assert_eq!(view.item_name_snapshot, "冻结商品名称");
        assert_eq!(view.pricing_mode, SalesPricingMode::Manual);
        assert_eq!(view.reference_prices, None);
        assert_eq!(view.face_value, None);
    }

    #[test]
    fn formal_goods_view_keeps_zero_quantity_without_repricing() {
        let mut goods = goods();
        let mut line = line(LineType::GoodsService);
        line.gross_amount = Amount::from_str("0").unwrap();
        line.net_amount = Amount::from_str("0").unwrap();
        line.tax_amount = Amount::from_str("0").unwrap();
        goods.quantity = Quantity::from_str("0").unwrap();
        goods.pricing_mode = SalesPricingMode::Auto;
        let view = SalesOrderWorkingCopyLineView::from_revision_line(&line, Some(&goods), None).unwrap();
        assert_eq!(view.gross_amount, line.gross_amount);
        assert_eq!(view.quantity, Some(goods.quantity));
        assert_eq!(view.unit_price_gross, Some(goods.unit_price_gross));
        assert_eq!(view.pricing_mode, SalesPricingMode::Auto);
    }

    #[test]
    fn formal_voucher_view_preserves_face_value_count_transaction_and_form() {
        let voucher = voucher();
        let view =
            SalesOrderWorkingCopyLineView::from_revision_line(&line(LineType::Voucher), None, Some(&voucher))
                .unwrap();
        assert_eq!(view.face_value, Some(voucher.face_value));
        assert_eq!(view.card_count, Some(4));
        assert_eq!(view.unit_price_gross, Some(voucher.unit_price_gross));
        assert_eq!(view.transaction_amount, Some(voucher.transaction_amount));
        assert_eq!(view.card_form, Some(CardForm::Electronic));
        assert_eq!(view.quantity, None);
        assert_eq!(view.sku_id, None);
    }

    #[test]
    fn formal_view_rejects_missing_wrong_and_cross_revision_subtype() {
        let line = line(LineType::GoodsService);
        let mut goods = goods();
        let voucher = voucher();
        assert!(SalesOrderWorkingCopyLineView::from_revision_line(&line, None, None).is_err());
        assert!(SalesOrderWorkingCopyLineView::from_revision_line(&line, None, Some(&voucher)).is_err());
        assert!(
            SalesOrderWorkingCopyLineView::from_revision_line(&line, Some(&goods), Some(&voucher)).is_err()
        );
        goods.revision_line_id = SalesOrderRevisionLineId::new("historical-revision-line-1");
        assert!(SalesOrderWorkingCopyLineView::from_revision_line(&line, Some(&goods), None).is_err());
    }
}
