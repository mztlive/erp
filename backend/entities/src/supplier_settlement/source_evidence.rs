//! W27 结算来源证据批次。
//!
//! 当前 D32 已提供不可变履约明细成本与退款分配，但尚无可直接结算的外部账单、
//! 运费、服务费和关联到订单的取消事实。本实体把一次受控补证冻结为不可变批次；
//! W27 草稿创建与刷新只能消费该批次，不能接收客户端拼装的结算明细金额。

use std::collections::HashSet;

use entity_core::BaseModel;
use entity_macros::Entity;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::common::time::{BusinessDate, Instant};
use crate::errors::{Error, Result};
use crate::ids::{SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId};
use crate::money::{Amount, Quantity};
use crate::validation::normalize_required_text;

const COMMAND_ID_MAX_LEN: usize = 128;
const POLICY_VALUE_MAX_LEN: usize = 128;
const TIMEZONE_MAX_LEN: usize = 64;
const BILL_VALUE_MAX_LEN: usize = 128;
const EVIDENCE_REFERENCE_MAX_LEN: usize = 256;
const ACTOR_MAX_LEN: usize = 128;
const HASH_LEN: usize = 64;
const MAX_LINES: usize = 1_000;
const MAX_REFERENCES_PER_LINE: usize = 32;

/// 已被来源证据批次冻结的正式事实类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementSourceFactType {
    /// 供应商订单完成事实。
    FulfillmentCompleted,
    /// 供应商取消结果证据。
    CancelConfirmed,
    /// 供应商退款事实与分配。
    RefundConfirmed,
}

impl SettlementSourceFactType {
    /// 返回摘要与审计使用的稳定代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FulfillmentCompleted => "FULFILLMENT_COMPLETED",
            Self::CancelConfirmed => "CANCEL_CONFIRMED",
            Self::RefundConfirmed => "REFUND_CONFIRMED",
        }
    }
}

/// 一条来源证据的冻结金额和逐行身份。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierSettlementSourceEvidenceLine {
    /// 供应商子订单。
    pub supplier_fulfillment_order_id: SupplierFulfillmentOrderId,
    /// 供应商履约明细。
    pub supplier_fulfillment_item_id: SupplierFulfillmentItemId,
    /// 冻结数量。
    pub quantity: Quantity,
    /// 本行覆盖的正式事实类别。
    pub source_fact_types: Vec<SettlementSourceFactType>,
    /// 订单完成、取消、退款分配、费用及账单行的正式证据引用。
    pub evidence_reference_ids: Vec<String>,
    /// 订单成本含税金额。
    pub order_gross: Amount,
    /// 订单成本不含税金额。
    pub order_net: Amount,
    /// 订单成本税额。
    pub order_tax: Amount,
    /// 运费含税金额。
    pub freight_gross: Amount,
    /// 运费不含税金额。
    pub freight_net: Amount,
    /// 运费税额。
    pub freight_tax: Amount,
    /// 服务费含税金额。
    pub service_fee_gross: Amount,
    /// 服务费不含税金额。
    pub service_fee_net: Amount,
    /// 服务费税额。
    pub service_fee_tax: Amount,
    /// 退款含税金额。
    pub refund_gross: Amount,
    /// 退款不含税金额。
    pub refund_net: Amount,
    /// 退款税额。
    pub refund_tax: Amount,
    /// ERP 试算含税金额。
    pub erp_gross: Amount,
    /// ERP 试算不含税金额。
    pub erp_net: Amount,
    /// ERP 试算税额。
    pub erp_tax: Amount,
    /// 供应商账单行含税金额。
    pub supplier_billed_gross: Amount,
    /// 供应商账单行不含税金额。
    pub supplier_billed_net: Amount,
    /// 供应商账单行税额。
    pub supplier_billed_tax: Amount,
}

