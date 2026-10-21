//! 询价命令输入与安全回执；不接受供应商或内部报价字段。

use erp_core::common::time::Instant;
use erp_sales::entity::sales_inquiry::{
    InquiryCreateData, InquiryFeedbackData, InquiryReinquiryData, InquiryRequirementData, InquiryStatus,
};
pub use erp_workflow::entity::work_item::InquiryTaskAssignment as InquiryAssignment;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateInquiryCommand {
    pub command_key: String,
    pub data: InquiryCreateData,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveInquiryCommand {
    pub command_key: String,
    pub expected_version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_at: Option<Instant>,
    pub data: InquiryRequirementData,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitInquiryCommand {
    pub command_key: String,
    pub expected_version: u64,
    pub reason: String,
    pub assignments: Vec<InquiryAssignment>,
    #[serde(default)]
    pub shared_fee_owner_user_id: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryTaskCommand {
    pub command_key: String,
    pub expected_version: u64,
    pub task_id: String,
    pub task_version: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryTaskReasonCommand {
    pub command_key: String,
    pub expected_version: u64,
    pub task_id: String,
    pub task_version: u64,
    pub reason: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveInquiryFeedbackCommand {
    pub command_key: String,
    pub expected_version: u64,
    pub task_id: String,
    pub task_version: u64,
    pub data: InquiryFeedbackData,
}

/// 发布时不接受客户端拼接他人责任结果，完整反馈由服务端从各责任草稿合成。
pub type PublishInquiryCommand = InquiryTaskCommand;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptInquiryCommand {
    pub command_key: String,
    pub expected_version: u64,
    pub feedback_round: u32,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReinquireInquiryCommand {
    pub command_key: String,
    pub expected_version: u64,
    pub data: InquiryReinquiryData,
    pub assignments: Vec<InquiryAssignment>,
    #[serde(default)]
    pub shared_fee_owner_user_id: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryReasonCommand {
    pub command_key: String,
    pub expected_version: u64,
    pub reason: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReassignInquiryCommand {
    pub command_key: String,
    pub expected_version: u64,
    pub task_id: String,
    pub task_version: u64,
    pub owner_user_id: String,
    pub reason: String,
}

/// 所有询价写入的安全强类型结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InquiryMutationView {
    pub inquiry_id: String,
    pub version: u64,
    pub requirement_version: u32,
    pub feedback_round: u32,
    pub status: InquiryStatus,
    pub task_ids: Vec<String>,
}
