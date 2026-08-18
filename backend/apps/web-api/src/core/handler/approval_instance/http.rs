//! 审批实例 HTTP 请求与列表合同。
//!
//! 写请求使用 `deny_unknown_fields`，拒绝 instance/execution/definition/next 等禁用字段。

use serde::Deserialize;

/// 实例列表默认页大小。
pub const DEFAULT_INSTANCE_LIMIT: u32 = 20;
/// 实例列表最大页大小。
pub const MAX_INSTANCE_LIMIT: u32 = 100;
/// 历史默认页大小。
pub const DEFAULT_HISTORY_LIMIT: u32 = 50;
/// 历史最大页大小。
pub const MAX_HISTORY_LIMIT: u32 = 100;
/// 详情最近执行条数上限。
pub const DETAIL_HISTORY_LIMIT: u32 = 20;
/// 改派候选人默认页大小。
pub const DEFAULT_REASSIGNEE_LIMIT: u32 = 20;
/// 改派候选人最大页大小。
pub const MAX_REASSIGNEE_LIMIT: u32 = 50;

/// 实例列表固定视图。
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstanceListView {
    /// 本人当前开放审批任务。
    Mine,
    /// 本人发起且仍有单据读取权。
    Started,
    /// 具备管理权与 DataScope 的实例。
    Managed,
    /// 受阻管理子集。
    Blocked,
}

impl InstanceListView {
    /// 返回稳定 view 名。
    ///
    /// # 返回
    /// 返回 `mine` / `started` / `managed` / `blocked`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mine => "mine",
            Self::Started => "started",
            Self::Managed => "managed",
            Self::Blocked => "blocked",
        }
    }
}

/// 合同冻结的实例状态。
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InstanceStatusFilter {
    /// 运行中。
    Running,
    /// 已通过。
    Approved,
    /// 已取消。
    Cancelled,
    /// 受阻。
    Blocked,
}

impl InstanceStatusFilter {
    /// 返回稳定状态码。
    ///
    /// # 返回
    /// 返回 `RUNNING` 等。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "RUNNING",
            Self::Approved => "APPROVED",
            Self::Cancelled => "CANCELLED",
            Self::Blocked => "BLOCKED",
        }
    }
}

/// `GET /approval-instances` 查询。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InstanceListQuery {
    /// 固定 view。
    pub view: InstanceListView,
    /// 可选单据类型。
    pub document_type: Option<String>,
    /// 可选实例状态。
    pub status: Option<InstanceStatusFilter>,
    /// 稳定游标。
    pub cursor: Option<String>,
    /// 页大小，默认 20，最大 100。
    pub limit: Option<u32>,
}

/// 规范化后的列表查询。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedInstanceListQuery {
    /// 固定 view。
    pub view: InstanceListView,
    /// 可选单据类型。
    pub document_type: Option<String>,
    /// 可选状态。
    pub status: Option<InstanceStatusFilter>,
    /// 解码后的游标。
    pub cursor: Option<InstanceListCursor>,
    /// 页大小。
    pub limit: u32,
}

/// 编码当前 view 与两个排序字段的游标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceListCursor {
    /// 生成游标时的 view。
    pub view: InstanceListView,
    /// 第一排序字段。
    pub sort_primary: String,
    /// 第二排序字段（ID）。
    pub sort_id: String,
}

impl InstanceListCursor {
    /// 编码游标。
    ///
    /// # 返回
    /// 返回 `view|primary|id`。
    pub fn encode(&self) -> String {
        format!("{}|{}|{}", self.view.as_str(), self.sort_primary, self.sort_id)
    }

    /// 解码并校验 view。
    ///
    /// # 错误
    /// 格式非法或跨 view 使用时返回说明。
    pub fn decode(raw: &str, expected: InstanceListView) -> Result<Self, String> {
        let mut parts = raw.splitn(3, '|');
        let view = parse_cursor_view(parts.next().unwrap_or_default())?;
        let sort_primary = parts.next().unwrap_or_default();
        let sort_id = parts.next().unwrap_or_default();
        if sort_primary.is_empty() || sort_id.is_empty() {
            return Err("cursor 必须包含当前 view 的两个排序字段".to_string());
        }
        if view != expected {
            return Err("cursor 不得跨 view 使用".to_string());
        }
        Ok(Self {
            view,
            sort_primary: sort_primary.to_string(),
            sort_id: sort_id.to_string(),
        })
    }
}

