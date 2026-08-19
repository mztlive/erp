use entities::sales_order::{GoodsLineFields, LineType, VoucherLineDraft};
use entities::sales_review::SalesChangeSubmissionLine;

use crate::errors::{Error, Result};

/// 从变更工作副本行还原实物及服务字段组（D13 行字段组转换为 D14 同形类型）。
///
/// # 参数
/// * `line` - 工作副本行
///
/// # 返回
/// 返回字段组；卡券行返回 `None`。
///
/// # 错误
/// 实物及服务行缺商品字段组时返回错误。
pub(super) fn change_copy_goods(
    line: &entities::sales_order::SalesOrderWorkingCopyLine,
) -> Result<Option<entities::sales_review::GoodsLineFields>> {
    if line.line_type != LineType::GoodsService {
        return Ok(None);
    }
    let sku_id = line
        .sku_id
        .clone()
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少商品字段组", line.line_no)))?;
    let sku_revision_id = line
        .sku_revision_id
        .clone()
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少 SKU 修订", line.line_no)))?;
    let fulfillment_mode = line
        .fulfillment_mode
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少履约方式", line.line_no)))?;
    let fulfillment_due_at = line
        .fulfillment_due_at
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少履约期限", line.line_no)))?;
    let quantity = line
        .quantity
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少数量", line.line_no)))?;
    let base_unit_code = line
        .base_unit_code
        .clone()
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少单位", line.line_no)))?;
    let unit_price_gross = line
        .unit_price_gross
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少含税单价", line.line_no)))?;
    Ok(Some(entities::sales_review::GoodsLineFields {
        sku_id,
        sku_revision_id,
        welfare_scenario: line.welfare_scenario.map(convert_welfare_scenario),
        fulfillment_mode: convert_fulfillment_mode(fulfillment_mode),
        fulfillment_due_at,
        quantity,
        base_unit_code,
        unit_price_gross,
    }))
}

/// D13 福利场景 → D14 同形类型转换（两域同形副本，待地基修订下沉）。
fn convert_welfare_scenario(
    value: entities::sales_order::WelfareScenario,
) -> entities::sales_review::WelfareScenario {
    match value {
        entities::sales_order::WelfareScenario::AnnualGiftBag => {
            entities::sales_review::WelfareScenario::AnnualGiftBag
        }
        entities::sales_order::WelfareScenario::MealSubsidy => {
            entities::sales_review::WelfareScenario::MealSubsidy
        }
        entities::sales_order::WelfareScenario::CondolenceGift => {
            entities::sales_review::WelfareScenario::CondolenceGift
        }
        entities::sales_order::WelfareScenario::ConsumptionFund => {
            entities::sales_review::WelfareScenario::ConsumptionFund
        }
        entities::sales_order::WelfareScenario::Other => entities::sales_review::WelfareScenario::Other,
    }
}

/// D13 履约方式 → D14 同形类型转换。
fn convert_fulfillment_mode(
    value: entities::sales_order::FulfillmentMode,
) -> entities::sales_review::FulfillmentMode {
    match value {
        entities::sales_order::FulfillmentMode::CompanyWarehouse => {
            entities::sales_review::FulfillmentMode::CompanyWarehouse
        }
        entities::sales_order::FulfillmentMode::SupplierDirect => {
            entities::sales_review::FulfillmentMode::SupplierDirect
        }
        entities::sales_order::FulfillmentMode::ElectronicDelivery => {
            entities::sales_review::FulfillmentMode::ElectronicDelivery
        }
        entities::sales_order::FulfillmentMode::OfflineService => {
            entities::sales_review::FulfillmentMode::OfflineService
        }
    }
}

/// D13 卡形态 → D14 同形类型转换。
fn convert_card_form(value: entities::sales_order::CardForm) -> entities::sales_review::CardForm {
    match value {
        entities::sales_order::CardForm::Electronic => entities::sales_review::CardForm::Electronic,
        entities::sales_order::CardForm::Physical => entities::sales_review::CardForm::Physical,
    }
}

/// D13 行类型 → D14 同形类型转换。
pub(super) fn convert_line_type(value: entities::sales_order::LineType) -> entities::sales_review::LineType {
    match value {
        entities::sales_order::LineType::GoodsService => entities::sales_review::LineType::GoodsService,
        entities::sales_order::LineType::Voucher => entities::sales_review::LineType::Voucher,
    }
}

/// 从变更工作副本行还原卡券字段组（D13 行字段组转换为 D14 同形类型）。
///
/// # 参数
/// * `line` - 工作副本行
///
/// # 返回
/// 返回字段组；实物及服务行返回 `None`。
///
/// # 错误
/// 卡券行缺卡券字段组时返回错误。
pub(super) fn change_copy_voucher(
    line: &entities::sales_order::SalesOrderWorkingCopyLine,
) -> Result<Option<entities::sales_review::VoucherLineDraft>> {
    use entities::sales_review::VoucherLineDraft as ChangeVoucherLineDraft;
    if line.line_type != LineType::Voucher {
        return Ok(None);
    }
    let face_value = line
        .face_value
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡券字段组", line.line_no)))?;
    let card_count = line
        .card_count
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡张数", line.line_no)))?;
    let unit_price_gross = line
        .unit_price_gross
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡券成交单价", line.line_no)))?;
    Ok(Some(ChangeVoucherLineDraft {
        face_value,
        card_count,
        unit_price_gross,
        face_value_total: line
            .face_value_total
            .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少面额小计", line.line_no)))?,
        transaction_amount: line
            .transaction_amount
            .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少成交金额", line.line_no)))?,
        gift_amount: line
            .gift_amount
            .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少配赠金额", line.line_no)))?,
        gift_rate: line.gift_rate,
        card_form: convert_card_form(
            line.card_form
                .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡形态", line.line_no)))?,
        ),
    }))
}