impl SupplierSettlementSourceEvidenceLine {
    /// 校验逐行身份、证据与金额恒等式。
    ///
    /// # 错误
    /// 缺少事实/证据、金额为负或任一 `gross = net + tax` 与 ERP 构成恒等不成立时返回错误。
    pub fn validate(&mut self) -> Result<()> {
        if self.quantity.to_decimal() <= Decimal::ZERO {
            return Err(Error::from("来源证据行数量必须大于零"));
        }
        self.source_fact_types.sort_by_key(|value| value.as_str());
        self.source_fact_types.dedup();
        if self.source_fact_types.is_empty() {
            return Err(Error::from("来源证据行至少包含一种正式事实"));
        }
        normalize_references(&mut self.evidence_reference_ids, MAX_REFERENCES_PER_LINE)?;
        if self.evidence_reference_ids.is_empty() {
            return Err(Error::from("来源证据行至少包含一个正式证据引用"));
        }
        for (value, field) in [
            (self.order_gross, "订单含税金额"),
            (self.order_net, "订单不含税金额"),
            (self.order_tax, "订单税额"),
            (self.freight_gross, "运费含税金额"),
            (self.freight_net, "运费不含税金额"),
            (self.freight_tax, "运费税额"),
            (self.service_fee_gross, "服务费含税金额"),
            (self.service_fee_net, "服务费不含税金额"),
            (self.service_fee_tax, "服务费税额"),
            (self.refund_gross, "退款含税金额"),
            (self.refund_net, "退款不含税金额"),
            (self.refund_tax, "退款税额"),
            (self.erp_gross, "ERP 含税金额"),
            (self.erp_net, "ERP 不含税金额"),
            (self.erp_tax, "ERP 税额"),
            (self.supplier_billed_gross, "供应商账单含税金额"),
            (self.supplier_billed_net, "供应商账单不含税金额"),
            (self.supplier_billed_tax, "供应商账单税额"),
        ] {
            ensure_non_negative(value, field)?;
        }
        ensure_triple(self.order_gross, self.order_net, self.order_tax, "订单金额")?;
        ensure_triple(self.freight_gross, self.freight_net, self.freight_tax, "运费金额")?;
        ensure_triple(
            self.service_fee_gross,
            self.service_fee_net,
            self.service_fee_tax,
            "服务费金额",
        )?;
        ensure_triple(self.refund_gross, self.refund_net, self.refund_tax, "退款金额")?;
        ensure_triple(self.erp_gross, self.erp_net, self.erp_tax, "ERP 金额")?;
        ensure_triple(
            self.supplier_billed_gross,
            self.supplier_billed_net,
            self.supplier_billed_tax,
            "供应商账单金额",
        )?;
        let expected_gross = self
            .order_gross
            .checked_add(self.freight_gross)
            .checked_add(self.service_fee_gross)
            .checked_sub(self.refund_gross);
        let expected_net = self
            .order_net
            .checked_add(self.freight_net)
            .checked_add(self.service_fee_net)
            .checked_sub(self.refund_net);
        let expected_tax = self
            .order_tax
            .checked_add(self.freight_tax)
            .checked_add(self.service_fee_tax)
            .checked_sub(self.refund_tax);
        if (expected_gross, expected_net, expected_tax) != (self.erp_gross, self.erp_net, self.erp_tax) {
            return Err(Error::from("ERP 金额必须逐项等于订单加费用减退款"));
        }
        Ok(())
    }
}

/// 不可变结算来源证据批次创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierSettlementSourceEvidenceData {
    /// 客户端稳定请求 ID。
    pub request_id: String,
    /// 供应商。
    pub supplier_id: SupplierAccountId,
    /// 结算期间开始（含）。
    pub period_start: BusinessDate,
    /// 结算期间结束（含）。
    pub period_end: BusinessDate,
    /// 供应商结算期间策略。
    pub period_policy_id: String,
    /// 期间策略冻结版本。
    pub period_policy_version: String,
    /// 期间策略时区。
    pub timezone: String,
    /// 同一供应商、期间和策略下单调递增的来源版本。
    pub source_version: u64,
    /// 外部账单号。
    pub external_bill_no: String,
    /// 外部账单版本。
    pub external_bill_version: String,
    /// 外部账单头证据引用。
    pub external_bill_evidence_reference_id: String,
    /// 逐行来源证据。
    pub lines: Vec<SupplierSettlementSourceEvidenceLine>,
    /// 本批次覆盖的正式事实水位。
    pub source_as_of: Instant,
    /// 记录人。
    pub recorded_by: String,
    /// 覆盖本批次全部输入与服务端派生金额的摘要。
    pub source_hash: String,
    /// 仅覆盖客户端强命令的幂等指纹。
    pub request_hash: String,
}

