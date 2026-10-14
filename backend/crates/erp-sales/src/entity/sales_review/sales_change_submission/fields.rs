//! 从销售变更提交行恢复商品与卡券字段快照。
use super::{Error, GoodsLineFields, LineType, Result, SalesChangeSubmissionLine, VoucherLineDraft};

impl SalesChangeSubmissionLine {
    /// 还原实物及服务字段组。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回用于正式版本行构造的完整实物及服务字段组。
    ///
    /// # 错误
    /// 行类型不是实物及服务，或任一必填字段缺失时返回错误。
    ///
    /// # 关键业务约束
    /// 可选的福利场景和服务区域缺失时必须保留为 `None`，不得填默认值。
    pub fn goods_fields(&self) -> Result<GoodsLineFields> {
        if self.line_type != LineType::GoodsService {
            return Err(Error::from(format!("第 {} 行不是实物及服务行", self.line_no)));
        }
        Ok(GoodsLineFields {
            pricing_mode: self.pricing_mode,
            delivery_allocations: self.delivery_allocations.clone(),
            sku_id: self
                .sku_id
                .clone()
                .ok_or_else(|| Error::from(format!("第 {} 行缺少商品字段组", self.line_no)))?,
            sku_revision_id: self
                .sku_revision_id
                .clone()
                .ok_or_else(|| Error::from(format!("第 {} 行缺少 SKU 修订", self.line_no)))?,
            welfare_scenario: self.welfare_scenario,
            procurement_supply_price_gross: self.procurement_supply_price_gross,
            fulfillment_due_at: self
                .fulfillment_due_at
                .ok_or_else(|| Error::from(format!("第 {} 行缺少履约期限", self.line_no)))?,
            quantity: self.quantity.ok_or_else(|| Error::from(format!("第 {} 行缺少数量", self.line_no)))?,
            base_unit_code: self
                .base_unit_code
                .clone()
                .ok_or_else(|| Error::from(format!("第 {} 行缺少单位", self.line_no)))?,
            unit_price_gross: self
                .unit_price_gross
                .ok_or_else(|| Error::from(format!("第 {} 行缺少含税单价", self.line_no)))?,
        })
    }

    /// 还原卡券字段组。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回用于正式版本行构造的完整卡券字段组。
    ///
    /// # 错误
    /// 行类型不是卡券，或任一必填字段缺失时返回错误。
    ///
    /// # 关键业务约束
    /// 可选配赠率缺失时必须保留为 `None`，由卡券行实体按公式推导。
    pub fn voucher_fields(&self) -> Result<VoucherLineDraft> {
        if self.line_type != LineType::Voucher {
            return Err(Error::from(format!("第 {} 行不是卡券行", self.line_no)));
        }
        Ok(VoucherLineDraft {
            face_value: self
                .face_value
                .ok_or_else(|| Error::from(format!("第 {} 行缺少卡券字段组", self.line_no)))?,
            card_count: self
                .card_count
                .ok_or_else(|| Error::from(format!("第 {} 行缺少卡张数", self.line_no)))?,
            unit_price_gross: self
                .unit_price_gross
                .ok_or_else(|| Error::from(format!("第 {} 行缺少卡券成交单价", self.line_no)))?,
            face_value_total: self
                .face_value_total
                .ok_or_else(|| Error::from(format!("第 {} 行缺少面额小计", self.line_no)))?,
            transaction_amount: self
                .transaction_amount
                .ok_or_else(|| Error::from(format!("第 {} 行缺少成交金额", self.line_no)))?,
            gift_amount: self
                .gift_amount
                .ok_or_else(|| Error::from(format!("第 {} 行缺少配赠金额", self.line_no)))?,
            gift_rate: self.gift_rate,
            card_form: self
                .card_form
                .ok_or_else(|| Error::from(format!("第 {} 行缺少卡形态", self.line_no)))?,
        })
    }
}
