//! 不可变来源证据命令与视图。

use entities::supplier_settlement::SupplierSettlementSourceEvidence;
use erp_core::ids::{SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId};
use erp_core::money::Amount;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::safe_command_id;
use application_core::non_blank;

/// 录入来源证据时由客户端提供、并由服务端逐行校验与补全的行。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RecordSettlementSourceEvidenceLineRequest {
    /// 供应商子订单。
    pub supplier_fulfillment_order_id: SupplierFulfillmentOrderId,
    /// 供应商履约明细。
    pub supplier_fulfillment_item_id: SupplierFulfillmentItemId,
    /// 取消发生时间；仅取消事实尚无可关联正式记录时携带。
    pub cancel_occurred_at: Option<i64>,
    /// 取消结果正式证据引用；与取消发生时间成对。
    pub cancel_evidence_reference_id: Option<String>,
    /// 费用及账单行补证引用；服务端会并入履约/退款正式引用。
    #[validate(length(min = 1, max = 20, message = "费用及账单行证据引用必须在1-20项之间"))]
    pub evidence_reference_ids: Vec<String>,
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
    /// 供应商账单行含税金额。
    pub supplier_billed_gross: Amount,
    /// 供应商账单行不含税金额。
    pub supplier_billed_net: Amount,
    /// 供应商账单行税额。
    pub supplier_billed_tax: Amount,
}

/// 录入不可变结算来源证据批次的强命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RecordSettlementSourceEvidenceRequest {
    /// 稳定请求 ID；重复请求只返回原批次。
    #[validate(length(min = 1, max = 128, message = "请求ID长度必须在1-128之间"))]
    #[validate(custom(function = "safe_command_id", message = "请求ID格式非法"))]
    pub request_id: String,
    /// 幂等键；与请求 ID 一起纳入命令摘要。
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
    /// 结算供应商。
    pub supplier_id: SupplierAccountId,
    /// 结算期间开始（含）。
    pub period_start: String,
    /// 结算期间结束（含）。
    pub period_end: String,
    /// 供应商结算期间策略。
    #[validate(custom(function = "non_blank", message = "期间策略不能为空"))]
    pub period_policy_id: String,
    /// 供应商结算期间策略版本。
    #[validate(custom(function = "non_blank", message = "期间策略版本不能为空"))]
    pub period_policy_version: String,
    /// 期间策略时区；当前只接受 `Asia/Shanghai`。
    #[validate(custom(function = "non_blank", message = "期间策略时区不能为空"))]
    pub timezone: String,
    /// 同范围单调递增来源版本。
    #[validate(range(min = 1, message = "来源版本必须大于0"))]
    pub source_version: u64,
    /// 外部账单号。
    #[validate(custom(function = "non_blank", message = "外部账单号不能为空"))]
    pub external_bill_no: String,
    /// 外部账单版本。
    #[validate(custom(function = "non_blank", message = "外部账单版本不能为空"))]
    pub external_bill_version: String,
    /// 外部账单头正式证据引用。
    #[validate(custom(function = "non_blank", message = "外部账单证据引用不能为空"))]
    pub external_bill_evidence_reference_id: String,
    /// 逐行补证输入；订单和退款金额由服务端派生。
    #[validate(length(min = 1, max = 1000, message = "来源证据行数必须在1-1000之间"))]
    #[validate(nested)]
    pub lines: Vec<RecordSettlementSourceEvidenceLineRequest>,
}

/// 来源证据批次响应。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierSettlementSourceEvidenceView {
    pub id: String,
    pub request_id: String,
    pub supplier_id: String,
    pub period_start: String,
    pub period_end: String,
    pub period_policy_id: String,
    pub period_policy_version: String,
    pub timezone: String,
    pub source_version: u64,
    pub external_bill_no: String,
    pub external_bill_version: String,
    pub source_as_of: i64,
    pub source_hash: String,
    pub line_count: usize,
}

/// 创建结算草稿前查询最新来源证据的服务端预检参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierSettlementSourceEvidenceQuery {
    pub supplier_id: SupplierAccountId,
    #[validate(custom(function = "non_blank", message = "期间开始不能为空"))]
    pub period_start: String,
    #[validate(custom(function = "non_blank", message = "期间结束不能为空"))]
    pub period_end: String,
}

impl From<SupplierSettlementSourceEvidence> for SupplierSettlementSourceEvidenceView {
    fn from(value: SupplierSettlementSourceEvidence) -> Self {
        Self {
            id: value.base.id,
            request_id: value.request_id,
            supplier_id: value.supplier_id.to_string(),
            period_start: value.period_start.to_string(),
            period_end: value.period_end.to_string(),
            period_policy_id: value.period_policy_id,
            period_policy_version: value.period_policy_version,
            timezone: value.timezone,
            source_version: value.source_version,
            external_bill_no: value.external_bill_no,
            external_bill_version: value.external_bill_version,
            source_as_of: value.source_as_of.unix_secs(),
            source_hash: value.source_hash,
            line_count: value.lines.len(),
        }
    }
}
