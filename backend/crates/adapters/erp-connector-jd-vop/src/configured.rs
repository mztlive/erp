//! 将受控部署资料转换为协议执行资料；不读取采购页面输入，不发起网络。
use std::sync::Arc;

use config::JdVopConfig;
use config::jd_vop_execution::{
    JdVopExecutionConfig, JdVopFundsConfig, JdVopFundsDirectionConfig, JdVopFundsSignConfig,
    JdVopInvoiceConfig,
};
use erp_party::SensitiveDataCodec;
use erp_supply::ports::connector::common::ConnectorResult;
use erp_supply::ports::connector::funds::{SupplierFundsCurrency, SupplierFundsDirection};

use super::transport::{JdVopReadTransport, JdVopWriteTransport};
use super::{
    JdVopConfirmationFundsPolicy, JdVopExecutionProfile, JdVopFundsProfile, JdVopFundsSign,
    JdVopFundsTimeFormat, JdVopFundsTradeRule, JdVopInvoiceProfile, JdVopInvoiceRecipient, JdVopRuntime,
    mapping,
};

impl JdVopRuntime {
    /// 正式装配读取、独立消息以及已核实的订单执行能力。
    /// # 参数
    /// settings 来自 SafeConfig；传输与密文编解码器由组合根提供。
    /// # 返回
    /// 缺省只读；只提供消息配置时不提供订单写能力。
    /// # 错误
    /// 账号绑定、执行资料、资金资料或配置无效时拒绝启动，不降级吞掉错误。
    pub fn from_configured_transport(
        settings: JdVopConfig,
        read: Arc<dyn JdVopReadTransport>,
        write: Arc<dyn JdVopWriteTransport>,
        codec: Arc<SensitiveDataCodec>,
    ) -> ConnectorResult<Self> {
        settings.validate().map_err(|_| mapping("京东账号执行配置不完整或归属不一致"))?;
        let profile = settings.execution.as_ref().map(|value| execution(&settings, value)).transpose()?;
        let messages = settings.messages.clone();
        let interval = settings.message_poll_interval_seconds;
        let source_type = settings.source_type;

        let mut runtime = match profile {
            Some(profile) => Self::with_verified_execution(settings, read, write.clone(), profile, codec)?,
            None => Self::with_verified_transport(settings, read)?,
        };

        if let Some(messages) = messages {
            runtime.configure_messages(write, messages.types, source_type, interval)?;
        }
        Ok(runtime)
    }
}

fn execution(settings: &JdVopConfig, value: &JdVopExecutionConfig) -> ConnectorResult<JdVopExecutionProfile> {
    let profile = JdVopExecutionProfile {
        source_type: settings.source_type,
        evidence_reference: value.evidence_reference.clone(),
        invoice: invoice(&value.invoice),
        verified_external_funds_account_ref: value.external_funds_account_ref.clone(),
        confirmation_funds_policy: value.confirmation_funds_evidence.clone().map(|evidence_reference| {
            JdVopConfirmationFundsPolicy::OriginalOrderReservation { evidence_reference }
        }),
        preflight_validity_seconds: value.preflight_validity_seconds,
        ordinary_skus: value.ordinary_skus.clone(),
        verified_message_types: None,
        verified_funds_profile: value.funds.as_ref().map(funds),
    };
    profile.validate()?;
    if let Some(funds) = &profile.verified_funds_profile {
        funds.validate()?;
        if funds.verified_account_reference != profile.verified_external_funds_account_ref {
            return Err(mapping("京东资金查询账号与订单执行账号不一致"));
        }
    }
    Ok(profile)
}

fn invoice(value: &JdVopInvoiceConfig) -> JdVopInvoiceProfile {
    JdVopInvoiceProfile {
        invoice_type: value.invoice_type,
        invoice_put_type: value.invoice_put_type,
        invoice_selected_title: value.invoice_selected_title,
        invoice_content_type: value.invoice_content_type,
        invoice_company_name: value.invoice_company_name.clone(),
        invoice_reg_code: value.invoice_reg_code.clone(),
        invoice_reg_company_name: value.invoice_reg_company_name.clone(),
        invoice_reg_address: value.invoice_reg_address.clone(),
        invoice_reg_phone: value.invoice_reg_phone.clone(),
        invoice_reg_bank: value.invoice_reg_bank.clone(),
        invoice_reg_bank_account: value.invoice_reg_bank_account.clone(),
        recipient: value.recipient.as_ref().map(|r| JdVopInvoiceRecipient {
            invoice_name: r.invoice_name.clone(),
            invoice_phone: r.invoice_phone.clone(),
            invoice_province_id: r.invoice_province_id,
            invoice_city_id: r.invoice_city_id,
            invoice_county_id: r.invoice_county_id,
            invoice_town_id: r.invoice_town_id,
            invoice_address: r.invoice_address.clone(),
        }),
    }
}

