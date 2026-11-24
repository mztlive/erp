//! 销售单域公共值类型与行字段组校验（数据模型 §6.4 / erp-phase-1 §4.3）。
//!
//! 草稿行（工作副本行）与提交行按 `line_type` 携带「商品、数量、价格、履约字段组」
//! 或「卡券字段组」（§6.5 字段组概念），行金额一律由
//! [`erp_core::money::line_amounts`] 统一计算（§4.2 铁律 1，逐行舍入）。
//!
//! 本组类型在 D14/D15 有同形副本（`common/**` P0 冻结，P1 §3 跨域约束），
//! 待 `chore/erp-p0-amend-*` 地基修订统一收口到 `entities/src/common/`。

use std::collections::HashSet;

use erp_core::common::time::Instant;
use erp_core::ids::{SalesOrderLineId, SkuId, SkuRevisionId};
use erp_core::money::{Amount, Quantity, Rate, UnitPrice, line_amounts, round_to_cent};
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use rust_decimal::Decimal;
use serde::de::Error as SerdeError;
use serde::{Deserialize, Serialize};

use crate::entity::sales_order::{SalesDeliveryAllocation, SalesPricingMode, SalesProcurementPriceBasis};

/// 基础单位代码最大长度。
const BASE_UNIT_CODE_MAX_LEN: usize = 32;

/// 外部身份匹配的精确基数结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalIdentityResolution {
    /// 没有有效映射。
    Missing,
    /// 恰好命中一条有效映射。
    Resolved(String),
    /// 命中多条有效映射。
    Ambiguous,
}

impl ExternalIdentityResolution {
    /// 按有效外部身份命中数量收敛解析结果。
    /// 结果：零条返回 `Missing`，一条返回 `Resolved`，多条返回 `Ambiguous`。
    ///
    /// # Panics
    /// 长度已确认为 1 时 `pop` 不应为空。若仍为空，说明长度检查已被破坏，允许 panic。
    ///
    /// 输入约定：
    /// - `matches`：仓储返回的全部当前有效外部身份
    pub fn from_matches(mut matches: Vec<String>) -> Self {
        match matches.len() {
            0 => Self::Missing,
            1 => Self::Resolved(matches.pop().expect("匹配数量已经确认恰好为一条")),
            _ => Self::Ambiguous,
        }
    }
}

/// 业务性质（数据模型 §6.4：`VOUCHER` 或 `GOODS_SERVICE`，创建后永久不变）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BusinessType {
    /// 卡券销售单。
    Voucher,
    /// 实物及服务销售单。
    GoodsService,
}

impl BusinessType {
    /// 为界面或响应展示提供销售单业务性质的中文名称；机器筛选和存储不使用该名称。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Voucher => "卡券",
            Self::GoodsService => "实物及服务",
        }
    }

    /// 将销售单业务性质转成用于持久化和查询的稳定代码，与中文展示文案分开。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Voucher => "VOUCHER",
            Self::GoodsService => "GOODS_SERVICE",
        }
    }

    /// 判断是否为卡券销售。
    /// 结果：卡券销售返回 `true`，实物及服务销售返回 `false`。
    pub fn is_voucher(self) -> bool {
        matches!(self, Self::Voucher)
    }

    /// 判断是否为实物及服务销售。
    /// 结果：实物及服务销售返回 `true`，卡券销售返回 `false`。
    pub fn is_goods_service(self) -> bool {
        matches!(self, Self::GoodsService)
    }
}

/// 最初创建入口（数据模型 §6.4：商城或 ERP，创建后永久不变）。
/// 商城变体仅保留历史卡券同步单据标识，不代表商城二期对接；重做时另行设计。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OriginSystem {
    /// 商城（历史来源标识，无二期业务规则）。
    Mall,
    /// ERP。
    Erp,
}

impl OriginSystem {
    /// 为界面或响应展示提供销售建单入口的中文名称；机器筛选和存储不使用该名称。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Mall => "商城",
            Self::Erp => "ERP",
        }
    }

    /// 将销售建单入口转成用于持久化和查询的稳定代码，与中文展示文案分开。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mall => "MALL",
            Self::Erp => "ERP",
        }
    }
}

