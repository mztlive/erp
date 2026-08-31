//! 审批实例 HTTP 请求与列表合同。
//!
//! 写请求使用 `deny_unknown_fields`，拒绝 instance/execution/definition/next 等禁用字段。

use axum::{
    extract::{FromRequestParts, Query},
    http::request::Parts,
};
use serde::Deserialize;
use services::approval::execution::{
    RuntimeInstanceListCursor, RuntimeInstanceListQuery, RuntimeInstanceListView, RuntimeInstanceStatusFilter,
};

use super::error::ApprovalHttpError;

/// 历史默认页大小。
pub const DEFAULT_HISTORY_LIMIT: u32 = 50;
/// 历史最大页大小。
pub const MAX_HISTORY_LIMIT: u32 = 100;
/// 详情最近执行条数上限。
pub const DETAIL_HISTORY_LIMIT: u32 = 20;

/// `GET /approval-instances` 查询。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InstanceListQuery {
    /// 固定 view。
    pub view: RuntimeInstanceListView,
    /// 可选单据类型。
    pub document_type: Option<String>,
    /// 可选实例状态。
    pub status: Option<RuntimeInstanceStatusFilter>,
    /// 稳定游标。
    pub cursor: Option<String>,
    /// 页大小，默认 20，最大 100。
    pub limit: Option<u32>,
    /// 可选字面量检索，匹配单据编号、对象 ID、当前节点或当前审批人。
    pub q: Option<String>,
}

/// 已完成 Axum 反序列化、opaque cursor 解码和 Service 查询校验的列表参数。
///
/// 本提取器把所有查询协议错误统一映射为 422，避免 Axum `Query` 的默认 400
/// 绕过审批 API 的稳定错误合同。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedInstanceListQuery {
    /// 响应游标必须沿用的固定视图。
    pub view: RuntimeInstanceListView,
    /// 已由 Service DTO 完成规范化和完整输入校验的查询。
    pub query: RuntimeInstanceListQuery,
}

impl<S> FromRequestParts<S> for PreparedInstanceListQuery
where
    S: Send + Sync,
{
    type Rejection = ApprovalHttpError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let headers = parts.headers.clone();
        let Query(query) = Query::<InstanceListQuery>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApprovalHttpError::unprocessable("查询参数不符合要求", &headers))?;
        Self::prepare(query, &headers)
    }
}

impl PreparedInstanceListQuery {
    /// 解码 HTTP cursor，并把查询交给 Service DTO 规范化。
    ///
    /// # 错误
    /// cursor wire 非法或 Service 查询合同不成立时返回 422。
    fn prepare(query: InstanceListQuery, headers: &axum::http::HeaderMap) -> Result<Self, ApprovalHttpError> {
        let cursor = query
            .decode_cursor()
            .map_err(|message| ApprovalHttpError::unprocessable(message, headers))?;
        let view = query.view;
        let query = RuntimeInstanceListQuery::prepare(
            view,
            query.document_type,
            query.status,
            cursor,
            query.limit,
            query.q,
        )
        .map_err(|error| match error {
            services::Error::ValidationError(message) => ApprovalHttpError::unprocessable(message, headers),
            error => ApprovalHttpError::from_service(error, headers),
        })?;
        Ok(Self { view, query })
    }
}

/// 编码当前 view 与两个排序字段的游标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceListCursor {
    /// 生成游标时的 view。
    pub view: RuntimeInstanceListView,
    /// 第一排序字段（时间）。
    pub sort_time: i64,
    /// 第二排序字段（实例 ID）。
    pub id: String,
}

impl InstanceListCursor {
    /// 编码游标。
    ///
    /// # 返回
    /// 返回 `view|primary|id`。
    pub fn encode(&self) -> String {
        format!("{}|{}|{}", self.view.as_str(), self.sort_time, self.id)
    }

