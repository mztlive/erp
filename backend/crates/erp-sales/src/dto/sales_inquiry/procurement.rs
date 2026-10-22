//! 采购责任需求投影不包含销售参考价或整单商业资料。
use std::collections::HashSet;

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
    /// `requirement_version`、`line_ids` 与起止时间须来自已核验的本人完成任务。
    /// # 返回
    /// 返回该任务期间发布的最后一轮反馈及其需求快照。
    /// # 错误
    /// 需求或发布记录缺失时返回 `None`，不得用当前需求补齐。
    pub fn from_completed_task(
        inquiry: &SalesInquiry,
        line_ids: &[String],
        actor: &str,
        requirement_version: u32,
        started_at: Instant,
        completed_at: Instant,
    ) -> Option<Self> {
        let requirement = inquiry.requirements.iter().find(|row| row.version == requirement_version)?;
        let feedback = completed_feedback(inquiry, line_ids, actor, requirement, started_at, completed_at)?;
        let draft = requirement_view(&requirement.data, line_ids);
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
                line_ids,
                &feedback.data.fees.iter().map(|fee| fee.fee_id.clone()).collect(),
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

    /// 生成拥有完整询价任务范围时的采购投影。
    /// # 参数
    /// `inquiry` 必须已通过独立完整范围授权。
    /// # 返回
    /// 不含任何销售参考价的独立响应。
    /// # 错误
    /// 无。
    pub fn from_inquiry(inquiry: &SalesInquiry) -> Self {
        Self::from_scoped(
            inquiry,
            &inquiry.draft.lines.iter().map(|line| line.line_id.clone()).collect::<Vec<_>>(),
        )
    }

    /// 按当前任务责任行投影需求、历史和工作资料。
    /// # 参数
    /// `line_ids` 为同事务重验的当前责任范围。
    /// # 返回
    /// 仅返回责任行；整单费用由完整任务范围可见，部分任务只见本行明确关联费用。
    /// # 错误
    /// 无；空范围返回空需求及反馈行。
    pub fn from_scoped(inquiry: &SalesInquiry, line_ids: &[String]) -> Self {
        Self::from_scoped_at(inquiry, line_ids, Instant::now())
    }

    /// 使用明确业务时刻生成责任范围及费用办理状态。
    /// # 参数
    /// `inquiry`、`line_ids` 为已授权事实；`now` 为当前时刻。
    /// # 返回
    /// 返回不含销售价格或接受/重询整单历史的采购投影。
    /// # 错误
    /// 无；费用不可办理时逐项返回公开原因。
    pub fn from_scoped_at(inquiry: &SalesInquiry, line_ids: &[String], now: Instant) -> Self {
        Self::from_responsibility(inquiry, line_ids, "", now)
    }

    /// 以当前任务责任及账号生成必要需求与费用上下文。
    /// # 参数
    /// `line_ids`、`actor` 为同事务核验的当前任务事实；`now` 为业务时刻。
    /// # 返回
    /// 额外开放明确整单费用责任人自己的整单费用工作，不扩大普通单据访问。
    /// # 错误
    /// 无；空行范围不返回需求。
    pub fn from_responsibility(
        inquiry: &SalesInquiry,
        line_ids: &[String],
        actor: &str,
        now: Instant,
    ) -> Self {
        let full = includes_shared_fees(inquiry, line_ids, actor);
        let fee_ids = scoped_fee_ids(inquiry, line_ids, full);
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
                line_ids,
                &fee_ids,
            ),
            shared_fee_owner_user_id: inquiry.shared_fee_owner_user_id.clone(),
            draft: requirement_view(&inquiry.draft, line_ids),
            requirements: inquiry
                .requirements
                .iter()
                .map(|snapshot| InquiryProcurementSnapshot {
                    version: snapshot.version,
                    submitted_by: snapshot.submitted_by.clone(),
                    submitted_at: snapshot.submitted_at,
                    change_reason: snapshot.change_reason.clone(),
                    data: requirement_view(&snapshot.data, line_ids),
                })
                .collect(),
            feedbacks: feedback_history(inquiry, line_ids, full),
            feedback_work: feedback_work(inquiry, line_ids, full, actor),
            fee_ledger: fee_progress(inquiry, &fee_ids),
            fee_allocations: inquiry
                .fee_allocations
                .iter()
                .filter(|row| fee_ids.contains(&row.fee_id))
                .cloned()
                .collect(),
            fee_statuses: fee_statuses(inquiry, now)
                .into_iter()
                .filter(|status| fee_ids.contains(&status.fee_id))
                .collect(),
        }
    }
}