/// 行类型（数据模型 §6.4/§6.5：`GOODS_SERVICE` 或 `VOUCHER`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LineType {
    /// 实物及服务行。
    GoodsService,
    /// 卡券行。
    Voucher,
}

impl LineType {
    /// 为界面或响应展示提供销售明细类型的中文名称；机器筛选和存储不使用该名称。
    pub fn label(&self) -> &'static str {
        match self {
            Self::GoodsService => "实物及服务",
            Self::Voucher => "卡券",
        }
    }

    /// 将销售明细类型转成用于持久化和查询的稳定代码，与中文展示文案分开。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GoodsService => "GOODS_SERVICE",
            Self::Voucher => "VOUCHER",
        }
    }

    /// 判断行类型是否属于给定业务性质。
    /// 结果：行类型与业务性质一致时返回 `true`。
    ///
    /// 输入约定：
    /// - `business_type`：销售单业务性质
    pub fn belongs_to(self, business_type: BusinessType) -> bool {
        matches!(
            (self, business_type),
            (Self::GoodsService, BusinessType::GoodsService) | (Self::Voucher, BusinessType::Voucher)
        )
    }
}

/// 福利场景；停用场景保留反序列化兼容，不再作为新建选项。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WelfareScenario {
    /// 礼包，保留既有稳定代码。
    AnnualGiftBag,
    /// 帮扶产品。
    AssistanceProduct,
    /// 蛋糕券。
    CakeVoucher,
    /// 电影票。
    MovieTicket,
    /// 工会消费券。
    UnionConsumptionVoucher,
    /// 餐补。
    MealSubsidy,
    /// 活动。
    Activity,
    /// 历史场景：慰问品。
    CondolenceGift,
    /// 历史场景：消费金。
    ConsumptionFund,
    /// 历史场景：其他。
    Other,
}

impl WelfareScenario {
    /// 为界面或响应展示提供福利销售场景的中文名称；机器筛选和存储不使用该名称。
    pub fn label(&self) -> &'static str {
        match self {
            Self::AnnualGiftBag => "礼包",
            Self::AssistanceProduct => "帮扶产品",
            Self::CakeVoucher => "蛋糕券",
            Self::MovieTicket => "电影票",
            Self::UnionConsumptionVoucher => "工会消费券",
            Self::MealSubsidy => "餐补",
            Self::Activity => "活动",
            Self::CondolenceGift => "慰问品",
            Self::ConsumptionFund => "消费金",
            Self::Other => "其他",
        }
    }

    /// 将福利销售场景转成用于持久化和查询的稳定代码，与中文展示文案分开。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AnnualGiftBag => "ANNUAL_GIFT_BAG",
            Self::AssistanceProduct => "ASSISTANCE_PRODUCT",
            Self::CakeVoucher => "CAKE_VOUCHER",
            Self::MovieTicket => "MOVIE_TICKET",
            Self::UnionConsumptionVoucher => "UNION_CONSUMPTION_VOUCHER",
            Self::MealSubsidy => "MEAL_SUBSIDY",
            Self::Activity => "ACTIVITY",
            Self::CondolenceGift => "CONDOLENCE_GIFT",
            Self::ConsumptionFund => "CONSUMPTION_FUND",
            Self::Other => "OTHER",
        }
    }
}

/// 卡券行卡形态（数据模型 §6.4：电子卡或实体卡）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CardForm {
    /// 电子卡。
    Electronic,
    /// 实体卡。
    Physical,
}

impl CardForm {
    /// 为界面或响应展示提供卡券形态的中文名称；机器筛选和存储不使用该名称。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Electronic => "电子卡",
            Self::Physical => "实体卡",
        }
    }

    /// 将卡券形态转成用于持久化和查询的稳定代码，与中文展示文案分开。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Electronic => "ELECTRONIC",
            Self::Physical => "PHYSICAL",
        }
    }
}

