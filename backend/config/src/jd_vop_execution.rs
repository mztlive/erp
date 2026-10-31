//! 服务端受控账号执行资料；缺省保持只读，不能通过布尔开关绕过资料核验。
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

use serde::Deserialize;

/// 已核实的消息范围，独立于订单和开票资料。
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JdVopMessageConfig {
    pub evidence_reference: String,
    pub account_scope: String,
    pub app_key: String,
    pub types: BTreeSet<u32>,
}

/// 由部署管理员登记的账号执行依据；采购请求不得覆盖。
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JdVopExecutionConfig {
    pub evidence_reference: String,
    pub account_scope: String,
    pub app_key: String,
    pub external_funds_account_ref: String,
    pub confirmation_funds_evidence: Option<String>,
    pub preflight_validity_seconds: u64,
    pub ordinary_skus: BTreeSet<String>,
    pub invoice: JdVopInvoiceConfig,
    pub funds: Option<JdVopFundsConfig>,
}

/// 合同采购主体的发票资料。
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JdVopInvoiceConfig {
    pub invoice_type: u8,
    pub invoice_put_type: u8,
    pub invoice_selected_title: u8,
    pub invoice_content_type: u8,
    pub invoice_company_name: String,
    pub invoice_reg_code: Option<String>,
    pub invoice_reg_company_name: Option<String>,
    pub invoice_reg_address: Option<String>,
    pub invoice_reg_phone: Option<String>,
    pub invoice_reg_bank: Option<String>,
    pub invoice_reg_bank_account: Option<String>,
    pub recipient: Option<JdVopInvoiceRecipientConfig>,
}

/// 专票收件人及四级地址，不从服务器所在地推导。
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JdVopInvoiceRecipientConfig {
    pub invoice_name: String,
    pub invoice_phone: String,
    pub invoice_province_id: u64,
    pub invoice_city_id: u64,
    pub invoice_county_id: u64,
    pub invoice_town_id: u64,
    pub invoice_address: String,
}

/// 本账号核实的余额及交易解释规则。
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JdVopFundsConfig {
    pub evidence_reference: String,
    pub verified_account_reference: String,
    pub expected_pin: String,
    pub account_type: String,
    pub currency: JdVopFundsCurrencyConfig,
    pub timezone_offset_seconds: i32,
    pub time_format: JdVopFundsTimeConfig,
    pub maximum_page_size: NonZeroU32,
    pub trade_types: BTreeMap<String, JdVopTradeConfig>,
    #[serde(default)]
    pub message_effects: BTreeMap<String, JdVopFundsDirectionConfig>,
}

#[derive(Clone, Copy, Deserialize)]
pub enum JdVopFundsCurrencyConfig {
    CNY,
}
#[derive(Clone, Copy, Deserialize)]
pub enum JdVopFundsTimeConfig {
    LocalDateTimeSeconds,
}
#[derive(Clone, Copy, Deserialize)]
pub enum JdVopFundsDirectionConfig {
    Recharge,
    Debit,
    Return,
}
#[derive(Clone, Copy, Deserialize)]
pub enum JdVopFundsSignConfig {
    Positive,
    Negative,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JdVopTradeConfig {
    pub direction: JdVopFundsDirectionConfig,
    pub sign: JdVopFundsSignConfig,
}