/// 不可变结算来源证据批次。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierSettlementSourceEvidence {
    #[serde(flatten)]
    pub base: BaseModel,
    pub request_id: String,
    pub supplier_id: SupplierAccountId,
    pub period_start: BusinessDate,
    pub period_end: BusinessDate,
    pub period_policy_id: String,
    pub period_policy_version: String,
    pub timezone: String,
    pub source_version: u64,
    pub external_bill_no: String,
    pub external_bill_version: String,
    pub external_bill_evidence_reference_id: String,
    pub lines: Vec<SupplierSettlementSourceEvidenceLine>,
    pub source_as_of: Instant,
    pub recorded_by: String,
    pub source_hash: String,
    pub request_hash: String,
}

impl SupplierSettlementSourceEvidence {
    /// 创建并完整校验一个不可变来源证据批次。
    ///
    /// # 错误
    /// 身份、周期、策略、账单、行数、逐行配对唯一性或摘要不合法时返回错误。
    pub fn new(id: impl Into<String>, data: SupplierSettlementSourceEvidenceData) -> Result<Self> {
        if data.period_end < data.period_start {
            return Err(Error::from("来源证据期间结束不得早于开始"));
        }
        if data.source_version == 0 {
            return Err(Error::from("来源版本必须大于零"));
        }
        if data.lines.is_empty() || data.lines.len() > MAX_LINES {
            return Err(Error::from("来源证据行数必须在 1-1000 之间"));
        }
        let request_id = normalize_required_text(
            data.request_id,
            "来源证据请求ID不能为空",
            COMMAND_ID_MAX_LEN,
            "来源证据请求ID过长",
        )?;
        let period_policy_id = normalize_required_text(
            data.period_policy_id,
            "结算期间策略不能为空",
            POLICY_VALUE_MAX_LEN,
            "结算期间策略过长",
        )?;
        let period_policy_version = normalize_required_text(
            data.period_policy_version,
            "结算期间策略版本不能为空",
            POLICY_VALUE_MAX_LEN,
            "结算期间策略版本过长",
        )?;
        let timezone = normalize_required_text(
            data.timezone,
            "结算期间策略时区不能为空",
            TIMEZONE_MAX_LEN,
            "结算期间策略时区过长",
        )?;
        let external_bill_no = normalize_required_text(
            data.external_bill_no,
            "外部账单号不能为空",
            BILL_VALUE_MAX_LEN,
            "外部账单号过长",
        )?;
        let external_bill_version = normalize_required_text(
            data.external_bill_version,
            "外部账单版本不能为空",
            BILL_VALUE_MAX_LEN,
            "外部账单版本过长",
        )?;
        let external_bill_evidence_reference_id = normalize_required_text(
            data.external_bill_evidence_reference_id,
            "外部账单证据引用不能为空",
            EVIDENCE_REFERENCE_MAX_LEN,
            "外部账单证据引用过长",
        )?;
        let recorded_by = normalize_required_text(
            data.recorded_by,
            "来源证据记录人不能为空",
            ACTOR_MAX_LEN,
            "来源证据记录人过长",
        )?;
        let source_hash = normalize_hash(data.source_hash)?;
        let request_hash = normalize_hash(data.request_hash)?;
        let mut lines = data.lines;
        let mut identities = HashSet::with_capacity(lines.len());
        for line in &mut lines {
            line.validate()?;
            let identity = (
                line.supplier_fulfillment_order_id.to_string(),
                line.supplier_fulfillment_item_id.to_string(),
            );
            if !identities.insert(identity) {
                return Err(Error::from("来源证据不得重复同一供应商订单明细"));
            }
        }
        lines.sort_by(|left, right| {
            left.supplier_fulfillment_item_id
                .as_ref()
                .cmp(right.supplier_fulfillment_item_id.as_ref())
        });

        Ok(Self {
            base: BaseModel::new(id.into()),
            request_id,
            supplier_id: data.supplier_id,
            period_start: data.period_start,
            period_end: data.period_end,
            period_policy_id,
            period_policy_version,
            timezone,
            source_version: data.source_version,
            external_bill_no,
            external_bill_version,
            external_bill_evidence_reference_id,
            lines,
            source_as_of: data.source_as_of,
            recorded_by,
            source_hash,
            request_hash,
        })
    }
}