/// 实物及服务行字段组（数据模型 §6.4/§6.5「商品、数量、价格、履约字段组」）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoodsLineFields {
    /// 正式销售项 SKU。
    pub sku_id: SkuId,
    /// 精确 SKU 修订；销售提交时重新校验销售资格。
    pub sku_revision_id: SkuRevisionId,
    /// 福利场景。
    pub welfare_scenario: Option<WelfareScenario>,
    /// 销售填写的采购供给价（含税单价）；提交时必填，用于含税毛利估算和审批参考。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub procurement_supply_price_gross: Option<UnitPrice>,
    /// 公司商品池取价方式；历史未选择及询价覆盖时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub procurement_price_basis: Option<SalesProcurementPriceBasis>,
    /// 公司对客户承诺完成本明细交付或服务的最晚时间。
    pub fulfillment_due_at: Instant,
    /// 基础单位数量。
    pub quantity: Quantity,
    /// 基础单位代码。
    pub base_unit_code: String,
    /// 含税成交单价快照。
    pub unit_price_gross: UnitPrice,
    /// 成交价编辑方式；历史缺省保留为手工价。
    #[serde(default, skip_serializing_if = "SalesPricingMode::is_manual")]
    pub pricing_mode: SalesPricingMode,
    /// 按地址分配的交付数量；历史未指定时为空。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delivery_allocations: Vec<SalesDeliveryAllocation>,
}

pub(crate) fn ensure_positive_sales_price(unit_price_gross: UnitPrice) -> Result<()> {
    if unit_price_gross.to_decimal() <= Decimal::ZERO {
        return Err(Error::from("含税成交单价必须大于 0"));
    }
    Ok(())
}

impl GoodsLineFields {
    pub(crate) fn ensure_active_sales_price(&self) -> Result<()> {
        if self.quantity.to_decimal().is_zero() {
            return Ok(());
        }
        ensure_positive_sales_price(self.unit_price_gross)
    }
}

