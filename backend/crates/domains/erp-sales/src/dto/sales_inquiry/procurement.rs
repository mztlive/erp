//! 已授权询价的采购投影；销售参考价不进入采购响应。
use entity_core::BaseModel;
use erp_core::common::time::Instant;
use erp_core::money::Quantity;
use serde::Serialize;

use super::{InquiryFeeStatus, InquiryProcurementReinquiry, InquirySalesFeeProgress, fee_statuses};
use crate::entity::sales_inquiry::*;

#[derive(Debug, Clone, Serialize)]
pub struct InquiryProcurementLine {
    pub line_id: String,
    pub sku_id: String,
    pub sku_revision_id: String,
    pub sku_code: String,
    pub product_name: String,
    pub specification: String,
    pub unit_code: String,
    pub quantity_scale: u32,
    pub quantity: Quantity,
    pub fulfillment_kind: InquiryFulfillmentKind,
    pub delivery_due_at: Instant,
    pub delivery_terms: String,
    pub allocations: Vec<InquiryDeliveryAllocation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InquiryProcurementRequirement {
    pub lines: Vec<InquiryProcurementLine>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InquiryProcurementSnapshot {
    pub version: u32,
    pub submitted_by: String,
    pub submitted_at: Instant,
    pub change_reason: String,
    pub data: InquiryProcurementRequirement,
}

impl From<&InquiryRequirementSnapshot> for InquiryProcurementSnapshot {
    /// 保留冻结需求的完整采购可见字段。
    fn from(snapshot: &InquiryRequirementSnapshot) -> Self {
        Self {
            version: snapshot.version,
            submitted_by: snapshot.submitted_by.clone(),
            submitted_at: snapshot.submitted_at,
            change_reason: snapshot.change_reason.clone(),
            data: requirement_view(&snapshot.data),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InquiryProcurementView {
    #[serde(flatten)]
    pub base: BaseModel,
    pub inquiry_no: String,
    pub customer_id: String,
    pub customer_name: String,
    pub deadline_at: Instant,
    pub status: InquiryStatus,
    pub requirement_version: u32,
    pub feedback_round: u32,
    pub reinquiry: Option<InquiryProcurementReinquiry>,
    pub shared_fee_owner_user_id: Option<String>,
    pub draft: InquiryProcurementRequirement,
    pub requirements: Vec<InquiryProcurementSnapshot>,
    pub feedbacks: Vec<InquiryFeedbackSnapshot>,
    pub feedback_work: Vec<InquiryFeedbackWork>,
    pub fee_ledger: Vec<InquirySalesFeeProgress>,
    pub fee_allocations: Vec<InquiryFeeAllocation>,
    pub fee_statuses: Vec<InquiryFeeStatus>,
}

impl InquiryProcurementView {
    /// 生成已完成任务的历史只读结果，不带入后续需求、报价或费用办理资料。
    /// # 参数
    /// requirement_version 与起止时间来自已核验完成任务，inquiry 已通过对象读取授权。
    /// # 返回
    /// 返回该任务期间发布的最后一轮完整反馈及其需求快照。
    /// # 错误
    /// 需求或发布记录缺失时返回 None，不使用当前需求补齐。
    pub fn from_completed_task(
        inquiry: &SalesInquiry,
        requirement_version: u32,
        started_at: Instant,
        completed_at: Instant,
    ) -> Option<Self> {
        let requirement = inquiry.requirements.iter().find(|row| row.version == requirement_version)?;
        let feedback = inquiry
            .feedbacks
            .iter()
            .rev()
            .find(|row| {
                row.requirement_version == requirement.version
                    && row.published_at >= started_at
                    && row.published_at <= completed_at
            })?
            .clone();
        let draft = requirement_view(&requirement.data);
        Some(Self {
            base: inquiry.base.clone(),
            inquiry_no: inquiry.inquiry_no.clone(),
            customer_id: inquiry.customer_id.clone(),
            customer_name: inquiry.customer_name.clone(),
            deadline_at: inquiry.deadline_at,
            status: inquiry.status,
            requirement_version,
            feedback_round: feedback.round,
            reinquiry: InquiryProcurementReinquiry::from_round(
                inquiry,
                requirement_version,
                feedback.round.saturating_sub(1),
                started_at,
            ),
            shared_fee_owner_user_id: None,
            draft: draft.clone(),
            requirements: vec![InquiryProcurementSnapshot {
                version: requirement.version,
                submitted_by: requirement.submitted_by.clone(),
                submitted_at: requirement.submitted_at,
                change_reason: requirement.change_reason.clone(),
                data: draft,
            }],
            feedbacks: vec![feedback],
            feedback_work: vec![],
            fee_ledger: vec![],
            fee_allocations: vec![],
            fee_statuses: vec![],
        })
    }

    /// 生成已通过询价对象读取授权的完整采购投影。
    /// # 参数
    /// inquiry 为已授权询价。
    /// # 返回
    /// 全部商品行、费用和当前有效采购工作，不包含销售参考价。
    /// # 错误
    /// 无。
    pub fn from_inquiry(inquiry: &SalesInquiry) -> Self {
        Self::from_inquiry_at(inquiry, Instant::now())
    }

    /// 按指定时刻生成完整采购资料和费用办理状态。
    /// # 参数
    /// inquiry 已通过询价对象读取授权，now 为业务时刻。
    /// # 返回
    /// 返回完整采购投影；工作仅保留当前版本且未冻结的记录。
    /// # 错误
    /// 无；费用不可办理时逐项返回公开原因。
    pub fn from_inquiry_at(inquiry: &SalesInquiry, now: Instant) -> Self {
        Self {
            base: inquiry.base.clone(),
            inquiry_no: inquiry.inquiry_no.clone(),
            customer_id: inquiry.customer_id.clone(),
            customer_name: inquiry.customer_name.clone(),
            deadline_at: inquiry.deadline_at,
            status: inquiry.status,
            requirement_version: inquiry.requirement_version,
            feedback_round: inquiry.feedback_round,
            reinquiry: InquiryProcurementReinquiry::from_round(
                inquiry,
                inquiry.requirement_version,
                inquiry.feedback_round,
                now,
            ),
            shared_fee_owner_user_id: inquiry.shared_fee_owner_user_id.clone(),
            draft: requirement_view(&inquiry.draft),
            requirements: inquiry.requirements.iter().map(InquiryProcurementSnapshot::from).collect(),
            feedbacks: inquiry.feedbacks.clone(),
            feedback_work: inquiry
                .feedback_work
                .iter()
                .filter(|work| !work.frozen && work.requirement_version == inquiry.requirement_version)
                .cloned()
                .collect(),
            fee_ledger: inquiry
                .fee_ledger
                .iter()
                .map(|entry| InquirySalesFeeProgress {
                    allocation_id: entry.allocation_id.clone(),
                    fee_id: entry.fee_id.clone(),
                    sales_order_id: entry.sales_order_id.clone(),
                    amount: entry.amount,
                    state: entry.state,
                })
                .collect(),
            fee_allocations: inquiry.fee_allocations.clone(),
            fee_statuses: fee_statuses(inquiry, now),
        }
    }
}

fn requirement_view(data: &InquiryRequirementData) -> InquiryProcurementRequirement {
    InquiryProcurementRequirement {
        note: data.note.clone(),
        lines: data
            .lines
            .iter()
            .map(|line| InquiryProcurementLine {
                line_id: line.line_id.clone(),
                sku_id: line.sku_id.clone(),
                sku_revision_id: line.sku_revision_id.clone(),
                sku_code: line.sku_code.clone(),
                product_name: line.product_name.clone(),
                specification: line.specification.clone(),
                unit_code: line.unit_code.clone(),
                quantity_scale: line.quantity_scale,
                quantity: line.quantity,
                fulfillment_kind: line.fulfillment_kind,
                delivery_due_at: line.delivery_due_at,
                delivery_terms: line.delivery_terms.clone(),
                allocations: line.allocations.clone(),
            })
            .collect(),
    }
}
