//! 转单进度与商品报价有效性独立于询价处理状态。
use erp_core::common::time::Instant;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::dto::sales_order::inquiry_basis::InquirySalesBasis;
use crate::entity::sales_inquiry::{InquiryReservationState, SalesInquiry};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InquiryTransferStatus {
    NotStarted,
    Partial,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InquiryQuoteValidity {
    Unavailable,
    Valid,
    PartiallyExpired,
    Expired,
}

#[derive(Debug, Clone, Serialize)]
pub struct InquirySalesProgress {
    pub transfer_status: InquiryTransferStatus,
    pub total_lines: usize,
    pub completed_lines: usize,
    pub quote_validity: InquiryQuoteValidity,
    pub can_create_sales: bool,
}

impl InquirySalesProgress {
    /// 按稳定需求行汇总跨轮次占用、消耗及当前接受结果的有效性。
    /// # 参数
    /// `inquiry` 为已授权聚合；`now` 为本次服务端读取时刻。
    /// # 返回
    /// 数量按行比较，不混加不同单位；开单资格包含有效期及重询范围。
    /// # 错误
    /// 无；缺少有效开单依据时返回不可开单，不扩大权限。
    pub fn from_inquiry(inquiry: &SalesInquiry, now: Instant) -> Self {
        let mut any_used = false;
        let mut completed_lines = 0;
        for line in &inquiry.draft.lines {
            let mut remaining = line.quantity.to_decimal();
            for row in inquiry
                .quantity_ledger
                .iter()
                .filter(|row| row.state != InquiryReservationState::Released)
                .flat_map(|row| &row.selections)
                .filter(|row| row.line_id == line.line_id)
            {
                any_used |= row.quantity.to_decimal() > Decimal::ZERO;
                remaining = (remaining - row.quantity.to_decimal()).max(Decimal::ZERO);
            }
            completed_lines +=
                usize::from(remaining == Decimal::ZERO && line.quantity.to_decimal() > Decimal::ZERO);
        }
        let total_lines = inquiry.draft.lines.len();
        let transfer_status = if total_lines > 0 && completed_lines == total_lines {
            InquiryTransferStatus::Complete
        } else if any_used {
            InquiryTransferStatus::Partial
        } else {
            InquiryTransferStatus::NotStarted
        };
        let accepted =
            inquiry.acceptances.last().filter(|row| row.requirement_version == inquiry.requirement_version);
        Self {
            transfer_status,
            total_lines,
            completed_lines,
            quote_validity: validity(inquiry, now),
            can_create_sales: accepted
                .is_some_and(|row| InquirySalesBasis::from_inquiry(inquiry, row.feedback_round, now).is_ok()),
        }
    }
}

fn validity(inquiry: &SalesInquiry, now: Instant) -> InquiryQuoteValidity {
    let Some(feedback) =
        inquiry.feedbacks.last().filter(|row| row.requirement_version == inquiry.requirement_version)
    else {
        return InquiryQuoteValidity::Unavailable;
    };
    let mut valid = false;
    let mut expired = false;
    for price in feedback.data.lines.iter().flat_map(|line| &line.prices) {
        valid |= price.valid_until > now;
        expired |= price.valid_until <= now;
    }
    match (valid, expired) {
        (true, true) => InquiryQuoteValidity::PartiallyExpired,
        (true, false) => InquiryQuoteValidity::Valid,
        (false, true) => InquiryQuoteValidity::Expired,
        (false, false) => InquiryQuoteValidity::Unavailable,
    }
}
