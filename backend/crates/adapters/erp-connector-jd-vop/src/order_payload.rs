//! 创建、确认及取消的类型化载荷；不补造发票参数或省略已批准单价。
use std::collections::BTreeSet;

use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::common::ConnectorResult;
use erp_supply::ports::connector::order::{CreateOrder, DeliveryMethod, OrderLine};
use rust_decimal::Decimal;
use serde::{Serialize, Serializer};
use serde_json::value::RawValue;
use serde_json::{Value, to_value};

use super::funds::JdVopFundsProfile;
use super::{error, mapping};

/// G0 核实的确认资金规则；仅允许服务端按账号证据构造，不从业务配置反序列化。
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JdVopConfirmationFundsPolicy {
    /// 确认继续使用原预占或原已扣款保障，不要求新增或重复资金扣减。
    OriginalOrderReservation { evidence_reference: String },
}

/// 受信服务端核实的账号执行资料，不从采购前端任意 JSON 接收。
#[derive(Clone, Serialize)]
pub struct JdVopExecutionProfile {
    pub source_type: u32,
    pub evidence_reference: String,
    pub invoice: JdVopInvoiceProfile,
    pub verified_external_funds_account_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmation_funds_policy: Option<JdVopConfirmationFundsPolicy>,
    pub preflight_validity_seconds: u64,
    pub ordinary_skus: BTreeSet<String>,
    pub verified_message_types: Option<BTreeSet<u32>>,
    pub verified_funds_profile: Option<JdVopFundsProfile>,
}

/// 发票参数由账号合同及财务资料核实；尚未取得的资料保持缺失并阻断。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JdVopInvoiceProfile {
    pub invoice_type: u8,
    pub invoice_put_type: u8,
    pub invoice_selected_title: u8,
    pub invoice_content_type: u8,
    pub invoice_company_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice_reg_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice_reg_company_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice_reg_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice_reg_phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice_reg_bank: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice_reg_bank_account: Option<String>,
    #[serde(flatten)]
    pub recipient: Option<JdVopInvoiceRecipient>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JdVopInvoiceRecipient {
    pub invoice_name: String,
    pub invoice_phone: String,
    pub invoice_province_id: u64,
    pub invoice_city_id: u64,
    pub invoice_county_id: u64,
    pub invoice_town_id: u64,
    pub invoice_address: String,
}

impl JdVopExecutionProfile {
    /// 校验已验收的服务端账号资料；本方法不代替 G0 外部证据核验。
    /// # 参数
    /// 无。
    /// # 返回
    /// 账号调用渠道、证据引用及发票资料完整时成功。
    /// # 错误
    /// 未核实参数、缺失财务资料或不支持的发票类型时阻断。
    pub fn validate(&self) -> ConnectorResult<()> {
        if self.confirmation_funds_policy.is_some() {
            self.ensure_confirmation_funds()?;
        }
        let invoice = &self.invoice;
        if self.source_type == 0
            || self.evidence_reference.trim().is_empty()
            || self.verified_external_funds_account_ref.trim().is_empty()
            || !(1..=300).contains(&self.preflight_validity_seconds)
            || self.ordinary_skus.is_empty()
            || self.evidence_reference.len() > 1024
            || !matches!(invoice.invoice_type, 23 | 24)
            || !matches!(invoice.invoice_put_type, 2 | 4)
            || !matches!(invoice.invoice_selected_title, 4 | 5)
            || !matches!(invoice.invoice_content_type, 1 | 100)
            || !text(&invoice.invoice_company_name, 200)
            || (invoice.invoice_selected_title == 5 && !present(&invoice.invoice_reg_code))
        {
            return Err(mapping("京东账号执行依据或开票资料尚未完整核实"));
        }
        if invoice.invoice_type == 24 {
            self.validate_special_invoice()?;
        }
        Ok(())
    }

