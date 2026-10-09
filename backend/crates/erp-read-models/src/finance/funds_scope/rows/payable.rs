//! 应付范围行的响应合同。

use erp_core::money::Amount;
use erp_finance::dto::payable::{PayableEntryView, PaymentGuidanceView, PaymentRecipientView};
use erp_finance::entity::payable::{PayableAccountStatus, PayableSourceType};
use serde::Serialize;

/// M09 应付往来子账范围行：部分授权只返获授权份额，整单金额为 null。
#[derive(Debug, Clone, Serialize)]
pub struct ScopedPayableAccountRow {
    /// 子账主键。
    pub id: String,
    /// 来源单据。
    pub source_document_id: String,
    /// 来源类型。
    pub source_type: PayableSourceType,
    /// 往来供应商。
    pub supplier_id: String,
    /// 子账状态。
    pub status: PayableAccountStatus,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 获授权核销份额合计。
    pub visible_settled_share: Amount,
    /// 整单含税应付总额；部分授权为 null。
    pub gross_total: Option<Amount>,
    /// 整单已核销合计；部分授权为 null。
    pub settled_total: Option<Amount>,
    /// 未分配余额；部分授权为 null。
    pub open_total: Option<Amount>,
    /// 剩余可收票额度；部分授权为 null，整单资格返回真实余额。
    pub open_invoiceable_total: Option<Amount>,
    /// 已收票合计；部分授权为 null，整单资格返回真实已收票额。
    pub invoiced_total: Option<Amount>,
    /// 部分受限时为 true。
    pub permission_limited: bool,
    /// 来源采购单当前采购负责人。
    pub procurement_owner_user_id: Option<String>,
    /// 来源采购单当前业务组织。
    pub business_org_unit_id: Option<String>,
    /// 付款工作台需要的当前默认收款账户。列表不填，详情在能解析时返回。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payment_recipient: Option<PaymentRecipientView>,
    /// 当前生效采购条款的付款建议；仅整单授权时返回。
    pub payment_guidance: Option<PaymentGuidanceView>,
    /// 应付分录。列表不填；详情必须带上，否则付款会把子账 id 当成分录 id。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<PayableEntryView>,
    /// 来源业务单号。采购来源为采购单号；空单号不返回，不得回退内部 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_document_no: Option<String>,
    /// 供应商当前法定名称。能解析时返回，供往来列表展示。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supplier_name: Option<String>,
}
