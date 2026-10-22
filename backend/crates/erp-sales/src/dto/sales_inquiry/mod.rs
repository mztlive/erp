//! 销售及任务采购独立响应结构，敏感字段从序列化类型上隔离。
mod procurement;
mod progress;
mod reinquiry;
use entity_core::BaseModel;
use erp_core::common::time::Instant;
use erp_core::money::Amount;
pub use procurement::*;
pub use progress::*;
pub use reinquiry::InquiryProcurementReinquiry;
use serde::Serialize;

use crate::entity::sales_inquiry::*;

/// 普通销售只能读取公开金额和处理进度，不含实际收款单据/内部行身份。
#[derive(Debug, Clone, Serialize)]
pub struct InquirySalesFeeProgress {
    pub allocation_id: String,
    pub fee_id: String,
    pub sales_order_id: String,
    pub amount: Amount,
    pub state: InquiryReservationState,
}

/// 当前可引用的已接受费用轮次及公开不可办理原因。
#[derive(Debug, Clone, Serialize)]
pub struct InquiryFeeStatus {
    pub fee_id: String,
    pub feedback_round: u32,
    pub unavailable_reason: Option<String>,
}

/// 对销售公开的整单视图；采购工作草稿不属于该类型。
#[derive(Debug, Clone, Serialize)]
pub struct InquirySalesView {
    #[serde(flatten)]
    pub base: BaseModel,
    pub inquiry_no: String,
    pub customer_id: String,
    pub customer_name: String,
    pub sales_owner_user_id: String,
    pub business_org_unit_id: String,
    pub deadline_at: Instant,
    pub copied_from_inquiry_id: Option<String>,
    pub status: InquiryStatus,
    pub requirement_version: u32,
    pub feedback_round: u32,
    pub shared_fee_owner_user_id: Option<String>,
    pub draft: InquiryRequirementData,
    pub requirements: Vec<InquiryRequirementSnapshot>,
    pub feedbacks: Vec<InquiryFeedbackSnapshot>,
    pub acceptances: Vec<InquiryAcceptance>,
    pub reinquiries: Vec<InquiryReinquiry>,
    pub events: Vec<InquiryEvent>,
    pub quantity_ledger: Vec<InquiryQuantityReservation>,
    pub sales_progress: InquirySalesProgress,
    pub fee_allocations: Vec<InquiryFeeAllocation>,
    pub fee_ledger: Vec<InquirySalesFeeProgress>,
    pub fee_statuses: Vec<InquiryFeeStatus>,
}

impl InquirySalesView {
    /// 从聚合生成确定性销售白名单响应。
    /// # 参数
    /// `inquiry` 是通过当前销售数据范围校验的询价。
    /// # 返回
    /// 返回只含销售公开事实的独立结构。
    /// # 错误
    /// 无。
    pub fn from_inquiry(inquiry: &SalesInquiry) -> Self {
        Self::from_inquiry_at(inquiry, Instant::now())
    }

    /// 按明确业务时刻生成销售白名单及已接受费用资格。
    /// # 参数
    /// `inquiry` 为已授权询价；`now` 为业务时刻。
    /// # 返回
    /// 返回独立销售响应，不暴露采购内部信息。
    /// # 错误
    /// 无；费用受影响或过期时在逐项状态中说明。
    pub fn from_inquiry_at(inquiry: &SalesInquiry, now: Instant) -> Self {
        Self {
            base: inquiry.base.clone(),
            inquiry_no: inquiry.inquiry_no.clone(),
            customer_id: inquiry.customer_id.clone(),
            customer_name: inquiry.customer_name.clone(),
            sales_owner_user_id: inquiry.sales_owner_user_id.clone(),
            business_org_unit_id: inquiry.business_org_unit_id.clone(),
            deadline_at: inquiry.deadline_at,
            copied_from_inquiry_id: inquiry.copied_from_inquiry_id.clone(),
            status: inquiry.status,
            requirement_version: inquiry.requirement_version,
            feedback_round: inquiry.feedback_round,
            shared_fee_owner_user_id: inquiry.shared_fee_owner_user_id.clone(),
            draft: inquiry.draft.clone(),
            requirements: inquiry.requirements.clone(),
            feedbacks: inquiry.feedbacks.clone(),
            acceptances: inquiry.acceptances.clone(),
            reinquiries: inquiry.reinquiries.clone(),
            events: inquiry.events.clone(),
            quantity_ledger: inquiry.quantity_ledger.clone(),
            sales_progress: InquirySalesProgress::from_inquiry(inquiry, now),
            fee_allocations: inquiry.fee_allocations.clone(),
            fee_statuses: fee_statuses(inquiry, now),
            fee_ledger: inquiry
                .fee_ledger
                .iter()
                .map(|row| InquirySalesFeeProgress {
                    allocation_id: row.allocation_id.clone(),
                    fee_id: row.fee_id.clone(),
                    sales_order_id: row.sales_order_id.clone(),
                    amount: row.amount,
                    state: row.state,
                })
                .collect(),
        }
    }
}

fn fee_statuses(inquiry: &SalesInquiry, now: Instant) -> Vec<InquiryFeeStatus> {
    let accepted_round = inquiry.acceptances.last().map(|entry| entry.feedback_round);
    let feedback = accepted_round
        .and_then(|round| inquiry.feedbacks.iter().find(|snapshot| snapshot.round == round))
        .or_else(|| inquiry.feedbacks.last());
    feedback
        .into_iter()
        .flat_map(|snapshot| {
            snapshot.data.fees.iter().map(move |fee| InquiryFeeStatus {
                fee_id: fee.fee_id.clone(),
                feedback_round: snapshot.round,
                unavailable_reason: inquiry
                    .ensure_fee_available(snapshot.round, &fee.fee_id, now)
                    .err()
                    .map(|error| error.to_string()),
            })
        })
        .collect()
}