fn funds(value: &JdVopFundsConfig) -> JdVopFundsProfile {
    JdVopFundsProfile {
        evidence_reference: value.evidence_reference.clone(),
        verified_account_reference: value.verified_account_reference.clone(),
        expected_pin: value.expected_pin.clone(),
        account_type: value.account_type.clone(),
        currency: SupplierFundsCurrency::CNY,
        timezone_offset_seconds: value.timezone_offset_seconds,
        time_format: JdVopFundsTimeFormat::LocalDateTimeSeconds,
        maximum_page_size: value.maximum_page_size,
        trade_types: value
            .trade_types
            .iter()
            .map(|(key, rule)| {
                (
                    key.clone(),
                    JdVopFundsTradeRule {
                        direction: direction(rule.direction),
                        sign: match rule.sign {
                            JdVopFundsSignConfig::Positive => JdVopFundsSign::Positive,
                            JdVopFundsSignConfig::Negative => JdVopFundsSign::Negative,
                        },
                    },
                )
            })
            .collect(),
        message_effects: value
            .message_effects
            .iter()
            .map(|(key, value)| (key.clone(), direction(*value)))
            .collect(),
    }
}

fn direction(value: JdVopFundsDirectionConfig) -> SupplierFundsDirection {
    match value {
        JdVopFundsDirectionConfig::Recharge => SupplierFundsDirection::Recharge,
        JdVopFundsDirectionConfig::Debit => SupplierFundsDirection::Debit,
        JdVopFundsDirectionConfig::Return => SupplierFundsDirection::Return,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn controlled_funds_mapping_preserves_verified_rules_and_rejects_account_mismatch() {
        let settings: JdVopConfig = serde_json::from_value(json!({"source_type":3})).unwrap();
        let mut value: JdVopExecutionConfig = serde_json::from_value(json!({
            "evidence_reference":"offline-fixture","account_scope":"account","app_key":"app",
            "external_funds_account_ref":"funds-account","confirmation_funds_evidence":"original-funds",
            "preflight_validity_seconds":60,"ordinary_skus":["123"],
            "invoice":{"invoice_type":23,"invoice_put_type":2,"invoice_selected_title":4,
                "invoice_content_type":1,"invoice_company_name":"测试公司"},
            "funds":{"evidence_reference":"funds-fixture","verified_account_reference":"funds-account",
                "expected_pin":"pin","account_type":"0","currency":"CNY","timezone_offset_seconds":28800,
                "time_format":"LocalDateTimeSeconds","maximum_page_size":100,
                "trade_types":{"1":{"direction":"Recharge","sign":"Positive"},
                    "2":{"direction":"Debit","sign":"Negative"}},"message_effects":{"109":"Return"}}
        }))
        .unwrap();
        let profile = execution(&settings, &value).unwrap();
        let funds = profile.verified_funds_profile.unwrap();
        assert_eq!(funds.verified_account_reference, "funds-account");
        assert_eq!(funds.expected_pin, "pin");
        assert_eq!(funds.timezone_offset_seconds, 28800);
        assert_eq!(funds.trade_types["2"].direction, SupplierFundsDirection::Debit);
        assert_eq!(funds.trade_types["2"].sign, JdVopFundsSign::Negative);
        assert_eq!(funds.message_effects["109"], SupplierFundsDirection::Return);

        value.funds.as_mut().unwrap().verified_account_reference = "other".into();
        assert!(execution(&settings, &value).is_err());

        value.funds.as_mut().unwrap().verified_account_reference = "funds-account".into();
        value.funds.as_mut().unwrap().trade_types.clear();
        assert!(execution(&settings, &value).is_err());
    }
}
