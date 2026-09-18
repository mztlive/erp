//! 销售审核域公共值类型与行字段组校验（数据模型 §6.4/§6.5）。
//!
//! 草稿行（工作副本行）与提交行按 `line_type` 携带「商品、数量、价格、履约字段组」
//! 或「卡券字段组」（§6.5 字段组概念），行金额一律由
//! [`erp_core::money::line_amounts`] 统一计算（§4.2 铁律 1，逐行舍入）。
//!
//! 业务性质、行类型、福利场景、卡形态、字段组与行清单规则与销售单域同形
//! （`common/**` P0 冻结，P1 §3 跨域约束），规范定义与纯规则由销售单域唯一承载，
//! 本模块只做复用。唯一的本地形态是 [`VoucherLineDraft`]：与销售单域同形，差异
//! 仅在线上传输层——卡张数只接受数字（销售单域同时接受十进制字符串，系外域历史
//! 兼容）；序列化输出形状一致。

use erp_core::Result;
use erp_core::money::{Amount, Rate, UnitPrice};
use serde::{Deserialize, Serialize};

/// 行金额与清单纯规则复用（签名只涉及已复用的同形类型与金额基元）。
pub(crate) use crate::entity::sales_order::types::{BuiltLineGroups, LineSummary, validate_line_list};
/// 业务性质、行类型、福利场景、卡形态与行字段组的规范定义复用。
pub use crate::entity::sales_order::types::{
    BusinessType, CardForm, GoodsLineFields, LineType, VoucherLineFields, WelfareScenario,
};

/// 卡券行字段组创建入参（数据模型 §6.4：来源同时提供面额、单价与合计，逐项核对；
/// `gift_rate` 缺省时按 `gift_amount / transaction_amount` 推导）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoucherLineDraft {
    /// 单卡面额。
    pub face_value: Amount,
    /// 卡张数（正整数）。
    pub card_count: u32,
    /// 单卡含税成交单价。
    pub unit_price_gross: UnitPrice,
    /// 面额小计（= 面额 × 张数）。
    pub face_value_total: Amount,
    /// 最终成交金额（= 含税单价 × 张数，按约定舍入）。
    pub transaction_amount: Amount,
    /// 配赠金额（= 面额小计 − 成交金额）。
    pub gift_amount: Amount,
    /// 配赠率（以成交金额为分母）；`None` 时由实体推导。
    pub gift_rate: Option<Rate>,
    /// 卡形态。
    pub card_form: CardForm,
}

impl From<VoucherLineDraft> for crate::entity::sales_order::types::VoucherLineDraft {
    /// 将审核域卡券字段组转为销售单域同形字段组。
    ///
    /// # 参数
    /// * `value` - 审核域卡券草稿字段组
    ///
    /// # 返回
    /// 返回销售单域卡券草稿字段组。
    fn from(value: VoucherLineDraft) -> Self {
        Self {
            face_value: value.face_value,
            card_count: value.card_count,
            unit_price_gross: value.unit_price_gross,
            face_value_total: value.face_value_total,
            transaction_amount: value.transaction_amount,
            gift_amount: value.gift_amount,
            gift_rate: value.gift_rate,
            card_form: value.card_form,
        }
    }
}

impl From<crate::entity::sales_order::types::VoucherLineDraft> for VoucherLineDraft {
    /// 将销售单域卡券字段组转为审核域同形字段组。
    ///
    /// # 参数
    /// * `value` - 销售单域卡券草稿字段组
    ///
    /// # 返回
    /// 返回审核域卡券草稿字段组。
    fn from(value: crate::entity::sales_order::types::VoucherLineDraft) -> Self {
        Self {
            face_value: value.face_value,
            card_count: value.card_count,
            unit_price_gross: value.unit_price_gross,
            face_value_total: value.face_value_total,
            transaction_amount: value.transaction_amount,
            gift_amount: value.gift_amount,
            gift_rate: value.gift_rate,
            card_form: value.card_form,
        }
    }
}

