//! 采购仅接收本轮、本责任范围的重询说明，不接收重询中的完整销售需求。
use std::collections::HashSet;

use erp_core::common::time::Instant;
use serde::Serialize;

use crate::entity::sales_inquiry::SalesInquiry;

/// 当前采购轮次的销售重询说明；商品和费用身份已按任务范围裁剪。
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
    /// 从指定轮次及已授权范围生成重询说明。
    /// # 参数
    /// 版本、轮次、截止时刻来自当前任务或历史任务快照；行及费用身份已完成授权裁剪。
    /// # 返回
    /// 返回涉及本任务的最近一次匹配说明；无匹配、无责任行或范围不相交时返回 `None`。
    /// # 错误
    /// 无；不回退到其他轮次或需求版本。
    pub(super) fn from_round(
        inquiry: &SalesInquiry,
        requirement_version: u32,
        previous_round: u32,
        before: Instant,
        line_ids: &[String],
        fee_ids: &HashSet<String>,
    ) -> Option<Self> {
        if line_ids.is_empty() {
            return None;
        }
        let entry = inquiry.reinquiries.iter().rev().find(|entry| {
            entry.requirement_version == requirement_version
                && entry.previous_feedback_round == previous_round
                && entry.at <= before
        })?;
        let all_items = entry.data.line_ids.is_empty() && entry.data.fee_ids.is_empty();
        let lines: Vec<_> = entry.data.line_ids.iter().filter(|id| line_ids.contains(id)).cloned().collect();
        let fees: Vec<_> = entry.data.fee_ids.iter().filter(|id| fee_ids.contains(*id)).cloned().collect();
        if !all_items && lines.is_empty() && fees.is_empty() {
            return None;
        }
        Some(Self {
            initiated_by_procurement: entry.initiated_by_procurement,
            at: entry.at,
            previous_feedback_round: entry.previous_feedback_round,
            reason: entry.data.reason.clone(),
            concerns: entry.data.concerns.clone(),
            all_items,
            line_ids: lines,
            fee_ids: fees,
        })
    }
}
