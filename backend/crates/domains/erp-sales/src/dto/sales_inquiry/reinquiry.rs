//! 采购接收指定轮次的完整重询说明，不包含销售参考价。

use erp_core::common::time::Instant;
use serde::Serialize;

use crate::entity::sales_inquiry::SalesInquiry;

/// 当前采购轮次的完整重询说明。
#[derive(Debug, Clone, Serialize)]
pub struct InquiryProcurementReinquiry {
    pub initiated_by_procurement: bool,
    pub at: Instant,
    pub previous_feedback_round: u32,
    pub reason: String,
    pub concerns: String,
    pub all_items: bool,
    pub line_ids: Vec<String>,
    pub fee_ids: Vec<String>,
}

impl InquiryProcurementReinquiry {
    // 从指定业务轮次生成完整重询说明。
    // 返回最近一次匹配说明；没有匹配版本、轮次或时间时返回 None。
    // 失败条件：不回退到其他轮次或需求版本。
    //
    // 输入约定：版本、轮次、截止时刻来自当前询价或已核验的历史任务快照。
    pub(super) fn from_round(
        inquiry: &SalesInquiry,
        requirement_version: u32,
        previous_round: u32,
        before: Instant,
    ) -> Option<Self> {
        let entry = inquiry.reinquiries.iter().rev().find(|entry| {
            entry.requirement_version == requirement_version
                && entry.previous_feedback_round == previous_round
                && entry.at <= before
        })?;
        let all_items = entry.data.line_ids.is_empty() && entry.data.fee_ids.is_empty();
        Some(Self {
            initiated_by_procurement: entry.initiated_by_procurement,
            at: entry.at,
            previous_feedback_round: entry.previous_feedback_round,
            reason: entry.data.reason.clone(),
            concerns: entry.data.concerns.clone(),
            all_items,
            line_ids: entry.data.line_ids.clone(),
            fee_ids: entry.data.fee_ids.clone(),
        })
    }
}
