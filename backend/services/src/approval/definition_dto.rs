//! 审批流程定义写请求与目录/详情视图。
//!
//! 写请求使用 `deny_unknown_fields`，拒绝客户端提交节点键、类型、用途、连线或处理器。

use entities::document_registry::DocumentType;
use serde::{Deserialize, Serialize};

/// 草稿创建来源。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DraftSource {
    /// 创建无节点草稿。
    Empty,
    /// 从当前已发布定义复制节点。
    CurrentPublished,
}

impl DraftSource {
    /// 返回稳定代码。
    ///
    /// # 返回
    /// 返回 `EMPTY` 或 `CURRENT_PUBLISHED`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "EMPTY",
            Self::CurrentPublished => "CURRENT_PUBLISHED",
        }
    }
}

/// 创建定义草稿请求。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CreateDefinitionDraftRequest {
    /// 固定单据类型。
    pub document_type: DocumentType,
    /// 管理名称。
    pub name: String,
    /// 空草稿或从当前发布复制。
    pub draft_source: DraftSource,
    /// 幂等键。
    pub idempotency_key: String,
}

/// 草稿节点写请求。只允许定位、名称、顺序和指定审批人。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DefinitionNodeRequest {
    /// 编辑已有节点时的主键；新增节点必须为空。
    #[serde(default)]
    pub node_id: Option<String>,
    /// 节点名称。
    pub node_name: String,
    /// 从 1 开始的展示顺序。
    pub display_order: u32,
    /// 指定审批人账号。
    pub assignee_user_id: String,
}

/// 整组替换草稿节点请求。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReplaceDefinitionNodesRequest {
    /// 草稿定义主键。
    pub definition_id: String,
    /// 期望的定义锁版本。
    pub expected_definition_lock_version: u64,
    /// 有序节点。
    pub nodes: Vec<DefinitionNodeRequest>,
}

/// 发布草稿请求。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PublishDefinitionRequest {
    /// 草稿定义主键。
    pub definition_id: String,
    /// 期望的定义锁版本。
    pub expected_definition_lock_version: u64,
    /// 幂等键。
    pub idempotency_key: String,
}

/// 退役已发布定义请求。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RetireDefinitionRequest {
    /// 已发布定义主键。
    pub definition_id: String,
    /// 期望的定义锁版本。
    pub expected_definition_lock_version: u64,
    /// 幂等键。
    pub idempotency_key: String,
}

/// 目录中的审批要求。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalRequirementView {
    /// 无需审批。
    NoApproval,
    /// 必须审批。
    ProcessRequired,
}

/// 目录配置状态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DefinitionConfigurationStatus {
    /// 无审批类型。
    NotApplicable,
    /// 必须审批但没有可绑定的已发布定义。
    MissingConfiguration,
    /// 仅有活动草稿。
    Draft,
    /// 存在当前已发布定义。
    Published,
}

/// 当前用户对某类型允许的定义管理动作。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DefinitionAllowedAction {
    /// 创建草稿。
    CreateDraft,
    /// 替换草稿节点。
    ReplaceNodes,
    /// 发布草稿。
    Publish,
    /// 退役已发布定义。
    Retire,
}

/// 固定单据类型目录行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DefinitionCatalogItem {
    /// 固定单据类型。
    pub document_type: DocumentType,
    /// 中文单据类型名称。
    pub document_type_label: String,
    /// 审批要求。
    pub approval_requirement: ApprovalRequirementView,
    /// 当前发布业务版本。
    pub published_version: Option<u32>,
    /// 活动草稿业务版本。
    pub draft_version: Option<u32>,
    /// 配置状态。
    pub configuration_status: DefinitionConfigurationStatus,
    /// 当前用户允许的动作。
    pub allowed_actions: Vec<DefinitionAllowedAction>,
}

/// 历史版本摘要。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DefinitionVersionItem {
    /// 定义主键。
    pub definition_id: String,
    /// 业务版本。
    pub definition_version: u32,
    /// 定义状态。
    pub status: String,
    /// 管理名称。
    pub name: String,
    /// 锁版本。
    pub definition_lock_version: u64,
}

/// 定义节点详情。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DefinitionNodeView {
    /// 节点主键。
    pub node_id: String,
    /// 服务端生成的节点键。
    pub node_key: String,
    /// 节点名称。
    pub node_name: String,
    /// 固定为 `USER_APPROVAL`。
    pub node_type: String,
    /// 服务端保持的用途。
    pub node_purpose: Option<String>,
    /// 展示顺序。
    pub display_order: u32,
    /// 指定审批人。
    pub assignee_user_id: String,
    /// 审批人显示名快照。
    pub assignee_name_snapshot: String,
}