fn includes_shared_fees(inquiry: &SalesInquiry, line_ids: &[String], actor: &str) -> bool {
    !line_ids.is_empty()
        && (inquiry
            .current_requirement()
            .map(|snapshot| snapshot.data.lines.iter().all(|line| line_ids.contains(&line.line_id)))
            .unwrap_or(false)
            || (!actor.is_empty() && inquiry.shared_fee_owner_user_id.as_deref() == Some(actor)))
}

fn completed_feedback(
    inquiry: &SalesInquiry,
    line_ids: &[String],
    actor: &str,
    requirement: &InquiryRequirementSnapshot,
    started_at: Instant,
    completed_at: Instant,
) -> Option<InquiryFeedbackSnapshot> {
    let mut feedback = inquiry
        .feedbacks
        .iter()
        .rev()
        .find(|row| {
            row.requirement_version == requirement.version
                && row.published_at >= started_at
                && row.published_at <= completed_at
        })?
        .clone();
    let full =
        !line_ids.is_empty() && requirement.data.lines.iter().all(|line| line_ids.contains(&line.line_id));
    let own_shared_fees = inquiry
        .feedback_work
        .iter()
        .filter(|work| {
            !line_ids.is_empty()
                && !work.line_ids.is_empty()
                && work.frozen
                && work.actor_id == actor
                && work.requirement_version == requirement.version
                && work.saved_at >= started_at
                && work.saved_at <= feedback.published_at
                && work.line_ids.iter().all(|id| line_ids.contains(id))
        })
        .flat_map(|work| &work.data.fees)
        .filter(|fee| fee.line_ids.is_empty())
        .map(|fee| &fee.fee_id)
        .collect::<HashSet<_>>();
    feedback.data.lines.retain(|line| line_ids.contains(&line.line_id));
    feedback.data.fees.retain(|fee| {
        if fee.line_ids.is_empty() {
            full || own_shared_fees.contains(&fee.fee_id)
        } else {
            fee.line_ids.iter().all(|id| line_ids.contains(id))
        }
    });
    Some(feedback)
}

fn requirement_view(data: &InquiryRequirementData, line_ids: &[String]) -> InquiryProcurementRequirement {
    InquiryProcurementRequirement {
        note: data.note.clone(),
        lines: data
            .lines
            .iter()
            .filter(|line| line_ids.contains(&line.line_id))
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

fn scope_feedback(data: &mut InquiryFeedbackData, line_ids: &[String], full: bool) {
    data.lines.retain(|line| line_ids.contains(&line.line_id));
    data.fees.retain(|fee| {
        if fee.line_ids.is_empty() { full } else { fee.line_ids.iter().all(|id| line_ids.contains(id)) }
    });
}

fn feedback_history(inquiry: &SalesInquiry, line_ids: &[String], full: bool) -> Vec<InquiryFeedbackSnapshot> {
    inquiry
        .feedbacks
        .iter()
        .map(|snapshot| {
            let mut scoped = snapshot.clone();
            scope_feedback(&mut scoped.data, line_ids, full);
            scoped
        })
        .collect()
}

fn feedback_work(
    inquiry: &SalesInquiry,
    line_ids: &[String],
    full: bool,
    actor: &str,
) -> Vec<InquiryFeedbackWork> {
    inquiry
        .feedback_work
        .iter()
        .filter(|work| {
            !work.frozen
                && work.requirement_version == inquiry.requirement_version
                && (actor.is_empty() || work.actor_id == actor)
                && work.line_ids.iter().all(|id| line_ids.contains(id))
        })
        .map(|work| {
            let mut scoped = work.clone();
            scope_feedback(&mut scoped.data, line_ids, full);
            scoped
        })
        .collect()
}

fn scoped_fee_ids(inquiry: &SalesInquiry, line_ids: &[String], full: bool) -> HashSet<String> {
    inquiry
        .feedbacks
        .iter()
        .flat_map(|snapshot| &snapshot.data.fees)
        .filter(|fee| {
            if fee.line_ids.is_empty() { full } else { fee.line_ids.iter().all(|id| line_ids.contains(id)) }
        })
        .map(|fee| fee.fee_id.clone())
        .collect()
}

fn fee_progress(inquiry: &SalesInquiry, fee_ids: &HashSet<String>) -> Vec<InquirySalesFeeProgress> {
    inquiry
        .fee_ledger
        .iter()
        .filter(|entry| fee_ids.contains(&entry.fee_id))
        .map(|entry| InquirySalesFeeProgress {
            allocation_id: entry.allocation_id.clone(),
            fee_id: entry.fee_id.clone(),
            sales_order_id: entry.sales_order_id.clone(),
            amount: entry.amount,
            state: entry.state,
        })
        .collect()
}
