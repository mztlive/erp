//! `sales_order_working_copy_line`（数据模型 §6.5）。
//!
//! 工作副本行是草稿的组成部分，`(working_copy_id, sales_order_line_id)` 唯一，
//! 行号与金额由 [`SalesOrderWorkingCopyLine::new`] 校验并推导。本模块只依赖
//! common/ids/money/validation 与 `super::types` 的行字段组，不依赖表头实体。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{SalesOrderLineId, SalesOrderWorkingCopyId, SalesOrderWorkingCopyLineId, SkuId};
use crate::money::{Amount, Quantity, Rate, UnitPrice};
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::amount_validation::sum_line_amounts;
use super::types::{build_line_groups, GoodsLineFields, LineType, VoucherLineDraft, WelfareScenario};

/// 销售项名称快照最大长度。
const ITEM_NAME_MAX_LEN: usize = 256;
/// 规格快照最大长度。
const SPEC_MAX_LEN: usize = 256;
/// 单位快照最大长度。
const UNIT_MAX_LEN: usize = 64;

/// 工作副本行创建数据（行字段组按 `line_type` 二选一）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderWorkingCopyLineData {
    /// 稳定明细身份。
    pub sales_order_line_id: SalesOrderLineId,
    /// 行号。
    pub line_no: u32,
    /// 行类型。
    pub line_type: LineType,
    /// 销项税率。
    pub sales_tax_rate: Rate,
    /// 销售项名称快照。
    pub item_name_snapshot: String,
    /// 规格快照。
    pub spec_snapshot: Option<String>,
    /// 单位快照。
    pub unit_snapshot: Option<String>,
    /// 实物及服务字段组。
    pub goods: Option<GoodsLineFields>,
    /// 卡券字段组。
    pub voucher: Option<VoucherLineDraft>,
}

/// 工作副本行实体（数据模型 §6.5：`(working_copy_id, sales_order_line_id)` 唯一）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesOrderWorkingCopyLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属工作副本。
    pub working_copy_id: SalesOrderWorkingCopyId,
    /// 稳定明细身份。
    pub sales_order_line_id: SalesOrderLineId,
    /// 行号。
    pub line_no: u32,
    /// 行类型。
    pub line_type: LineType,
    /// 行含税金额。
    pub gross_amount: Amount,
    /// 行不含税金额。
    pub net_amount: Amount,
    /// 行税额。
    pub tax_amount: Amount,
    /// 销项税率。
    pub sales_tax_rate: Rate,
    /// 销售项名称快照。
    pub item_name_snapshot: String,
    /// 规格快照。
    pub spec_snapshot: Option<String>,
    /// 单位快照。
    pub unit_snapshot: Option<String>,
    /// 正式销售项 SKU。
    pub sku_id: Option<SkuId>,
    /// 精确 SKU 修订。
    pub sku_revision_id: Option<crate::ids::SkuRevisionId>,
    /// 福利场景。
    pub welfare_scenario: Option<WelfareScenario>,
    /// 采购责任解析使用的服务区域。
    pub service_region: Option<String>,
    /// 公司对客户承诺完成本明细交付或服务的最晚时间。
    pub fulfillment_due_at: Option<Instant>,
    /// 基础单位数量。
    pub quantity: Option<Quantity>,
    /// 基础单位代码。
    pub base_unit_code: Option<String>,
    /// 含税成交单价快照。
    pub unit_price_gross: Option<UnitPrice>,
    /// 单卡面额。
    pub face_value: Option<Amount>,
    /// 卡张数。
    pub card_count: Option<u32>,
    /// 面额小计。
    pub face_value_total: Option<Amount>,
    /// 最终成交金额。
    pub transaction_amount: Option<Amount>,
    /// 配赠金额。
    pub gift_amount: Option<Amount>,
    /// 配赠率。
    pub gift_rate: Option<Rate>,
    /// 卡形态。
    pub card_form: Option<super::types::CardForm>,
}