    /// 核对原订单确认只使用已核实的原资金保障。
    /// # 参数
    /// 无；读取服务端冻结的账号证据。
    /// # 返回
    /// 已核实原预占或原扣款足以支持确认时成功。
    /// # 错误
    /// 未取得规则或证据引用无效时返回能力缺失，禁止发送写请求。
    pub(super) fn ensure_confirmation_funds(&self) -> ConnectorResult<()> {
        match &self.confirmation_funds_policy {
            Some(JdVopConfirmationFundsPolicy::OriginalOrderReservation { evidence_reference })
                if text(evidence_reference, 1024) && evidence_reference.trim() == evidence_reference =>
            {
                Ok(())
            },
            _ => Err(error(
                SupplierFailureClass::CapabilityGap,
                "JD_VOP_CONFIRMATION_FUNDS_POLICY_REQUIRED",
                "京东原订单确认的资金规则尚未核实，请先补齐账号接入依据",
            )),
        }
    }

    /// 取得技术账号绑定内容，确认资金准入证据的更新不改变原订单查询身份。
    /// # 参数
    /// 无；读取当前服务端执行资料。
    /// # 返回
    /// 与新增确认准入字段之前相同的账号绑定内容。
    /// # 错误
    /// 执行资料无法序列化时返回脱敏映射错误。
    pub(super) fn binding_content(&self) -> ConnectorResult<Value> {
        let mut content = to_value(self).map_err(|_| mapping("京东账号配置绑定编码失败"))?;
        if let Some(fields) = content.as_object_mut() {
            fields.remove("confirmation_funds_policy");
        }
        Ok(content)
    }

    fn validate_special_invoice(&self) -> ConnectorResult<()> {
        let invoice = &self.invoice;
        if invoice.invoice_put_type != 2
            || invoice.invoice_content_type != 1
            || invoice.invoice_selected_title != 5
            || [
                &invoice.invoice_reg_code,
                &invoice.invoice_reg_company_name,
                &invoice.invoice_reg_address,
                &invoice.invoice_reg_phone,
                &invoice.invoice_reg_bank,
                &invoice.invoice_reg_bank_account,
            ]
            .iter()
            .any(|value| !present(value))
        {
            return Err(mapping("京东增值税专用发票资料或集中开票方式不完整"));
        }
        let recipient = invoice.recipient.as_ref().ok_or_else(|| mapping("京东专票收票资料未核实"))?;
        if !text(&recipient.invoice_name, 100)
            || !text(&recipient.invoice_phone, 32)
            || !text(&recipient.invoice_address, 100)
            || [
                recipient.invoice_province_id,
                recipient.invoice_city_id,
                recipient.invoice_county_id,
                recipient.invoice_town_id,
            ]
            .contains(&0)
        {
            return Err(mapping("京东专票收票人及四级地址未核实"));
        }
        Ok(())
    }
}

fn present(value: &Option<String>) -> bool {
    value.as_deref().is_some_and(|value| text(value, 256))
}
fn text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= maximum && !value.chars().any(char::is_control)
}

/// 直接序列化十进制词法值，金额不经浮点数且不改为 JSON 字符串。
#[derive(Clone)]
pub struct ExactNumber(pub String);

