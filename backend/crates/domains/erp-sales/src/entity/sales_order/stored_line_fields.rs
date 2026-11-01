//! 从草稿和冻结提交的扁平存储字段还原同一销售行字段组。

use erp_core::common::time::Instant;
use erp_core::ids::{SkuId, SkuRevisionId};
use erp_core::money::{Amount, Quantity, Rate, UnitPrice};
use erp_core::{Error, Result};

use super::{
    CardForm, GoodsLineFields, LineType, SalesDeliveryAllocation, SalesOrderSubmissionLine,
    SalesOrderWorkingCopyLine, SalesPricingMode, SalesProcurementPriceBasis, VoucherLineDraft,
    WelfareScenario,
};
use crate::entity::sales_review::SalesChangeSubmissionLine;

/// 三种行记录共用的字段组读取输入；不持久化，不包含所属版本身份。
pub(crate) struct StoredLineFields<'a> {
    line_no: u32,
    line_type: LineType,
    pricing_mode: SalesPricingMode,
    delivery_allocations: &'a [SalesDeliveryAllocation],
    sku_id: Option<&'a SkuId>,
    sku_revision_id: Option<&'a SkuRevisionId>,
    welfare_scenario: Option<WelfareScenario>,
    procurement_supply_price_gross: Option<UnitPrice>,
    procurement_price_basis: Option<SalesProcurementPriceBasis>,
    fulfillment_due_at: Option<Instant>,
    quantity: Option<Quantity>,
    base_unit_code: Option<&'a str>,
    unit_price_gross: Option<UnitPrice>,
    face_value: Option<Amount>,
    card_count: Option<u32>,
    face_value_total: Option<Amount>,
    transaction_amount: Option<Amount>,
    gift_amount: Option<Amount>,
    gift_rate: Option<Rate>,
    card_form: Option<CardForm>,
}

// 仅复用存储字段投影；字段是否必填以及报错顺序由下面的构造方法唯一维护。
macro_rules! stored_line_fields {
    ($($source:ty),+ $(,)?) => {
        $(impl<'a> From<&'a $source> for StoredLineFields<'a> {
            fn from(line: &'a $source) -> Self {
                Self {
                    line_no: line.line_no,
                    line_type: line.line_type,
                    pricing_mode: line.pricing_mode,
                    delivery_allocations: &line.delivery_allocations,
                    sku_id: line.sku_id.as_ref(),
                    sku_revision_id: line.sku_revision_id.as_ref(),
                    welfare_scenario: line.welfare_scenario,
                    procurement_supply_price_gross: line.procurement_supply_price_gross,
                    procurement_price_basis: line.procurement_price_basis,
                    fulfillment_due_at: line.fulfillment_due_at,
                    quantity: line.quantity,
                    base_unit_code: line.base_unit_code.as_deref(),
                    unit_price_gross: line.unit_price_gross,
                    face_value: line.face_value,
                    card_count: line.card_count,
                    face_value_total: line.face_value_total,
                    transaction_amount: line.transaction_amount,
                    gift_amount: line.gift_amount,
                    gift_rate: line.gift_rate,
                    card_form: line.card_form,
                }
            }
        })+
    };
}

stored_line_fields!(SalesOrderWorkingCopyLine, SalesOrderSubmissionLine, SalesChangeSubmissionLine);