impl InstanceListQuery {
    /// 校验 view/status/limit/cursor 合同。
    ///
    /// # 错误
    /// 非法组合返回说明，调用方映射为 422。
    pub fn normalize(&self) -> Result<NormalizedInstanceListQuery, String> {
        ensure_view_status(self.view, self.status)?;
        let limit = self.limit.unwrap_or(DEFAULT_INSTANCE_LIMIT);
        if !(1..=MAX_INSTANCE_LIMIT).contains(&limit) {
            return Err(format!("limit 必须在 1 到 {MAX_INSTANCE_LIMIT} 之间"));
        }
        let cursor = self
            .cursor
            .as_deref()
            .filter(|value| !value.is_empty())
            .map(|raw| InstanceListCursor::decode(raw, self.view))
            .transpose()?;
        Ok(NormalizedInstanceListQuery {
            view: self.view,
            document_type: self.document_type.clone(),
            status: self.status,
            cursor,
            limit,
        })
    }
}

/// 历史查询。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InstanceHistoryQuery {
    /// 稳定游标。
    pub cursor: Option<String>,
    /// 页大小，默认 50，最大 100。
    pub limit: Option<u32>,
}

impl InstanceHistoryQuery {
    /// 规范化历史上限。
    ///
    /// # 错误
    /// 超过上限时返回说明。
    pub fn normalized_limit(&self) -> Result<u32, String> {
        let limit = self.limit.unwrap_or(DEFAULT_HISTORY_LIMIT);
        if (1..=MAX_HISTORY_LIMIT).contains(&limit) {
            return Ok(limit);
        }
        Err(format!("limit 必须在 1 到 {MAX_HISTORY_LIMIT} 之间"))
    }
}

/// 改派候选人查询。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EligibleReassigneesQuery {
    /// 检索词。
    pub search: Option<String>,
    /// 稳定游标。
    pub cursor: Option<String>,
    /// 页大小，默认 20，最大 50。
    pub limit: Option<u32>,
}

impl EligibleReassigneesQuery {
    /// 规范化改派候选人页大小。
    ///
    /// # 错误
    /// 超过上限时返回说明。
    pub fn normalized_limit(&self) -> Result<u32, String> {
        let limit = self.limit.unwrap_or(DEFAULT_REASSIGNEE_LIMIT);
        if (1..=MAX_REASSIGNEE_LIMIT).contains(&limit) {
            return Ok(limit);
        }
        Err(format!("limit 必须在 1 到 {MAX_REASSIGNEE_LIMIT} 之间"))
    }
}

/// 决定 HTTP 请求。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SubmitDecisionHttpRequest {
    /// 当前待办。
    pub work_item_id: String,
    /// `APPROVE` 或 `REJECT`。
    pub decision: DecisionValue,
    /// 驳回原因。
    pub reason: Option<String>,
    /// 任务版本字符串。
    pub expected_task_version: String,
    /// 幂等键。
    pub idempotency_key: String,
}

/// 决定值。
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DecisionValue {
    /// 通过。
    Approve,
    /// 驳回。
    Reject,
}

impl DecisionValue {
    /// 返回稳定决定码。
    ///
    /// # 返回
    /// 返回 `APPROVE` 或 `REJECT`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "APPROVE",
            Self::Reject => "REJECT",
        }
    }
}

/// 恢复当前审批人请求。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResumeApproverHttpRequest {
    /// 期望实例版本。
    pub expected_instance_version: String,
    /// 期望执行版本。
    pub expected_execution_version: String,
    /// 期望绑定版本。
    pub expected_assignment_version: String,
    /// 可空已关闭任务版本。
    pub expected_closed_task_version: Option<String>,
    /// 幂等键。
    pub idempotency_key: String,
}

/// 改派当前审批人请求。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReassignApproverHttpRequest {
    /// 目标用户。
    pub target_user_id: String,
    /// 非空原因。
    pub reason: String,
    /// 可空已关闭任务版本。
    pub expected_closed_task_version: Option<String>,
    /// 期望实例版本。
    pub expected_instance_version: String,
    /// 期望执行版本。
    pub expected_execution_version: String,
    /// 期望绑定版本。
    pub expected_assignment_version: String,
    /// 幂等键。
    pub idempotency_key: String,
}

/// 受阻取消请求。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CancelBlockedHttpRequest {
    /// 非空原因。
    pub reason: String,
    /// 期望实例版本。
    pub expected_instance_version: String,
    /// 期望执行版本。
    pub expected_execution_version: String,
    /// 可空任务版本。
    pub expected_task_version: Option<String>,
    /// 幂等键。
    pub idempotency_key: String,
}

/// 升级未提交绑定请求。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UpgradeBindingHttpRequest {
    /// 升级原因。
    pub reason: String,
    /// 期望单据版本。
    pub expected_document_version: String,
    /// 期望绑定版本。
    pub expected_approval_binding_version: String,
    /// 幂等键。
    pub idempotency_key: String,
}