    /// 解码并校验 view。
    ///
    /// # 错误
    /// 格式非法或跨 view 使用时返回说明。
    pub fn decode(raw: &str, expected: RuntimeInstanceListView) -> Result<Self, String> {
        let mut parts = raw.splitn(3, '|');
        let view = parse_cursor_view(parts.next().unwrap_or_default())?;
        let sort_time = parts.next().unwrap_or_default();
        let id = parts.next().unwrap_or_default();
        if sort_time.is_empty() || id.is_empty() {
            return Err("cursor 必须包含当前 view 的两个排序字段".to_string());
        }
        if view != expected {
            return Err("cursor 不得跨 view 使用".to_string());
        }
        let sort_time = sort_time
            .parse::<i64>()
            .map_err(|_| "cursor 的排序时间必须是整数".to_string())?;
        Ok(Self {
            view,
            sort_time,
            id: id.to_string(),
        })
    }

    /// 由 Service 游标形成 HTTP opaque cursor。
    ///
    /// # 参数
    /// * `view` - 当前查询视图
    /// * `cursor` - Service 返回的稳定游标
    ///
    /// # 返回
    /// 返回只负责 wire 编码的 HTTP 游标。
    pub fn from_runtime(view: RuntimeInstanceListView, cursor: RuntimeInstanceListCursor) -> Self {
        Self {
            view,
            sort_time: cursor.sort_time,
            id: cursor.id,
        }
    }

    /// 转为 Service 稳定游标。
    ///
    /// # 返回
    /// 返回已完成 wire 解码的 `sort_time/id`。
    pub fn into_runtime(self) -> RuntimeInstanceListCursor {
        RuntimeInstanceListCursor {
            sort_time: self.sort_time,
            id: self.id,
        }
    }
}