/// 定义详情。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DefinitionDetailView {
    /// 定义主键。
    pub definition_id: String,
    /// 固定单据类型。
    pub document_type: DocumentType,
    /// 中文单据类型名称。
    pub document_type_label: String,
    /// 管理名称。
    pub name: String,
    /// 业务版本。
    pub definition_version: u32,
    /// 定义状态。
    pub status: String,
    /// 入口节点键。
    pub entry_node_key: String,
    /// 锁版本。
    pub definition_lock_version: u64,
    /// 按展示顺序排列的节点。
    pub nodes: Vec<DefinitionNodeView>,
    /// 草稿创建人。
    pub created_by: String,
    /// 发布人。
    pub published_by: Option<String>,
    /// 发布时间。
    pub published_at: Option<i64>,
    /// 退役人。
    pub retired_by: Option<String>,
    /// 退役时间。
    pub retired_at: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 未知字段、连线、角色池、resolver、action、终点和处理器必须被拒绝。
    #[test]
    fn write_requests_deny_unknown_fields() {
        let draft = serde_json::from_str::<CreateDefinitionDraftRequest>(
            r#"{"document_type":"stock_adjustment","name":"库存","draft_source":"EMPTY","idempotency_key":"k1"}"#,
        )
        .expect("合法草稿请求应通过");
        assert_eq!(draft.draft_source, DraftSource::Empty);
        let copied = serde_json::from_str::<CreateDefinitionDraftRequest>(
            r#"{"document_type":"stock_adjustment","name":"库存","draft_source":"CURRENT_PUBLISHED","idempotency_key":"k1"}"#,
        )
        .expect("从当前发布复制的合法请求应通过");
        assert_eq!(copied.draft_source, DraftSource::CurrentPublished);

        assert!(serde_json::from_str::<CreateDefinitionDraftRequest>(
            r#"{"document_type":"stock_adjustment","name":"库存","draft_source":"UNKNOWN","idempotency_key":"k1"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<CreateDefinitionDraftRequest>(
            r#"{"document_type":"stock_adjustment","name":"库存","draft_source":"EMPTY","idempotency_key":"k1","extra":true}"#
        )
        .is_err());
        assert!(serde_json::from_str::<DefinitionNodeRequest>(
            r#"{"node_name":"仓储","display_order":1,"assignee_user_id":"u1","node_key":"client"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<DefinitionNodeRequest>(
            r#"{"node_name":"仓储","display_order":1,"assignee_user_id":"u1","node_type":"USER_APPROVAL"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<DefinitionNodeRequest>(
            r#"{"node_name":"仓储","display_order":1,"assignee_user_id":"u1","node_purpose":"X"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<ReplaceDefinitionNodesRequest>(
            r#"{"definition_id":"d1","expected_definition_lock_version":1,"nodes":[],"transitions":[]}"#
        )
        .is_err());
        assert!(serde_json::from_str::<ReplaceDefinitionNodesRequest>(
            r#"{"definition_id":"d1","expected_definition_lock_version":1,"nodes":[],"resolver":"x"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<DefinitionNodeRequest>(
            r#"{"node_name":"仓储","display_order":1,"assignee_user_id":"u1","pool":"yes"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<DefinitionNodeRequest>(
            r#"{"node_name":"仓储","display_order":1,"assignee_user_id":"u1","handler":"x"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<DefinitionNodeRequest>(
            r#"{"node_name":"仓储","display_order":1,"assignee_user_id":"u1","resolver":"x"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<DefinitionNodeRequest>(
            r#"{"node_name":"仓储","display_order":1,"assignee_user_id":"u1","action":"APPROVE"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<DefinitionNodeRequest>(
            r#"{"node_name":"仓储","display_order":1,"assignee_user_id":"u1","terminal":true}"#
        )
        .is_err());
        assert!(serde_json::from_str::<DefinitionNodeRequest>(
            r#"{"node_name":"仓储","display_order":1,"assignee_user_id":"u1","role":"approver"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<DefinitionNodeRequest>(
            r#"{"node_name":"仓储","display_order":1,"assignee_user_id":"u1","candidate_pool":[]}"#
        )
        .is_err());
        assert!(serde_json::from_str::<PublishDefinitionRequest>(
            r#"{"definition_id":"d1","expected_definition_lock_version":1,"idempotency_key":"k1","extra":true}"#
        )
        .is_err());
        assert!(serde_json::from_str::<RetireDefinitionRequest>(
            r#"{"definition_id":"d1","expected_definition_lock_version":1,"idempotency_key":"k1","force":true}"#
        )
        .is_err());
    }

    /// 编辑已有节点时 `node_id` 可选。
    #[test]
    fn node_id_is_optional_for_new_nodes() {
        let node: DefinitionNodeRequest =
            serde_json::from_str(r#"{"node_name":"仓储复核","display_order":1,"assignee_user_id":"user-1"}"#)
                .expect("新增节点不必提交 node_id");
        assert!(node.node_id.is_none());
        let existing: DefinitionNodeRequest = serde_json::from_str(
            r#"{"node_id":"n1","node_name":"仓储复核","display_order":1,"assignee_user_id":"user-1"}"#,
        )
        .expect("编辑节点可带 node_id");
        assert_eq!(existing.node_id.as_deref(), Some("n1"));
    }
}