fn ensure_view_status(view: InstanceListView, status: Option<InstanceStatusFilter>) -> Result<(), String> {
    match (view, status) {
        (InstanceListView::Mine, None | Some(InstanceStatusFilter::Running)) => Ok(()),
        (InstanceListView::Mine, _) => Err("mine 只接受省略 status 或 status=RUNNING".to_string()),
        (InstanceListView::Blocked, None | Some(InstanceStatusFilter::Blocked)) => Ok(()),
        (InstanceListView::Blocked, _) => Err("blocked 只接受省略 status 或 status=BLOCKED".to_string()),
        (InstanceListView::Started | InstanceListView::Managed, _) => Ok(()),
    }
}

fn parse_cursor_view(raw: &str) -> Result<InstanceListView, String> {
    match raw {
        "mine" => Ok(InstanceListView::Mine),
        "started" => Ok(InstanceListView::Started),
        "managed" => Ok(InstanceListView::Managed),
        "blocked" => Ok(InstanceListView::Blocked),
        _ => Err("cursor 包含未知 view".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        CancelBlockedHttpRequest, InstanceListCursor, InstanceListQuery, InstanceListView,
        InstanceStatusFilter, ReassignApproverHttpRequest, ResumeApproverHttpRequest,
        SubmitDecisionHttpRequest, UpgradeBindingHttpRequest, DETAIL_HISTORY_LIMIT,
    };

    #[test]
    fn decision_request_denies_forbidden_fields() {
        let ok = serde_json::from_value::<SubmitDecisionHttpRequest>(json!({
            "work_item_id": "wi-1",
            "decision": "APPROVE",
            "reason": null,
            "expected_task_version": "3",
            "idempotency_key": "k1"
        }))
        .expect("合法决定");
        assert_eq!(ok.work_item_id, "wi-1");
        for field in [
            "instance_id",
            "execution_id",
            "definition_id",
            "next_node",
            "next_assignee",
            "actor_id",
            "subject_id",
            "reject_target",
        ] {
            let mut value = json!({
                "work_item_id": "wi-1",
                "decision": "APPROVE",
                "expected_task_version": "3",
                "idempotency_key": "k1"
            });
            value[field] = json!("forged");
            assert!(
                serde_json::from_value::<SubmitDecisionHttpRequest>(value).is_err(),
                "{field} 必须拒绝"
            );
        }
    }

    #[test]
    fn resume_reassign_cancel_upgrade_deny_unknown_fields() {
        assert!(serde_json::from_value::<ResumeApproverHttpRequest>(json!({
            "expected_instance_version": "1",
            "expected_execution_version": "1",
            "expected_assignment_version": "1",
            "idempotency_key": "k1",
            "target_user_id": "u9"
        }))
        .is_err());
        assert!(serde_json::from_value::<ReassignApproverHttpRequest>(json!({
            "target_user_id": "u9",
            "reason": "换人",
            "expected_instance_version": "1",
            "expected_execution_version": "1",
            "expected_assignment_version": "1",
            "idempotency_key": "k1",
            "recovery_action": "RETRY_CURRENT_STEP"
        }))
        .is_err());
        assert!(serde_json::from_value::<CancelBlockedHttpRequest>(json!({
            "reason": "冻结",
            "expected_instance_version": "1",
            "expected_execution_version": "1",
            "idempotency_key": "k1",
            "next_node": "n2"
        }))
        .is_err());
        assert!(serde_json::from_value::<UpgradeBindingHttpRequest>(json!({
            "reason": "升版",
            "expected_document_version": "1",
            "expected_approval_binding_version": "1",
            "idempotency_key": "k1",
            "definition_id": "def-2"
        }))
        .is_err());
    }

    #[test]
    fn list_view_status_and_cursor_contract() {
        let mine = InstanceListQuery {
            view: InstanceListView::Mine,
            document_type: None,
            status: Some(InstanceStatusFilter::Blocked),
            cursor: None,
            limit: None,
        };
        assert!(mine.normalize().is_err());
        let blocked = InstanceListQuery {
            view: InstanceListView::Blocked,
            document_type: None,
            status: Some(InstanceStatusFilter::Running),
            cursor: None,
            limit: Some(20),
        };
        assert!(blocked.normalize().is_err());
        let managed = InstanceListQuery {
            view: InstanceListView::Managed,
            document_type: None,
            status: Some(InstanceStatusFilter::Approved),
            cursor: None,
            limit: Some(20),
        };
        assert!(managed.normalize().is_ok());
        let cross = InstanceListCursor::decode("mine|1|wi-1", InstanceListView::Started);
        assert!(cross.unwrap_err().contains("跨 view"));
        let encoded = InstanceListCursor {
            view: InstanceListView::Mine,
            sort_primary: "9".to_string(),
            sort_id: "wi-1".to_string(),
        }
        .encode();
        let decoded = InstanceListCursor::decode(&encoded, InstanceListView::Mine).expect("同 view");
        assert_eq!(decoded.sort_id, "wi-1");
        assert_eq!(DETAIL_HISTORY_LIMIT, 20);
    }
}