/// D14 卡形态 → D13 同形类型转换。
fn convert_card_form_to_sales(value: entities::sales_review::CardForm) -> entities::sales_order::CardForm {
    match value {
        entities::sales_review::CardForm::Electronic => entities::sales_order::CardForm::Electronic,
        entities::sales_review::CardForm::Physical => entities::sales_order::CardForm::Physical,
    }
}

/// D14 行类型 → D13 同形类型转换（版本行使用 D13 类型）。
pub(super) fn convert_line_type_to_sales(
    value: entities::sales_review::LineType,
) -> entities::sales_order::LineType {
    match value {
        entities::sales_review::LineType::GoodsService => entities::sales_order::LineType::GoodsService,
        entities::sales_review::LineType::Voucher => entities::sales_order::LineType::Voucher,
    }
}

/// D14 履约方式 → D13 同形类型转换。
fn convert_fulfillment_mode_to_sales(
    value: entities::sales_review::FulfillmentMode,
) -> entities::sales_order::FulfillmentMode {
    match value {
        entities::sales_review::FulfillmentMode::CompanyWarehouse => {
            entities::sales_order::FulfillmentMode::CompanyWarehouse
        }
        entities::sales_review::FulfillmentMode::SupplierDirect => {
            entities::sales_order::FulfillmentMode::SupplierDirect
        }
        entities::sales_review::FulfillmentMode::ElectronicDelivery => {
            entities::sales_order::FulfillmentMode::ElectronicDelivery
        }
        entities::sales_review::FulfillmentMode::OfflineService => {
            entities::sales_order::FulfillmentMode::OfflineService
        }
    }
}

/// D14 福利场景 → D13 同形类型转换。
fn convert_welfare_scenario_to_sales(
    value: entities::sales_review::WelfareScenario,
) -> entities::sales_order::WelfareScenario {
    match value {
        entities::sales_review::WelfareScenario::AnnualGiftBag => {
            entities::sales_order::WelfareScenario::AnnualGiftBag
        }
        entities::sales_review::WelfareScenario::MealSubsidy => {
            entities::sales_order::WelfareScenario::MealSubsidy
        }
        entities::sales_review::WelfareScenario::CondolenceGift => {
            entities::sales_order::WelfareScenario::CondolenceGift
        }
        entities::sales_review::WelfareScenario::ConsumptionFund => {
            entities::sales_order::WelfareScenario::ConsumptionFund
        }
        entities::sales_review::WelfareScenario::Other => entities::sales_order::WelfareScenario::Other,
    }
}

/// 从变更提交行还原实物及服务字段组。
///
/// # 参数
/// * `line` - 变更提交行
///
/// # 返回
/// 返回字段组。
///
/// # 错误
/// 实物及服务行缺商品字段组时返回错误。
pub(super) fn change_submission_goods(line: &SalesChangeSubmissionLine) -> Result<GoodsLineFields> {
    let sku_id = line
        .sku_id
        .clone()
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少商品字段组", line.line_no)))?;
    let sku_revision_id = line
        .sku_revision_id
        .clone()
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少 SKU 修订", line.line_no)))?;
    let fulfillment_mode = line
        .fulfillment_mode
        .map(convert_fulfillment_mode_to_sales)
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少履约方式", line.line_no)))?;
    let fulfillment_due_at = line
        .fulfillment_due_at
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少履约期限", line.line_no)))?;
    let quantity = line
        .quantity
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少数量", line.line_no)))?;
    let base_unit_code = line
        .base_unit_code
        .clone()
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少单位", line.line_no)))?;
    let unit_price_gross = line
        .unit_price_gross
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少含税单价", line.line_no)))?;
    Ok(GoodsLineFields {
        sku_id,
        sku_revision_id,
        welfare_scenario: line.welfare_scenario.map(convert_welfare_scenario_to_sales),
        fulfillment_mode,
        fulfillment_due_at,
        quantity,
        base_unit_code,
        unit_price_gross,
    })
}

/// 从变更提交行还原卡券字段组。
///
/// # 参数
/// * `line` - 变更提交行
///
/// # 返回
/// 返回字段组。
///
/// # 错误
/// 卡券行缺卡券字段组时返回错误。
pub(super) fn change_submission_voucher(line: &SalesChangeSubmissionLine) -> Result<VoucherLineDraft> {
    let face_value = line
        .face_value
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡券字段组", line.line_no)))?;
    let card_count = line
        .card_count
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡张数", line.line_no)))?;
    let unit_price_gross = line
        .unit_price_gross
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡券成交单价", line.line_no)))?;
    let face_value_total = line
        .face_value_total
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少面额小计", line.line_no)))?;
    let transaction_amount = line
        .transaction_amount
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少成交金额", line.line_no)))?;
    let gift_amount = line
        .gift_amount
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少配赠金额", line.line_no)))?;
    Ok(VoucherLineDraft {
        face_value,
        card_count,
        unit_price_gross,
        face_value_total,
        transaction_amount,
        gift_amount,
        gift_rate: line.gift_rate,
        card_form: convert_card_form_to_sales(
            line.card_form
                .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡形态", line.line_no)))?,
        ),
    })
}
