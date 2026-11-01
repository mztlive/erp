//! 询价状态与不可变协作记录。
use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};

use super::InquiryRequirementData;

/// 处理状态独立于报价有效性和转单进度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InquiryStatus {
    Draft,
    /// 历史状态；已有责任采购可直接处理，新提交不再写入。
    AwaitingProcurement,
    ProcurementProcessing,
    AwaitingSalesSupplement,
    AwaitingSales,
    Confirmed,
    Closed,
}

/// 创建字段不允许调用方注入版本、已发布结果或余额。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryCreateData {
    pub customer_id: String,
    pub customer_name: String,
    pub sales_owner_user_id: String,
    pub business_org_unit_id: String,
    pub deadline_at: Instant,
    #[serde(default)]
    pub copied_from_inquiry_id: Option<String>,
    pub draft: InquiryRequirementData,
}

/// 一次提交冻结的完整需求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryRequirementSnapshot {
    pub version: u32,
    pub submitted_by: String,
    pub submitted_at: Instant,
    pub change_reason: String,
    pub data: InquiryRequirementData,
}

/// 接受记录引用确切需求版本和反馈轮次。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryAcceptance {
    pub requirement_version: u32,
    pub feedback_round: u32,
    pub accepted_by: String,
    pub accepted_at: Instant,
}

/// 重询范围；空行/费用集合表示全部范围。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryReinquiryData {
    pub reason: String,
    #[serde(default)]
    pub line_ids: Vec<String>,
    #[serde(default)]
    pub fee_ids: Vec<String>,
    #[serde(default)]
    pub concerns: String,
    /// 需求变化时必须提供完整新版工作副本。
    #[serde(default)]
    pub requirement: Option<InquiryRequirementData>,
}

/// 已冻结重询事实。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryReinquiry {
    #[serde(default)]
    pub initiated_by_procurement: bool,
    pub actor_id: String,
    pub at: Instant,
    pub previous_feedback_round: u32,
    pub requirement_version: u32,
    pub data: InquiryReinquiryData,
}

/// 退回、改派及关闭等非覆盖式状态记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryEvent {
    pub actor_id: String,
    pub kind: String,
    pub reason: String,
    pub at: Instant,
}

/// 一次任务改派的原责任、新责任及原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InquiryReassignmentData {
    pub previous_owner: String,
    pub target_owner: String,
    pub line_ids: Vec<String>,
    pub reason: String,
}