/// 构建行字段组并计算行金额三元组。
///
/// 行类型与字段组必须一一对应：实物及服务行只允许 `goods`，卡券行只允许 `voucher`。
/// 金额按数据模型 §4.2 铁律 1 逐行计算并舍入到分；卡券行按 §6.4 校验面额小计、
/// 成交金额与配赠金额的一致性，成交金额为零时拒绝生效（配赠率无定义）。
/// 规则实现复用销售单域 [`crate::entity::sales_order::types::build_line_groups`]。
///
/// # 参数
/// * `line_type` - 行类型
/// * `goods` - 实物及服务字段组
/// * `voucher` - 卡券字段组
/// * `sales_tax_rate` - 销项税率
///
/// # 返回
/// 返回含行金额三元组的构建结果。
///
/// # 错误
/// 字段组与行类型不匹配、卡券金额不一致或成交金额为零时返回错误。
pub(crate) fn build_line_groups(
    line_type: LineType,
    goods: Option<GoodsLineFields>,
    voucher: Option<VoucherLineDraft>,
    sales_tax_rate: Rate,
) -> Result<BuiltLineGroups> {
    crate::entity::sales_order::types::build_line_groups(
        line_type,
        goods,
        voucher.map(crate::entity::sales_order::types::VoucherLineDraft::from),
        sales_tax_rate,
    )
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::Instant;
    use erp_core::ids::{SalesOrderLineId, SkuId, SkuRevisionId};
    use erp_core::money::Quantity;

    use super::*;

    fn amt(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn price(value: &str) -> UnitPrice {
        UnitPrice::from_str(value).unwrap()
    }

    fn qty(value: &str) -> Quantity {
        Quantity::from_str(value).unwrap()
    }

    fn rate(value: &str) -> Rate {
        Rate::from_str(value).unwrap()
    }

    #[test]
    fn review_types_reuse_canonical_enums_and_rules() {
        assert_eq!(BusinessType::Voucher.label(), "卡券");
        assert_eq!(BusinessType::Voucher.as_str(), "VOUCHER");
        assert!(BusinessType::Voucher.is_voucher());
        assert!(LineType::Voucher.belongs_to(BusinessType::Voucher));
        assert!(!LineType::Voucher.belongs_to(BusinessType::GoodsService));
        assert_eq!(WelfareScenario::MealSubsidy.label(), "餐补");
        assert_eq!(CardForm::Physical.label(), "实体卡");
        assert_eq!(serde_json::to_string(&BusinessType::GoodsService).unwrap(), "\"GOODS_SERVICE\"");
        assert_eq!(serde_json::to_string(&CardForm::Electronic).unwrap(), "\"ELECTRONIC\"");
    }

    #[test]
    fn validate_line_list_accepts_goods_and_voucher_shapes() {
        let goods_lines = [
            LineSummary {
                line_no: 1,
                line_id: SalesOrderLineId::new("l-1"),
                line_type: LineType::GoodsService,
            },
            LineSummary {
                line_no: 2,
                line_id: SalesOrderLineId::new("l-2"),
                line_type: LineType::GoodsService,
            },
        ];
        assert!(validate_line_list(BusinessType::GoodsService, &goods_lines).is_ok());

        let voucher_lines =
            [LineSummary { line_no: 1, line_id: SalesOrderLineId::new("l-3"), line_type: LineType::Voucher }];
        assert!(validate_line_list(BusinessType::Voucher, &voucher_lines).is_ok());
    }

    #[test]
    fn validate_line_list_rejects_empty_duplicate_and_out_of_range() {
        assert!(validate_line_list(BusinessType::GoodsService, &[]).is_err(), "空清单");

        let duplicated_no = [
            LineSummary {
                line_no: 1,
                line_id: SalesOrderLineId::new("l-1"),
                line_type: LineType::GoodsService,
            },
            LineSummary {
                line_no: 1,
                line_id: SalesOrderLineId::new("l-2"),
                line_type: LineType::GoodsService,
            },
        ];
        assert!(validate_line_list(BusinessType::GoodsService, &duplicated_no).is_err());

        let duplicated_id = [
            LineSummary {
                line_no: 1,
                line_id: SalesOrderLineId::new("l-1"),
                line_type: LineType::GoodsService,
            },
            LineSummary {
                line_no: 2,
                line_id: SalesOrderLineId::new("l-1"),
                line_type: LineType::GoodsService,
            },
        ];
        assert!(validate_line_list(BusinessType::GoodsService, &duplicated_id).is_err());

        let zero_line_no = [LineSummary {
            line_no: 0,
            line_id: SalesOrderLineId::new("l-1"),
            line_type: LineType::GoodsService,
        }];
        assert!(validate_line_list(BusinessType::GoodsService, &zero_line_no).is_err());
    }

    #[test]
    fn validate_line_list_rejects_voucher_mis_shaped_lines() {
        let two_voucher_lines = [
            LineSummary { line_no: 1, line_id: SalesOrderLineId::new("l-1"), line_type: LineType::Voucher },
            LineSummary { line_no: 2, line_id: SalesOrderLineId::new("l-2"), line_type: LineType::Voucher },
        ];
        assert!(validate_line_list(BusinessType::Voucher, &two_voucher_lines).is_err());

        let goods_line_in_voucher_order = [LineSummary {
            line_no: 1,
            line_id: SalesOrderLineId::new("l-1"),
            line_type: LineType::GoodsService,
        }];
        assert!(validate_line_list(BusinessType::Voucher, &goods_line_in_voucher_order).is_err());

        let voucher_line_in_goods_order =
            [LineSummary { line_no: 1, line_id: SalesOrderLineId::new("l-1"), line_type: LineType::Voucher }];
        assert!(validate_line_list(BusinessType::GoodsService, &voucher_line_in_goods_order).is_err());
    }

    fn voucher_draft() -> VoucherLineDraft {
        VoucherLineDraft {
            face_value: amt("100.00"),
            card_count: 3,
            unit_price_gross: price("90.0000"),
            face_value_total: amt("300.00"),
            transaction_amount: amt("270.00"),
            gift_amount: amt("30.00"),
            gift_rate: None,
            card_form: CardForm::Electronic,
        }
    }

    #[test]
    fn build_voucher_groups_derives_gift_rate_and_amounts() {
        let built =
            build_line_groups(LineType::Voucher, None, Some(voucher_draft()), rate("0.130000")).unwrap();

        // 270.00 × 13% → 税 35.10，净额 234.90，gross = net + tax。
        let voucher = built.voucher.unwrap();
        assert_eq!(voucher.transaction_amount, amt("270.00"));
        assert_eq!(voucher.gift_rate.to_decimal().to_string(), "0.111111");
        assert_eq!(
            built.gross_amount.to_decimal(),
            built.net_amount.to_decimal() + built.tax_amount.to_decimal()
        );
    }

    #[test]
    fn build_voucher_groups_rejects_inconsistent_amounts() {
        let bad_total = VoucherLineDraft { face_value_total: amt("301.00"), ..voucher_draft() };
        assert!(build_line_groups(LineType::Voucher, None, Some(bad_total), rate("0.130000")).is_err());

        let bad_transaction = VoucherLineDraft { transaction_amount: amt("269.00"), ..voucher_draft() };
        assert!(build_line_groups(LineType::Voucher, None, Some(bad_transaction), rate("0.130000")).is_err());

        let bad_gift = VoucherLineDraft { gift_amount: amt("29.00"), ..voucher_draft() };
        assert!(build_line_groups(LineType::Voucher, None, Some(bad_gift), rate("0.130000")).is_err());

        let wrong_rate = VoucherLineDraft { gift_rate: Some(rate("0.120000")), ..voucher_draft() };
        assert!(build_line_groups(LineType::Voucher, None, Some(wrong_rate), rate("0.130000")).is_err());

        let zero_count = VoucherLineDraft { card_count: 0, ..voucher_draft() };
        assert!(build_line_groups(LineType::Voucher, None, Some(zero_count), rate("0.130000")).is_err());
    }

    #[test]
    fn build_goods_groups_computes_amounts_and_normalizes_unit() {
        let built = build_line_groups(
            LineType::GoodsService,
            Some(GoodsLineFields {
                sku_id: SkuId::new("sku-1"),
                sku_revision_id: SkuRevisionId::new("skurev-1"),
                welfare_scenario: None,
                service_region: None,
                fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
                quantity: qty("3.000000"),
                base_unit_code: " 箱 ".to_string(),
                unit_price_gross: price("9.9900"),
            }),
            None,
            rate("0.130000"),
        )
        .unwrap();

        assert_eq!(built.goods.unwrap().base_unit_code, "箱");
        // 9.99 × 3 @ 13% = (29.97, 26.07, 3.90)（money.rs 确定性用例）。
        assert_eq!(built.gross_amount, amt("29.97"));
        assert_eq!(built.net_amount, amt("26.07"));
        assert_eq!(built.tax_amount, amt("3.90"));
    }

    #[test]
    fn build_groups_rejects_mismatched_line_type_and_fields() {
        let goods = GoodsLineFields {
            sku_id: SkuId::new("sku-1"),
            sku_revision_id: SkuRevisionId::new("skurev-1"),
            welfare_scenario: None,
            service_region: None,
            fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
            quantity: qty("1.000000"),
            base_unit_code: "箱".to_string(),
            unit_price_gross: price("9.9900"),
        };
        assert!(build_line_groups(LineType::GoodsService, None, None, rate("0.130000")).is_err());
        assert!(build_line_groups(LineType::Voucher, Some(goods), None, rate("0.130000")).is_err());
        assert!(
            build_line_groups(LineType::GoodsService, None, Some(voucher_draft()), rate("0.130000")).is_err()
        );
    }
}