// 读取卡券张数时兼容无符号整数和数字字符串；非法或超出 `u32` 范围的值由反序列化拒绝。
fn deserialize_u32_from_number_or_string<'de, D>(deserializer: D) -> std::result::Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum WireU32 {
        Number(u32),
        String(String),
    }

    match WireU32::deserialize(deserializer)? {
        WireU32::Number(value) => Ok(value),
        WireU32::String(value) => value.parse::<u32>().map_err(SerdeError::custom),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoucherLineDraft {
    /// 单卡面额。
    pub face_value: Amount,
    /// 卡张数（正整数）。
    #[serde(deserialize_with = "deserialize_u32_from_number_or_string")]
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

/// 卡券行字段组（实体保存形态，`gift_rate` 恒有确定值）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoucherLineFields {
    /// 单卡面额。
    pub face_value: Amount,
    /// 卡张数。
    pub card_count: u32,
    /// 单卡含税成交单价。
    pub unit_price_gross: UnitPrice,
    /// 面额小计。
    pub face_value_total: Amount,
    /// 最终成交金额。
    pub transaction_amount: Amount,
    /// 配赠金额。
    pub gift_amount: Amount,
    /// 配赠率。
    pub gift_rate: Rate,
    /// 卡形态。
    pub card_form: CardForm,
}

/// 草稿/提交行字段组的构建结果。
pub(crate) struct BuiltLineGroups {
    /// 规范化后的实物及服务字段组。
    pub goods: Option<GoodsLineFields>,
    /// 规范化后的卡券字段组。
    pub voucher: Option<VoucherLineFields>,
    /// 行含税金额（`gross = net + tax` 精确成立）。
    pub gross_amount: Amount,
    /// 行不含税金额。
    pub net_amount: Amount,
    /// 行税额。
    pub tax_amount: Amount,
}

impl BuiltLineGroups {
    // 校验提交行的采购供给价；卡券行和数量归零的移除行不适用。
    // 结果：有效商品行已填写采购供给价时返回成功。
    // 失败条件：有效商品行未填写时返回“请填写采购供给价”。
    pub(crate) fn ensure_supply_price(&self) -> Result<()> {
        if self.goods.as_ref().is_some_and(|goods| {
            !goods.quantity.to_decimal().is_zero() && goods.procurement_supply_price_gross.is_none()
        }) {
            return Err(Error::from("请填写采购供给价"));
        }
        Ok(())
    }

    // 读取商品行的交付分配；卡券行返回空列表。
    // 结果：独立的分配快照。
    pub(crate) fn delivery_allocations(&self) -> Vec<SalesDeliveryAllocation> {
        self.goods.as_ref().map(|goods| goods.delivery_allocations.clone()).unwrap_or_default()
    }
    // 读取已校验字段组的成交单价。
    // 结果：商品或卡券含税单价。
    pub(crate) fn unit_price_gross(&self) -> Option<UnitPrice> {
        self.goods
            .as_ref()
            .map(|goods| goods.unit_price_gross)
            .or_else(|| self.voucher.as_ref().map(|voucher| voucher.unit_price_gross))
    }
    // 读取商品行销售填写的采购供给价。
    // 结果：商品行未填写或卡券行返回 `None`。
    pub(crate) fn procurement_supply_price_gross(&self) -> Option<UnitPrice> {
        self.goods.as_ref().and_then(|goods| goods.procurement_supply_price_gross)
    }
}

// 构建行字段组并计算行金额三元组。
//
// 行类型与字段组必须一一对应：实物及服务行只允许 `goods`，卡券行只允许 `voucher`。
// 金额按数据模型 §4.2 铁律 1 逐行计算并舍入到分；卡券行按 §6.4 校验面额小计、
// 成交金额与配赠金额的一致性，成交金额为零时拒绝生效（配赠率无定义）。
// 返回含行金额三元组的构建结果。
// 失败条件：字段组与行类型不匹配、卡张数无法换成数量、卡券金额或配赠率不一致、成交金额为零，
// 或基础单位、服务区域为空或超长时返回错误。
//
// # Panics
// 实物及服务分支已确认 `goods` 为 `Some`。若此时仍为空，说明匹配条件被破坏，允许 panic。
//
// 输入约定：
// - `line_type`：行类型
// - `goods`：实物及服务字段组
// - `voucher`：卡券字段组
// - `sales_tax_rate`：销项税率
pub(crate) fn build_line_groups(
    line_type: LineType,
    goods: Option<GoodsLineFields>,
    voucher: Option<VoucherLineDraft>,
    sales_tax_rate: Rate,
) -> Result<BuiltLineGroups> {
    let (gross_amount, net_amount, tax_amount) = match (line_type, &goods, &voucher) {
        (LineType::GoodsService, Some(_), None) => {
            let goods = goods.as_ref().expect("已匹配 goods 字段组");
            line_amounts(goods.unit_price_gross, goods.quantity, sales_tax_rate)
        },
        (LineType::Voucher, None, Some(voucher)) => {
            let quantity = integer_quantity(voucher.card_count)?;
            line_amounts(voucher.unit_price_gross, quantity, sales_tax_rate)
        },
        _ => {
            return Err(Error::from(
                "行类型与字段组不一致：实物及服务行必须提供商品字段组，卡券行必须提供卡券字段组",
            ));
        },
    };

    let voucher = match voucher {
        Some(draft) => {
            let gift_rate = validate_voucher_draft(&draft)?;
            Some(VoucherLineFields {
                face_value: draft.face_value,
                card_count: draft.card_count,
                unit_price_gross: draft.unit_price_gross,
                face_value_total: draft.face_value_total,
                transaction_amount: draft.transaction_amount,
                gift_amount: draft.gift_amount,
                gift_rate,
                card_form: draft.card_form,
            })
        },
        None => None,
    };
    let goods = goods.map(normalize_goods).transpose()?;

    Ok(BuiltLineGroups { goods, voucher, gross_amount, net_amount, tax_amount })
}

fn normalize_goods(mut fields: GoodsLineFields) -> Result<GoodsLineFields> {
    // 历史零价行仍可重建工作副本；写入新成交明细时另行校验正价。
    if fields.unit_price_gross.to_decimal() < Decimal::ZERO {
        return Err(Error::from("含税成交单价不能为负数"));
    }
    fields.delivery_allocations =
        SalesDeliveryAllocation::validate_all(fields.delivery_allocations, fields.quantity)?;
    fields.base_unit_code = normalize_required_text(
        fields.base_unit_code,
        "基础单位不能为空",
        BASE_UNIT_CODE_MAX_LEN,
        "基础单位过长",
    )?;
    if fields.procurement_supply_price_gross.is_some_and(|price| price.to_decimal().is_sign_negative()) {
        return Err(Error::from("采购供给价不能为负数"));
    }
    Ok(fields)
}

// 校验草稿卡券字段组与推导金额的一致性（数据模型 §6.4）。
//
// 来源同时提供面额、单价与合计时逐项核对：面额小计、成交金额、配赠金额与
// 配赠率必须与公式推导值一致；不一致由本实体拒绝（P3 在同步入口转为差异任务）。
// 返回推导后的配赠率。
// 卡张数为零、成交金额为零、任一金额或配赠率与推导值不一致，或金额、配赠率无法落入值对象时返回错误。
//
// 输入约定：
// - `draft`：卡券字段组入参
fn validate_voucher_draft(draft: &VoucherLineDraft) -> Result<Rate> {
    let amounts = derive_voucher_amounts(draft.face_value, draft.card_count, draft.unit_price_gross)?;
    if draft.face_value_total != amounts.face_value_total {
        return Err(Error::from("面额小计必须等于面额乘卡张数"));
    }
    if draft.transaction_amount != amounts.transaction_amount {
        return Err(Error::from("成交金额必须等于单卡含税单价乘卡张数并按约定舍入"));
    }
    if draft.gift_amount != amounts.gift_amount {
        return Err(Error::from("配赠金额必须等于面额小计减成交金额"));
    }
    if let Some(gift_rate) = draft.gift_rate
        && gift_rate != amounts.gift_rate
    {
        return Err(Error::from("配赠率与配赠金额、成交金额不一致"));
    }
    Ok(amounts.gift_rate)
}

/// 卡券行的推导金额集合。
pub(crate) struct VoucherAmounts {
    /// 面额乘张数。
    pub face_value_total: Amount,
    /// 最终成交金额（按约定舍入的单价乘张数）。
    pub transaction_amount: Amount,
    /// 配赠金额（面额小计减成交金额）。
    pub gift_amount: Amount,
    /// 配赠率（以成交金额为分母，6 位小数）。
    pub gift_rate: Rate,
}

// 推导卡券行金额（数据模型 §6.4：正式版本由面额、张数与成交单价直接推导）。
// 返回推导金额集合。
// 卡张数为零、成交金额为零（配赠率无定义），或面额小计、成交金额、配赠金额、配赠率无法落入值对象时返回错误。
//
// 输入约定：
// - `face_value`：单卡面额
// - `card_count`：卡张数
// - `unit_price_gross`：单卡含税成交单价
pub(crate) fn derive_voucher_amounts(
    face_value: Amount,
    card_count: u32,
    unit_price_gross: UnitPrice,
) -> Result<VoucherAmounts> {
    if card_count == 0 {
        return Err(Error::from("卡券行卡张数必须为正整数"));
    }

    let face_value_total = face_value.to_decimal() * Decimal::from(card_count);
    let transaction_amount = round_to_cent(unit_price_gross.to_decimal() * Decimal::from(card_count));
    if transaction_amount.is_zero() {
        return Err(Error::from("成交金额为零时拒绝生效（配赠率无定义）"));
    }

    let gift_amount = face_value_total - transaction_amount;
    let gift_rate = Rate::try_from((gift_amount / transaction_amount).round_dp(6))?;
    Ok(VoucherAmounts {
        face_value_total: Amount::try_from(face_value_total)?,
        transaction_amount: Amount::try_from(transaction_amount)?,
        gift_amount: Amount::try_from(gift_amount)?,
        gift_rate,
    })
}

/// 行列表校验元组（用于草稿/提交头的行清单一致性校验）。
pub(crate) struct LineSummary {
    /// 单内稳定行号。
    pub line_no: u32,
    /// 稳定明细身份。
    pub line_id: SalesOrderLineId,
    /// 行类型。
    pub line_type: LineType,
}

// 校验行清单（数据模型 §6.4/§6.5 跨行断言）。
//
// 规则：行号与稳定明细身份不重复且行号为正；卡券销售单恰好一条卡券明细；
// 实物及服务销售单至少一行且只能有实物及服务行。
// 结果：全部断言通过时返回 `Ok(())`。
// 空清单、行号越界或重复、身份重复、行类型与业务性质不一致时返回错误。
//
// 输入约定：
// - `business_type`：销售单业务性质
// - `lines`：行清单摘要
pub(crate) fn validate_line_list(business_type: BusinessType, lines: &[LineSummary]) -> Result<()> {
    if lines.is_empty() {
        return Err(Error::from("销售单明细不能为空"));
    }
    let mut line_nos = HashSet::new();
    let mut line_ids = HashSet::new();
    for line in lines {
        if line.line_no == 0 {
            return Err(Error::from("行号必须为正整数"));
        }
        if !line_nos.insert(line.line_no) {
            return Err(Error::from("行号不能重复"));
        }
        if !line_ids.insert(&line.line_id) {
            return Err(Error::from("稳定明细身份不能重复"));
        }
    }

    match business_type {
        BusinessType::Voucher => {
            if lines.len() != 1 || lines[0].line_type != LineType::Voucher {
                return Err(Error::from("卡券销售单每个版本必须恰好包含一条卡券明细"));
            }
        },
        BusinessType::GoodsService => {
            if lines.iter().any(|line| line.line_type != LineType::GoodsService) {
                return Err(Error::from("实物及服务销售单只能包含实物及服务行"));
            }
        },
    }
    Ok(())
}

// 将卡张数转换为基础单位数量（整数，满足数量精度）。
//
// 行金额计算与审批快照总数量共用该换算，单位为张。
// 返回对应 `Quantity`。
// 数量超出精度（理论不可达，防御性分支）时返回错误。
// 业务边界：不得把卡张数写入或读取 `quantity` 字段；调用方必须显式换算。
//
// 输入约定：
// - `count`：卡张数
pub(crate) fn integer_quantity(count: u32) -> Result<Quantity> {
    Quantity::try_from(Decimal::from(count)).map_err(|error| {
        tracing::warn!(%error, "卡张数超出数量精度");
        Error::from("卡张数超出允许的范围或精度，请核对数量")
    })
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::SalesOrderLineId;

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
    fn enums_expose_labels_and_stable_codes() {
        assert_eq!(BusinessType::Voucher.label(), "卡券");
        assert_eq!(serde_json::to_string(&BusinessType::GoodsService).unwrap(), "\"GOODS_SERVICE\"");
        assert_eq!(OriginSystem::Mall.as_str(), "MALL");
        assert_eq!(LineType::Voucher.as_str(), "VOUCHER");
        assert_eq!(WelfareScenario::MealSubsidy.label(), "餐补");
        assert_eq!(CardForm::Physical.label(), "实体卡");
        assert_eq!(serde_json::to_string(&CardForm::Electronic).unwrap(), "\"ELECTRONIC\"");
    }

    #[test]
    fn welfare_scenarios_round_trip_current_codes_and_labels() {
        let scenarios = [
            (WelfareScenario::AnnualGiftBag, "ANNUAL_GIFT_BAG", "礼包"),
            (WelfareScenario::AssistanceProduct, "ASSISTANCE_PRODUCT", "帮扶产品"),
            (WelfareScenario::CakeVoucher, "CAKE_VOUCHER", "蛋糕券"),
            (WelfareScenario::MovieTicket, "MOVIE_TICKET", "电影票"),
            (WelfareScenario::UnionConsumptionVoucher, "UNION_CONSUMPTION_VOUCHER", "工会消费券"),
            (WelfareScenario::MealSubsidy, "MEAL_SUBSIDY", "餐补"),
            (WelfareScenario::Activity, "ACTIVITY", "活动"),
        ];
        for (scenario, code, label) in scenarios {
            assert_eq!(scenario.label(), label);
            assert_eq!(scenario.as_str(), code);
            assert_eq!(serde_json::to_value(scenario).unwrap(), code);
            assert_eq!(serde_json::from_value::<WelfareScenario>(serde_json::json!(code)).unwrap(), scenario);
        }
    }

    #[test]
    fn welfare_scenarios_preserve_legacy_values_and_reject_unknown_codes() {
        for (code, label) in
            [("CONDOLENCE_GIFT", "慰问品"), ("CONSUMPTION_FUND", "消费金"), ("OTHER", "其他")]
        {
            let scenario = serde_json::from_value::<WelfareScenario>(serde_json::json!(code)).unwrap();
            assert_eq!(scenario.label(), label);
            assert_eq!(scenario.as_str(), code);
            assert_eq!(serde_json::to_value(scenario).unwrap(), code);
        }

        for code in ["", "UNKNOWN"] {
            assert!(serde_json::from_value::<WelfareScenario>(serde_json::json!(code)).is_err());
        }
    }

    #[test]
    fn type_and_external_identity_rules_are_explicit() {
        assert!(BusinessType::Voucher.is_voucher());
        assert!(BusinessType::GoodsService.is_goods_service());
        assert!(LineType::Voucher.belongs_to(BusinessType::Voucher));
        assert!(!LineType::Voucher.belongs_to(BusinessType::GoodsService));

        assert_eq!(ExternalIdentityResolution::from_matches(Vec::new()), ExternalIdentityResolution::Missing);
        assert_eq!(
            ExternalIdentityResolution::from_matches(vec!["external-1".to_string()]),
            ExternalIdentityResolution::Resolved("external-1".to_string())
        );
        assert_eq!(
            ExternalIdentityResolution::from_matches(vec!["a".to_string(), "b".to_string()]),
            ExternalIdentityResolution::Ambiguous
        );
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
    fn voucher_draft_accepts_decimal_string_card_count_at_wire_boundary() {
        let mut payload = serde_json::to_value(voucher_draft()).unwrap();
        payload["card_count"] = serde_json::Value::String("3".to_string());

        let parsed: VoucherLineDraft = serde_json::from_value(payload).unwrap();
        assert_eq!(parsed.card_count, 3);
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
                pricing_mode: Default::default(),
                delivery_allocations: Vec::new(),
                sku_id: SkuId::new("sku-1"),
                sku_revision_id: SkuRevisionId::new("skurev-1"),
                welfare_scenario: None,
                procurement_supply_price_gross: None,
                procurement_price_basis: None,
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

    // 回归检查：采购供给价不改变销售行金额，采购供给价允许为零。
    #[test]
    fn procurement_supply_price_is_kept_without_affecting_amounts() {
        let goods = |mode: SalesPricingMode, unit: &str, supply: Option<&str>| GoodsLineFields {
            pricing_mode: mode,
            delivery_allocations: Vec::new(),
            sku_id: SkuId::new("sku-1"),
            sku_revision_id: SkuRevisionId::new("skurev-1"),
            welfare_scenario: None,
            procurement_supply_price_gross: supply.map(price),
            procurement_price_basis: None,
            fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
            quantity: qty("3.000000"),
            base_unit_code: "箱".to_string(),
            unit_price_gross: price(unit),
        };

        let built = build_line_groups(
            LineType::GoodsService,
            Some(goods(SalesPricingMode::Manual, "9.9900", Some("6.5000"))),
            None,
            rate("0.130000"),
        )
        .unwrap();
        assert_eq!(built.goods.unwrap().procurement_supply_price_gross, Some(price("6.5000")));
        assert_eq!(built.gross_amount, amt("29.97"));

        let historical_zero_price = build_line_groups(
            LineType::GoodsService,
            Some(goods(SalesPricingMode::Manual, "0", Some("0"))),
            None,
            rate("0.130000"),
        )
        .unwrap();
        assert_eq!(historical_zero_price.goods.unwrap().procurement_supply_price_gross, Some(price("0")));
        assert_eq!(historical_zero_price.gross_amount, amt("0"));

        let negative = goods(SalesPricingMode::Manual, "9.9900", Some("-0.0100"));
        let Err(error) = build_line_groups(LineType::GoodsService, Some(negative), None, rate("0.130000"))
        else {
            panic!("负数采购供给价必须拒绝");
        };
        assert_eq!(error.to_string(), "采购供给价不能为负数");
    }

    #[test]
    fn build_groups_rejects_mismatched_line_type_and_fields() {
        let goods = GoodsLineFields {
            pricing_mode: Default::default(),
            delivery_allocations: Vec::new(),
            sku_id: SkuId::new("sku-1"),
            sku_revision_id: SkuRevisionId::new("skurev-1"),
            welfare_scenario: None,
            procurement_supply_price_gross: None,
            procurement_price_basis: None,
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