impl SalesOrderWorkingCopyLine {
    /// 创建工作副本行。
    ///
    /// 完成文本字段校验与规范化，行金额三元组按
    /// [`crate::money::line_amounts`] 统一计算（§4.2 逐行舍入）；卡券行按 §6.4
    /// 校验面额小计、成交金额与配赠金额一致性。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesOrderWorkingCopyLineId`）
    /// * `working_copy_id` - 所属工作副本
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的工作副本行。
    ///
    /// # 错误
    /// 行号为零、必填为空、超长、行类型与字段组不一致或卡券金额不一致时返回错误。
    pub fn new(
        id: SalesOrderWorkingCopyLineId,
        working_copy_id: SalesOrderWorkingCopyId,
        data: SalesOrderWorkingCopyLineData,
    ) -> Result<Self> {
        if data.line_no == 0 {
            return Err(Error::from("行号必须为正整数"));
        }
        let item_name_snapshot = normalize_required_text(
            data.item_name_snapshot,
            "销售项名称不能为空",
            ITEM_NAME_MAX_LEN,
            "销售项名称过长",
        )?;
        let spec_snapshot = normalize_optional_text(data.spec_snapshot, "规格", SPEC_MAX_LEN)?;
        let unit_snapshot = normalize_optional_text(data.unit_snapshot, "单位", UNIT_MAX_LEN)?;
        let built = build_line_groups(data.line_type, data.goods, data.voucher, data.sales_tax_rate)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            working_copy_id,
            sales_order_line_id: data.sales_order_line_id,
            line_no: data.line_no,
            line_type: data.line_type,
            gross_amount: built.gross_amount,
            net_amount: built.net_amount,
            tax_amount: built.tax_amount,
            sales_tax_rate: data.sales_tax_rate,
            item_name_snapshot,
            spec_snapshot,
            unit_snapshot,
            sku_id: built.goods.as_ref().map(|g| g.sku_id.clone()),
            sku_revision_id: built.goods.as_ref().map(|g| g.sku_revision_id.clone()),
            welfare_scenario: built.goods.as_ref().and_then(|g| g.welfare_scenario),
            service_region: built.goods.as_ref().and_then(|g| g.service_region.clone()),
            fulfillment_due_at: built.goods.as_ref().map(|g| g.fulfillment_due_at),
            quantity: built.goods.as_ref().map(|g| g.quantity),
            base_unit_code: built.goods.as_ref().map(|g| g.base_unit_code.clone()),
            unit_price_gross: built
                .goods
                .as_ref()
                .map(|g| g.unit_price_gross)
                .or_else(|| built.voucher.as_ref().map(|v| v.unit_price_gross)),
            face_value: built.voucher.as_ref().map(|v| v.face_value),
            card_count: built.voucher.as_ref().map(|v| v.card_count),
            face_value_total: built.voucher.as_ref().map(|v| v.face_value_total),
            transaction_amount: built.voucher.as_ref().map(|v| v.transaction_amount),
            gift_amount: built.voucher.as_ref().map(|v| v.gift_amount),
            gift_rate: built.voucher.as_ref().map(|v| v.gift_rate),
            card_form: built.voucher.as_ref().map(|v| v.card_form),
        })
    }

    /// 判断明细是否属于给定工作副本。
    ///
    /// # 参数
    /// * `working_copy_id` - 待校验的工作副本
    ///
    /// # 返回
    /// 明细所属工作副本一致时返回 `true`。
    pub fn belongs_to_working_copy(&self, working_copy_id: &SalesOrderWorkingCopyId) -> bool {
        &self.working_copy_id == working_copy_id
    }

    /// 返回公司商品池可售校验所需的精确 SKU 引用。
    ///
    /// # 返回
    /// 卡券行返回 `None`；实物及服务行返回 `(SKU, SKU 修订)`。
    ///
    /// # 错误
    /// 实物及服务行缺少 SKU 或 SKU 修订时返回错误。
    pub fn sellable_sku_ref(&self) -> Result<Option<(&SkuId, &crate::ids::SkuRevisionId)>> {
        if self.line_type != LineType::GoodsService {
            return Ok(None);
        }
        let sku_id = self
            .sku_id
            .as_ref()
            .ok_or_else(|| Error::from(format!("第 {} 行缺少 SKU", self.line_no)))?;
        let revision_id = self
            .sku_revision_id
            .as_ref()
            .ok_or_else(|| Error::from(format!("第 {} 行缺少 SKU 修订", self.line_no)))?;
        Ok(Some((sku_id, revision_id)))
    }

    /// 还原实物及服务字段组。
    ///
    /// # 返回
    /// 实物及服务行返回完整字段组；卡券行返回 `None`。
    ///
    /// # 错误
    /// 实物及服务行缺少任一必填字段时返回错误。
    pub fn goods_fields(&self) -> Result<Option<GoodsLineFields>> {
        if self.line_type != LineType::GoodsService {
            return Ok(None);
        }
        Ok(Some(GoodsLineFields {
            sku_id: self
                .sku_id
                .clone()
                .ok_or_else(|| Error::from(format!("第 {} 行缺少商品字段组", self.line_no)))?,
            sku_revision_id: self
                .sku_revision_id
                .clone()
                .ok_or_else(|| Error::from(format!("第 {} 行缺少 SKU 修订", self.line_no)))?,
            welfare_scenario: self.welfare_scenario,
            service_region: self.service_region.clone(),
            fulfillment_due_at: self
                .fulfillment_due_at
                .ok_or_else(|| Error::from(format!("第 {} 行缺少履约期限", self.line_no)))?,
            quantity: self
                .quantity
                .ok_or_else(|| Error::from(format!("第 {} 行缺少数量", self.line_no)))?,
            base_unit_code: self
                .base_unit_code
                .clone()
                .ok_or_else(|| Error::from(format!("第 {} 行缺少单位", self.line_no)))?,
            unit_price_gross: self
                .unit_price_gross
                .ok_or_else(|| Error::from(format!("第 {} 行缺少含税单价", self.line_no)))?,
        }))
    }

    /// 还原卡券字段组。
    ///
    /// # 返回
    /// 卡券行返回完整字段组；实物及服务行返回 `None`。
    ///
    /// # 错误
    /// 卡券行缺少任一必填字段时返回错误。
    pub fn voucher_fields(&self) -> Result<Option<VoucherLineDraft>> {
        if self.line_type != LineType::Voucher {
            return Ok(None);
        }
        Ok(Some(VoucherLineDraft {
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
        }))
    }

    /// 汇总工作副本行已经舍入的金额三元组。
    ///
    /// # 参数
    /// * `lines` - 同一工作副本的冻结明细行
    ///
    /// # 返回
    /// 返回 `(含税合计, 不含税合计, 税额合计)`。
    ///
    /// # 错误
    /// 无；金额值对象负责保持精度和范围。
    pub fn amount_totals(lines: &[Self]) -> (Amount, Amount, Amount) {
        sum_line_amounts(
            lines
                .iter()
                .map(|line| (line.gross_amount, line.net_amount, line.tax_amount)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::working_copy_test_support::{amt, line_data, price, rate};
    use super::*;

    #[test]
    fn line_new_computes_amounts_and_normalizes() {
        let line = SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("wcl-1"),
            SalesOrderWorkingCopyId::new("wc-1"),
            line_data(1),
        )
        .unwrap();

        assert_eq!(line.item_name_snapshot, "年货礼盒");
        assert_eq!(line.gross_amount, amt("29.97"));
        assert_eq!(line.net_amount, amt("26.07"));
        assert_eq!(line.tax_amount, amt("3.90"));
        assert_eq!(
            line.gross_amount.to_decimal(),
            line.net_amount.to_decimal() + line.tax_amount.to_decimal(),
            "gross = net + tax 逐行成立"
        );
        assert_eq!(line.base_unit_code.as_deref(), Some("箱"));
        assert_eq!(line.unit_price_gross, Some(price("9.9900")));
        assert!(line.face_value.is_none());
        assert!(line.card_count.is_none());
    }

    #[test]
    fn sellable_reference_mapping_is_owned_by_line_entity() {
        let goods = SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("wcl-1"),
            SalesOrderWorkingCopyId::new("wc-1"),
            line_data(1),
        )
        .unwrap();
        assert!(goods.belongs_to_working_copy(&SalesOrderWorkingCopyId::new("wc-1")));
        assert!(!goods.belongs_to_working_copy(&SalesOrderWorkingCopyId::new("wc-2")));
        let (sku, revision) = goods.sellable_sku_ref().unwrap().unwrap();
        assert_eq!(sku.as_ref(), "sku-1");
        assert_eq!(revision.as_ref(), "skurev-1");
        assert_eq!(goods.goods_fields().unwrap().unwrap().sku_id.as_ref(), "sku-1");
        assert!(goods.voucher_fields().unwrap().is_none());
        assert_eq!(
            SalesOrderWorkingCopyLine::amount_totals(std::slice::from_ref(&goods)),
            (amt("29.97"), amt("26.07"), amt("3.90"))
        );
        let empty_totals = SalesOrderWorkingCopyLine::amount_totals(&[]);
        assert_eq!(empty_totals, (amt("0.00"), amt("0.00"), amt("0.00")));
        assert_eq!(empty_totals.0.to_decimal().to_string(), "0.00");

        let mut voucher = goods;
        voucher.line_type = LineType::Voucher;
        assert!(voucher.sellable_sku_ref().unwrap().is_none());
    }

    #[test]
    fn line_new_rejects_zero_no_mismatch_and_inconsistent_voucher() {
        let zero = SalesOrderWorkingCopyLineData {
            line_no: 0,
            ..line_data(1)
        };
        assert!(SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("wcl-1"),
            SalesOrderWorkingCopyId::new("wc-1"),
            zero
        )
        .is_err());

        let mismatch = SalesOrderWorkingCopyLineData {
            line_type: LineType::Voucher,
            goods: None,
            voucher: None,
            ..line_data(1)
        };
        assert!(SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("wcl-1"),
            SalesOrderWorkingCopyId::new("wc-1"),
            mismatch
        )
        .is_err());

        let blank_item = SalesOrderWorkingCopyLineData {
            item_name_snapshot: "   ".to_string(),
            ..line_data(1)
        };
        assert!(SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("wcl-1"),
            SalesOrderWorkingCopyId::new("wc-1"),
            blank_item
        )
        .is_err());
    }

    #[test]
    fn typed_field_reconstruction_rejects_corrupted_persisted_shape() {
        let goods = SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("wcl-1"),
            SalesOrderWorkingCopyId::new("wc-1"),
            line_data(1),
        )
        .unwrap();

        let mut missing_goods = goods.clone();
        missing_goods.sku_id = None;
        assert_eq!(
            missing_goods.goods_fields().unwrap_err().to_string(),
            "第 1 行缺少商品字段组"
        );

        let mut missing_voucher = goods;
        missing_voucher.line_type = LineType::Voucher;
        assert_eq!(
            missing_voucher.voucher_fields().unwrap_err().to_string(),
            "第 1 行缺少卡券字段组"
        );
    }

    #[test]
    fn voucher_line_builds_with_derived_gift_rate() {
        let voucher = VoucherLineDraft {
            face_value: amt("100.00"),
            card_count: 3,
            unit_price_gross: price("90.0000"),
            face_value_total: amt("300.00"),
            transaction_amount: amt("270.00"),
            gift_amount: amt("30.00"),
            gift_rate: None,
            card_form: super::super::types::CardForm::Electronic,
        };
        let data = SalesOrderWorkingCopyLineData {
            sales_order_line_id: SalesOrderLineId::new("line-1"),
            line_no: 1,
            line_type: LineType::Voucher,
            sales_tax_rate: rate("0.130000"),
            item_name_snapshot: "福利卡".to_string(),
            spec_snapshot: None,
            unit_snapshot: Some("张".to_string()),
            goods: None,
            voucher: Some(voucher),
        };
        let line = SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("wcl-1"),
            SalesOrderWorkingCopyId::new("wc-1"),
            data,
        )
        .unwrap();

        assert!(line.goods_fields().unwrap().is_none());
        assert_eq!(line.voucher_fields().unwrap().unwrap().card_count, 3);
        assert_eq!(line.transaction_amount, Some(amt("270.00")));
        assert_eq!(line.face_value_total, Some(amt("300.00")));
        assert_eq!(line.gift_amount, Some(amt("30.00")));
        assert_eq!(line.gift_rate.unwrap().to_decimal().to_string(), "0.111111");
        assert_eq!(line.gross_amount, amt("270.00"), "公共行金额等于成交金额");
    }
}