impl StoredLineFields<'_> {
    /// 还原实物及服务字段，沿用扁平快照的必填检查顺序。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回保留可选字段及冻结配送分配的商品字段组。
    ///
    /// # 错误
    /// 行类型不符或 SKU、修订、履约期限、数量、单位、成交价缺失时返回原有错误。
    pub(crate) fn goods(&self) -> Result<GoodsLineFields> {
        if self.line_type != LineType::GoodsService {
            return Err(Error::from(format!("第 {} 行不是实物及服务行", self.line_no)));
        }
        Ok(GoodsLineFields {
            pricing_mode: self.pricing_mode,
            delivery_allocations: self.delivery_allocations.to_vec(),
            sku_id: self.required(self.sku_id, "商品字段组")?.clone(),
            sku_revision_id: self.required(self.sku_revision_id, " SKU 修订")?.clone(),
            welfare_scenario: self.welfare_scenario,
            procurement_supply_price_gross: self.procurement_supply_price_gross,
            procurement_price_basis: self.procurement_price_basis,
            fulfillment_due_at: self.required(self.fulfillment_due_at, "履约期限")?,
            quantity: self.required(self.quantity, "数量")?,
            base_unit_code: self.required(self.base_unit_code, "单位")?.to_string(),
            unit_price_gross: self.required(self.unit_price_gross, "含税单价")?,
        })
    }

    /// 还原卡券字段，不重算快照金额或可选配赠率。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回可交给正式卡券行构造的字段组。
    ///
    /// # 错误
    /// 行类型不符或卡券任一必填字段缺失时返回原有错误。
    pub(crate) fn voucher(&self) -> Result<VoucherLineDraft> {
        if self.line_type != LineType::Voucher {
            return Err(Error::from(format!("第 {} 行不是卡券行", self.line_no)));
        }
        Ok(VoucherLineDraft {
            face_value: self.required(self.face_value, "卡券字段组")?,
            card_count: self.required(self.card_count, "卡张数")?,
            unit_price_gross: self.required(self.unit_price_gross, "卡券成交单价")?,
            face_value_total: self.required(self.face_value_total, "面额小计")?,
            transaction_amount: self.required(self.transaction_amount, "成交金额")?,
            gift_amount: self.required(self.gift_amount, "配赠金额")?,
            gift_rate: self.gift_rate,
            card_form: self.required(self.card_form, "卡形态")?,
        })
    }

    fn required<T>(&self, value: Option<T>, field: &str) -> Result<T> {
        value.ok_or_else(|| Error::from(format!("第 {} 行缺少{field}", self.line_no)))
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{
        SalesChangeSubmissionId, SalesChangeSubmissionLineId, SalesOrderSubmissionId,
        SalesOrderSubmissionLineId, SalesOrderWorkingCopyId, SalesOrderWorkingCopyLineId,
    };

    use super::*;
    use crate::entity::sales_order::SalesOrderSubmissionLineData;
    use crate::entity::sales_order::working_copy_test_support::{amt, line_data, price};
    use crate::entity::sales_review::SalesChangeSubmissionLineData;

    fn working_line(kind: LineType) -> SalesOrderWorkingCopyLine {
        let mut data = line_data(7);
        if kind == LineType::Voucher {
            data.line_type = kind;
            data.goods = None;
            data.voucher = Some(VoucherLineDraft {
                face_value: amt("100.00"),
                card_count: 3,
                unit_price_gross: price("90.0000"),
                face_value_total: amt("300.00"),
                transaction_amount: amt("270.00"),
                gift_amount: amt("30.00"),
                gift_rate: None,
                card_form: CardForm::Electronic,
            });
        }
        SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("working-line"),
            SalesOrderWorkingCopyId::new("working"),
            data,
        )
        .unwrap()
    }

    fn frozen_lines(
        working: &SalesOrderWorkingCopyLine,
    ) -> (SalesOrderSubmissionLine, SalesChangeSubmissionLine) {
        let submitted = SalesOrderSubmissionLine::new(
            SalesOrderSubmissionLineId::new("submitted-line"),
            SalesOrderSubmissionId::new("submitted"),
            SalesOrderSubmissionLineData::from_working_copy_line(working).unwrap(),
        )
        .unwrap();
        let changed = SalesChangeSubmissionLine::new(
            SalesChangeSubmissionLineId::new("change-line"),
            SalesChangeSubmissionId::new("change"),
            SalesChangeSubmissionLineData::from_sales_working_copy(working).unwrap(),
        )
        .unwrap();
        (submitted, changed)
    }

    #[test]
    fn goods_groups_from_all_storage_sources_preserve_frozen_values() {
        let working = working_line(LineType::GoodsService);
        let expected = working.goods_fields().unwrap().unwrap();
        let (submitted, changed) = frozen_lines(&working);
        assert_eq!(submitted.goods_fields().unwrap(), expected);
        assert_eq!(changed.goods_fields().unwrap(), expected);
        assert_eq!(expected.sku_revision_id.as_ref(), "skurev-1");
        assert_eq!(expected.procurement_supply_price_gross, Some(price("8.5000")));
        assert_eq!(expected.quantity.to_decimal().to_string(), "3.000000");
        assert!(working.voucher_fields().unwrap().is_none());
        for error in [submitted.voucher_fields().unwrap_err(), changed.voucher_fields().unwrap_err()] {
            assert_eq!(error.to_string(), "第 7 行不是卡券行");
        }
    }

    #[test]
    fn voucher_groups_from_all_storage_sources_keep_amounts_and_optional_rate() {
        let mut working = working_line(LineType::Voucher);
        working.gift_rate = None;
        let expected = working.voucher_fields().unwrap().unwrap();
        let (mut submitted, mut changed) = frozen_lines(&working);
        // 持久化历史可以没有可选配赠率；读取字段组不能用重算值替代它。
        submitted.gift_rate = None;
        changed.gift_rate = None;
        assert_eq!(submitted.voucher_fields().unwrap(), expected);
        let changed: VoucherLineDraft = changed.voucher_fields().unwrap().into();
        assert_eq!(changed, expected);
        assert_eq!(expected.transaction_amount, amt("270.00"));
        assert!(expected.gift_rate.is_none());
        assert!(working.goods_fields().unwrap().is_none());
        assert_eq!(submitted.goods_fields().unwrap_err().to_string(), "第 7 行不是实物及服务行");
    }

    #[test]
    fn goods_required_fields_keep_the_first_error_and_exact_messages() {
        let working = working_line(LineType::GoodsService);
        let mut fields = StoredLineFields::from(&working);
        fields.sku_id = None;
        fields.sku_revision_id = None;
        fields.fulfillment_due_at = None;
        fields.quantity = None;
        fields.base_unit_code = None;
        fields.unit_price_gross = None;
        assert_eq!(fields.goods().unwrap_err().to_string(), "第 7 行缺少商品字段组");
        fields.sku_id = working.sku_id.as_ref();
        assert_eq!(fields.goods().unwrap_err().to_string(), "第 7 行缺少 SKU 修订");
        fields.sku_revision_id = working.sku_revision_id.as_ref();
        assert_eq!(fields.goods().unwrap_err().to_string(), "第 7 行缺少履约期限");
        fields.fulfillment_due_at = working.fulfillment_due_at;
        assert_eq!(fields.goods().unwrap_err().to_string(), "第 7 行缺少数量");
        fields.quantity = working.quantity;
        assert_eq!(fields.goods().unwrap_err().to_string(), "第 7 行缺少单位");
        fields.base_unit_code = working.base_unit_code.as_deref();
        assert_eq!(fields.goods().unwrap_err().to_string(), "第 7 行缺少含税单价");
    }

    #[test]
    fn voucher_required_fields_keep_the_first_error_and_exact_messages() {
        let working = working_line(LineType::Voucher);
        let mut fields = StoredLineFields::from(&working);
        fields.face_value = None;
        fields.card_count = None;
        fields.unit_price_gross = None;
        fields.face_value_total = None;
        fields.transaction_amount = None;
        fields.gift_amount = None;
        fields.card_form = None;
        assert_eq!(fields.voucher().unwrap_err().to_string(), "第 7 行缺少卡券字段组");
        fields.face_value = working.face_value;
        assert_eq!(fields.voucher().unwrap_err().to_string(), "第 7 行缺少卡张数");
        fields.card_count = working.card_count;
        assert_eq!(fields.voucher().unwrap_err().to_string(), "第 7 行缺少卡券成交单价");
        fields.unit_price_gross = working.unit_price_gross;
        assert_eq!(fields.voucher().unwrap_err().to_string(), "第 7 行缺少面额小计");
        fields.face_value_total = working.face_value_total;
        assert_eq!(fields.voucher().unwrap_err().to_string(), "第 7 行缺少成交金额");
        fields.transaction_amount = working.transaction_amount;
        assert_eq!(fields.voucher().unwrap_err().to_string(), "第 7 行缺少配赠金额");
        fields.gift_amount = working.gift_amount;
        assert_eq!(fields.voucher().unwrap_err().to_string(), "第 7 行缺少卡形态");
    }
}