impl Serialize for ExactNumber {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let value = RawValue::from_string(self.0.clone()).map_err(serde::ser::Error::custom)?;
        value.serialize(serializer)
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitLine {
    pub sku_id: u64,
    pub sku_num: u32,
    pub sku_unit_price: ExactNumber,
    pub sku_need_gift: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Consignee {
    pub consignee_name: String,
    pub consignee_mobile: String,
    pub consignee_address: String,
    pub consignee_province_id: u64,
    pub consignee_city_id: u64,
    pub consignee_county_id: u64,
    pub consignee_town_id: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitOrder {
    pub third_order_id: String,
    pub submit_state_type: u8,
    pub sku_info_list: Vec<SubmitLine>,
    pub payment_info: PaymentInfo,
    pub consignee_info: Consignee,
    pub invoice_info: JdVopInvoiceProfile,
    pub naked_price_order: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentInfo {
    pub payment_type: u8,
}

pub(super) fn submit(request: &CreateOrder, profile: &JdVopExecutionProfile) -> ConnectorResult<SubmitOrder> {
    profile.validate()?;
    valid_order_no(&request.merchant_order_no)?;
    if !matches!(request.delivery.method, DeliveryMethod::Courier)
        || request.lines.is_empty()
        || request.lines.len() > 100
        || !text(&request.recipient.name, 100)
        || !text(&request.recipient.phone, 32)
        || !text(&request.recipient.address, 100)
    {
        return Err(mapping("京东普通实物履约资料不完整或包含未支持的服务"));
    }
    let address =
        request.recipient.supplier_address.as_ref().ok_or_else(|| mapping("请先确认京东四级收货地址"))?;

    let mut seen = BTreeSet::new();
    let mut lines = Vec::new();
    for line in &request.lines {
        if !profile.ordinary_skus.contains(&line.sku.spec_id) {
            return Err(mapping("京东商品普通实物履约资格尚未核实"));
        }
        if !seen.insert(line.sku.spec_id.clone()) {
            return Err(mapping("同一京东订货编号的多个采购行须先核实数量分配"));
        }
        lines.push(submit_line(line)?);
    }

    Ok(SubmitOrder {
        third_order_id: request.merchant_order_no.clone(),
        submit_state_type: 0,
        sku_info_list: lines,
        payment_info: PaymentInfo { payment_type: 4 },
        naked_price_order: false,
        invoice_info: profile.invoice.clone(),
        consignee_info: Consignee {
            consignee_name: request.recipient.name.clone(),
            consignee_mobile: request.recipient.phone.clone(),
            consignee_address: request.recipient.address.clone(),
            consignee_province_id: identifier(&address.province.code)?,
            consignee_city_id: identifier(&address.city.code)?,
            consignee_county_id: identifier(&address.county.code)?,
            consignee_town_id: identifier_or_zero(&address.town.code)?,
        },
    })
}

fn submit_line(line: &OrderLine) -> ConnectorResult<SubmitLine> {
    if !line.attributes.is_empty() || line.approved_unit_price.to_decimal() < Decimal::ZERO {
        return Err(mapping("京东商品附加选项或采购单价未核实"));
    }
    Ok(SubmitLine {
        sku_id: identifier(&line.sku.spec_id)?,
        sku_num: integer_quantity(line)?,
        sku_unit_price: ExactNumber(line.approved_unit_price.to_string()),
        sku_need_gift: false,
    })
}

pub(super) fn integer_quantity(line: &OrderLine) -> ConnectorResult<u32> {
    let quantity = line.quantity.to_decimal();
    if quantity <= Decimal::ZERO || !quantity.fract().is_zero() {
        return Err(mapping("京东订购数量须为正整数"));
    }
    let quantity: u32 = quantity
        .to_string()
        .split('.')
        .next()
        .unwrap_or_default()
        .parse()
        .map_err(|_| mapping("京东订购数量超出范围"))?;
    if quantity > u32::try_from(i32::MAX).expect("int正上限可表示为u32") {
        return Err(mapping("京东订购数量超出接口范围"));
    }
    Ok(quantity)
}

pub(super) fn identifier(value: &str) -> ConnectorResult<u64> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(mapping("京东编号格式无效"));
    }
    let id: u64 = value.parse().map_err(|_| mapping("京东编号超出范围"))?;
    if id == 0 || id > u64::try_from(i64::MAX).expect("long正上限可表示为u64") {
        return Err(mapping("京东编号超出接口范围"));
    }
    Ok(id)
}

pub(super) fn identifier_or_zero(value: &str) -> ConnectorResult<u64> {
    if value == "0" { Ok(0) } else { identifier(value) }
}

pub(super) fn valid_order_no(value: &str) -> ConnectorResult<()> {
    if value.is_empty()
        || value.len() > 100
        || !value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_-.:".contains(&byte))
    {
        return Err(mapping("京东原第三方订单号格式无效"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_supply::ports::connector::address::{ConfirmedSupplierAddress, SupplierAddressLevel};
    use erp_supply::ports::connector::common::{ActionKey, SupplierSku};
    use erp_supply::ports::connector::offer::DeliveryContext;
    use erp_supply::ports::connector::order::{DeliveryChoice, Recipient};

    use super::*;

    fn profile() -> JdVopExecutionProfile {
        JdVopExecutionProfile {
            source_type: 3,
            evidence_reference: "verified-g0".into(),
            invoice: JdVopInvoiceProfile {
                invoice_type: 23,
                invoice_put_type: 2,
                invoice_selected_title: 5,
                invoice_content_type: 1,
                invoice_company_name: "Buyer".into(),
                invoice_reg_code: Some("TAX1".into()),
                invoice_reg_company_name: None,
                invoice_reg_address: None,
                invoice_reg_phone: None,
                invoice_reg_bank: None,
                invoice_reg_bank_account: None,
                recipient: None,
            },
            verified_external_funds_account_ref: "funds-account".into(),
            confirmation_funds_policy: None,
            preflight_validity_seconds: 60,
            ordinary_skus: BTreeSet::from(["123".into()]),
            verified_message_types: None,
            verified_funds_profile: None,
        }
    }

    fn request() -> CreateOrder {
        let level = |code: &str| SupplierAddressLevel { code: code.into(), name: code.into() };
        CreateOrder {
            action: ActionKey { id: "create".into(), payload_hash: "hash".into() },
            merchant_order_no: "original-order".into(),
            lines: vec![OrderLine {
                line_id: "line".into(),
                sku: SupplierSku { product_id: None, spec_id: "123".into() },
                quantity: "1".parse().unwrap(),
                approved_unit_price: "10".parse().unwrap(),
                attributes: vec![],
                source_purchase_terms: None,
            }],
            recipient: Recipient {
                name: "Buyer".into(),
                phone: "13800000000".into(),
                region: "region".into(),
                address: "road".into(),
                geocoded_address: None,
                supplier_address: Some(ConfirmedSupplierAddress {
                    connection_id: "connection".into(),
                    snapshot_reference: "snapshot".into(),
                    address_fingerprint: "fingerprint".into(),
                    version: 1,
                    province: level("1"),
                    city: level("2"),
                    county: level("3"),
                    town: SupplierAddressLevel { code: "0".into(), name: "无需第四级地址".into() },
                }),
            },
            delivery_context: DeliveryContext { reference: "context".into(), supplier_address: None },
            delivery: DeliveryChoice {
                method: DeliveryMethod::Courier,
                shipping_fee: "0".parse().unwrap(),
                reference: "choice".into(),
            },
            approved_total: "10".parse().unwrap(),
        }
    }

    #[test]
    fn submit_encodes_absent_town_as_zero_without_relaxing_other_supplier_identifiers() {
        let mut request = request();
        let payload = submit(&request, &profile()).unwrap();
        assert_eq!(payload.consignee_info.consignee_town_id, 0);
        assert_eq!(payload.consignee_info.consignee_province_id, 1);
        assert_eq!(payload.consignee_info.consignee_city_id, 2);
        assert_eq!(payload.consignee_info.consignee_county_id, 3);

        for depth in 0..3 {
            let address = request.recipient.supplier_address.as_mut().unwrap();
            let level = match depth {
                0 => &mut address.province,
                1 => &mut address.city,
                _ => &mut address.county,
            };
            let original = level.code.clone();
            level.code = "0".into();
            assert!(submit(&request, &profile()).is_err());
            let address = request.recipient.supplier_address.as_mut().unwrap();
            match depth {
                0 => address.province.code = original,
                1 => address.city.code = original,
                _ => address.county.code = original,
            }
        }

        assert!(identifier("0").is_err());
        for invalid in ["", "00", "-1", "0.0", "town"] {
            request.recipient.supplier_address.as_mut().unwrap().town.code = invalid.into();
            assert!(submit(&request, &profile()).is_err());
        }
    }
}