impl InstanceListQuery {
    /// 解码可选 HTTP opaque cursor。
    ///
    /// # 返回
    /// 未提交 cursor 时返回 `None`；否则返回 Service 稳定游标。
    ///
    /// # 错误
    /// 显式空 cursor、格式非法、排序时间越出 `i64`、跨 view 使用，或 Mine
    /// 使用负排序时间时返回说明。
    pub fn decode_cursor(&self) -> Result<Option<RuntimeInstanceListCursor>, String> {
        let Some(raw) = self.cursor.as_deref() else {
            return Ok(None);
        };
        if raw.is_empty() {
            return Err("cursor 不能为空".to_string());
        }
        let cursor = InstanceListCursor::decode(raw, self.view)?;
        if self.view == RuntimeInstanceListView::Mine && cursor.sort_time < 0 {
            return Err("mine cursor sort_time 不能为负数".to_string());
        }
        Ok(Some(cursor.into_runtime()))
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

    /// 将游标解析为上一页最后一条执行序号。
    ///
    /// # 参数
    /// 无；读取 `cursor` 字段。
    ///
    /// # 返回
    /// 首页返回 `None`；后续页返回上一页 `execution_no`。
    ///
    /// # 错误
    /// 非空但不是无符号整数时返回说明。
    pub fn normalized_cursor(&self) -> Result<Option<u32>, String> {
        let Some(raw) = self
            .cursor
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(None);
        };
        raw.parse::<u32>()
            .map(Some)
            .map_err(|_| "cursor 必须是执行序号".to_string())
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

fn parse_cursor_view(raw: &str) -> Result<RuntimeInstanceListView, String> {
    match raw {
        "mine" => Ok(RuntimeInstanceListView::Mine),
        "started" => Ok(RuntimeInstanceListView::Started),
        "managed" => Ok(RuntimeInstanceListView::Managed),
        "blocked" => Ok(RuntimeInstanceListView::Blocked),
        _ => Err("cursor 包含未知 view".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use services::approval::execution::{
        RuntimeInstanceListCursor, RuntimeInstanceListView, RuntimeInstanceStatusFilter,
    };

    use super::{
        CancelBlockedHttpRequest, InstanceHistoryQuery, InstanceListCursor, InstanceListQuery,
        ResumeApproverHttpRequest, SubmitDecisionHttpRequest, UpgradeBindingHttpRequest,
        DETAIL_HISTORY_LIMIT,
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
    fn resume_cancel_upgrade_deny_unknown_fields() {
        assert!(serde_json::from_value::<ResumeApproverHttpRequest>(json!({
            "expected_instance_version": "1",
            "expected_execution_version": "1",
            "expected_assignment_version": "1",
            "idempotency_key": "k1",
            "target_user_id": "u9"
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
    fn list_query_reuses_service_enums_and_denies_unknown_fields() {
        let query = serde_json::from_value::<InstanceListQuery>(json!({
            "view": "managed",
            "status": "APPROVED",
            "limit": 20,
            "q": "SO-1"
        }))
        .expect("Service enum wire code 保持不变");
        assert_eq!(query.view, RuntimeInstanceListView::Managed);
        assert_eq!(query.status, Some(RuntimeInstanceStatusFilter::Approved));

        assert!(serde_json::from_value::<InstanceListQuery>(json!({
            "view": "mine",
            "unexpected": true
        }))
        .is_err());
    }

    #[test]
    fn list_cursor_only_encodes_and_decodes_protocol_shape() {
        let cross = InstanceListCursor::decode("mine|1|wi-1", RuntimeInstanceListView::Started);
        assert!(cross.unwrap_err().contains("跨 view"));
        assert!(
            InstanceListCursor::decode("mine|not-a-time|wi-1", RuntimeInstanceListView::Mine,)
                .unwrap_err()
                .contains("整数")
        );
        assert!(
            InstanceListCursor::decode("mine|9223372036854775808|wi-1", RuntimeInstanceListView::Mine,)
                .is_err()
        );

        for sort_time in [i64::MIN, i64::MAX] {
            let encoded = InstanceListCursor::from_runtime(
                RuntimeInstanceListView::Mine,
                RuntimeInstanceListCursor {
                    sort_time,
                    id: "wi-1".to_string(),
                },
            )
            .encode();
            let decoded = InstanceListCursor::decode(&encoded, RuntimeInstanceListView::Mine)
                .expect("同 view 的完整 i64 时间域");
            assert_eq!(decoded.sort_time, sort_time);
            assert_eq!(decoded.id, "wi-1");
        }

        let first_page = InstanceListQuery {
            view: RuntimeInstanceListView::Mine,
            document_type: None,
            status: None,
            cursor: None,
            limit: None,
            q: None,
        };
        assert_eq!(first_page.decode_cursor().expect("未提交 cursor"), None);

        let empty = InstanceListQuery {
            cursor: Some(String::new()),
            ..first_page.clone()
        };
        assert!(empty.decode_cursor().unwrap_err().contains("不能为空"));

        let negative_mine = InstanceListQuery {
            cursor: Some("mine|-1|wi-1".to_string()),
            ..first_page.clone()
        };
        assert!(negative_mine.decode_cursor().unwrap_err().contains("不能为负数"));

        let negative_managed = InstanceListQuery {
            view: RuntimeInstanceListView::Managed,
            cursor: Some("managed|-1|inst-1".to_string()),
            ..first_page
        };
        assert_eq!(
            negative_managed
                .decode_cursor()
                .expect("Managed 使用完整 i64 时间域")
                .expect("有 cursor")
                .sort_time,
            -1
        );
        assert_eq!(DETAIL_HISTORY_LIMIT, 20);
    }

    /// 历史游标只接受执行序号。
    #[test]
    fn history_cursor_parses_execution_no() {
        let empty = InstanceHistoryQuery {
            cursor: None,
            limit: None,
        };
        assert_eq!(empty.normalized_cursor().expect("首页"), None);
        let blank = InstanceHistoryQuery {
            cursor: Some("  ".into()),
            limit: None,
        };
        assert_eq!(blank.normalized_cursor().expect("空白等同首页"), None);
        let ok = InstanceHistoryQuery {
            cursor: Some("8".into()),
            limit: Some(50),
        };
        assert_eq!(ok.normalized_cursor().expect("合法序号"), Some(8));
        let bad = InstanceHistoryQuery {
            cursor: Some("round-1".into()),
            limit: None,
        };
        assert!(bad.normalized_cursor().is_err());
    }
}
