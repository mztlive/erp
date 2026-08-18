//! 定义管理 HTTP 薄包装。
//!
//! 路径参数与字符串版本属于 HTTP 形态差异；节点写请求复用 Service DTO。

use serde::Deserialize;
use services::approval::definition_dto::DefinitionNodeRequest;

/// 整组替换草稿节点的 HTTP 请求。
///
/// `definition_id` 来自路径，不得出现在请求体。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReplaceNodesHttpRequest {
    /// 期望的定义锁版本。
    pub expected_definition_lock_version: String,
    /// 有序节点。
    pub nodes: Vec<DefinitionNodeRequest>,
}

/// 发布或退役草稿的 HTTP 请求。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DefinitionLockHttpRequest {
    /// 期望的定义锁版本。
    pub expected_definition_lock_version: String,
    /// 幂等键。
    pub idempotency_key: String,
}

/// 定义期可选审批人查询。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EligibleAssigneesQuery {
    /// 姓名或账号检索。
    pub search: Option<String>,
    /// 稳定游标。
    pub cursor: Option<String>,
    /// 页大小，默认 20，最大 50。
    pub limit: Option<u32>,
}

/// 定义期候选列表默认页大小。
pub const DEFAULT_ASSIGNEE_LIMIT: u32 = 20;
/// 定义期候选列表最大页大小。
pub const MAX_ASSIGNEE_LIMIT: u32 = 50;

impl EligibleAssigneesQuery {
    /// 规范化候选查询。
    ///
    /// # 错误
    /// 页大小超过上限时返回说明。
    pub fn normalized_limit(&self) -> Result<u32, String> {
        let limit = self.limit.unwrap_or(DEFAULT_ASSIGNEE_LIMIT);
        if (1..=MAX_ASSIGNEE_LIMIT).contains(&limit) {
            return Ok(limit);
        }
        Err(format!("limit 必须在 1 到 {MAX_ASSIGNEE_LIMIT} 之间"))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use services::approval::definition_dto::{CreateDefinitionDraftRequest, DefinitionNodeRequest};

    use super::{DefinitionLockHttpRequest, EligibleAssigneesQuery, ReplaceNodesHttpRequest};

    #[test]
    fn draft_and_node_requests_deny_unknown_and_forbidden_fields() {
        assert!(serde_json::from_value::<CreateDefinitionDraftRequest>(json!({
            "document_type": "stock_adjustment",
            "name": "库存",
            "draft_source": "EMPTY",
            "idempotency_key": "k1",
            "source_definition_id": "forged"
        }))
        .is_err());
        assert!(serde_json::from_value::<DefinitionNodeRequest>(json!({
            "node_name": "仓储",
            "display_order": 1,
            "assignee_user_id": "u1",
            "node_key": "client"
        }))
        .is_err());
        assert!(serde_json::from_value::<DefinitionNodeRequest>(json!({
            "node_name": "仓储",
            "display_order": 1,
            "assignee_user_id": "u1",
            "transitions": []
        }))
        .is_err());
        assert!(serde_json::from_value::<ReplaceNodesHttpRequest>(json!({
            "expected_definition_lock_version": "1",
            "nodes": [],
            "definition_id": "forged"
        }))
        .is_err());
        assert!(serde_json::from_value::<DefinitionLockHttpRequest>(json!({
            "expected_definition_lock_version": "1",
            "idempotency_key": "k1",
            "actor_id": "forged"
        }))
        .is_err());
    }

    #[test]
    fn eligible_assignee_query_caps_limit() {
        let query = EligibleAssigneesQuery {
            search: Some("张".to_string()),
            cursor: None,
            limit: Some(50),
        };
        assert_eq!(query.normalized_limit().expect("max"), 50);
        let overflow = EligibleAssigneesQuery {
            search: None,
            cursor: None,
            limit: Some(51),
        };
        assert!(overflow.normalized_limit().is_err());
    }
}
