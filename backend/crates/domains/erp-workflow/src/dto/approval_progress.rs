//! 发起人审批进展的 HTTP 数据合同。

use serde::{Deserialize, Serialize};

/// 只允许确认实际展示过的版本；账号从认证上下文取得。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadApprovalProgressRequest {
    pub observed_instance_version: String,
}

/// 按审批单去重后的个人未读数量。
#[derive(Debug, Serialize)]
pub struct ApprovalProgressCount {
    pub unread_count: u64,
}