fn ensure_non_negative(value: Amount, field: &str) -> Result<()> {
    if value.to_decimal() < Decimal::ZERO {
        return Err(Error::from(format!("{field}不得为负")));
    }
    Ok(())
}

fn ensure_triple(gross: Amount, net: Amount, tax: Amount, field: &str) -> Result<()> {
    if net.checked_add(tax) != gross {
        return Err(Error::from(format!("{field}必须满足含税等于不含税加税额")));
    }
    Ok(())
}

fn normalize_references(values: &mut Vec<String>, max: usize) -> Result<()> {
    if values.len() > max {
        return Err(Error::from("来源证据引用数量超限"));
    }
    for value in values.iter_mut() {
        *value = normalize_required_text(
            std::mem::take(value),
            "来源证据引用不能为空",
            EVIDENCE_REFERENCE_MAX_LEN,
            "来源证据引用过长",
        )?;
    }
    values.sort();
    values.dedup();
    Ok(())
}

fn normalize_hash(value: String) -> Result<String> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() != HASH_LEN || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Error::from("来源证据摘要必须是64位SHA-256十六进制值"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn line() -> SupplierSettlementSourceEvidenceLine {
        SupplierSettlementSourceEvidenceLine {
            supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new("order-1"),
            supplier_fulfillment_item_id: SupplierFulfillmentItemId::new("item-1"),
            quantity: Quantity::from_str("1.000000").unwrap(),
            source_fact_types: vec![SettlementSourceFactType::FulfillmentCompleted],
            evidence_reference_ids: vec!["fulfillment://order-1/item-1".to_string()],
            order_gross: amount("100.00"),
            order_net: amount("87.00"),
            order_tax: amount("13.00"),
            freight_gross: amount("10.00"),
            freight_net: amount("8.70"),
            freight_tax: amount("1.30"),
            service_fee_gross: amount("0.00"),
            service_fee_net: amount("0.00"),
            service_fee_tax: amount("0.00"),
            refund_gross: amount("5.00"),
            refund_net: amount("4.35"),
            refund_tax: amount("0.65"),
            erp_gross: amount("105.00"),
            erp_net: amount("91.35"),
            erp_tax: amount("13.65"),
            supplier_billed_gross: amount("105.00"),
            supplier_billed_net: amount("91.35"),
            supplier_billed_tax: amount("13.65"),
        }
    }

    fn data() -> SupplierSettlementSourceEvidenceData {
        SupplierSettlementSourceEvidenceData {
            request_id: "source-1".to_string(),
            supplier_id: SupplierAccountId::new("supplier-1"),
            period_start: BusinessDate::from_ymd(2026, 7, 1).unwrap(),
            period_end: BusinessDate::from_ymd(2026, 7, 31).unwrap(),
            period_policy_id: "monthly".to_string(),
            period_policy_version: "1".to_string(),
            timezone: "Asia/Shanghai".to_string(),
            source_version: 1,
            external_bill_no: "BILL-1".to_string(),
            external_bill_version: "1".to_string(),
            external_bill_evidence_reference_id: "bill://BILL-1/1".to_string(),
            lines: vec![line()],
            source_as_of: Instant::from_unix_secs(1_700_000_000),
            recorded_by: "finance-1".to_string(),
            source_hash: "a".repeat(64),
            request_hash: "b".repeat(64),
        }
    }

    #[test]
    fn source_evidence_accepts_complete_batch() {
        let evidence = SupplierSettlementSourceEvidence::new("source-1", data()).unwrap();
        assert_eq!(evidence.lines.len(), 1);
        assert_eq!(evidence.external_bill_no, "BILL-1");
    }

    #[test]
    fn source_evidence_rejects_duplicate_line_pair() {
        let mut input = data();
        input.lines.push(line());
        assert!(SupplierSettlementSourceEvidence::new("source-2", input).is_err());
    }

    #[test]
    fn source_line_rejects_guessed_tax_or_missing_reference() {
        let mut invalid = line();
        invalid.supplier_billed_tax = amount("0.00");
        assert!(invalid.validate().is_err());

        let mut missing = line();
        missing.evidence_reference_ids.clear();
        assert!(missing.validate().is_err());
    }
}
