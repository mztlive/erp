use std::collections::HashMap;

use bpm::graph::DefinitionGraph;
use bpm::model::{ApprovalNodeDefinition, ApprovalProcessDefinition};
use database::repository::bpm::DefinitionCatalogStatusFact;
use entities::document_registry::DocumentType;

use super::super::definition_dto::{
    ApprovalRequirementView, DefinitionAllowedAction, DefinitionCatalogItem, DefinitionConfigurationStatus,
    DefinitionDetailView, DefinitionNodeView, DefinitionVersionItem,
};
use super::super::policy::{ApprovalRequirement, DocumentApprovalPolicy};
use super::super::process_kind::document_type_of;
use super::DefinitionManagementVisibility;

/// 配置状态。
pub(super) fn configuration_status(
    requirement: ApprovalRequirement,
    published: Option<u32>,
    draft: Option<u32>,
) -> DefinitionConfigurationStatus {
    match requirement {
        ApprovalRequirement::NoApproval => DefinitionConfigurationStatus::NotApplicable,
        ApprovalRequirement::ProcessRequired if published.is_some() => {
            DefinitionConfigurationStatus::Published
        }
        ApprovalRequirement::ProcessRequired if draft.is_some() => DefinitionConfigurationStatus::Draft,
        ApprovalRequirement::ProcessRequired => DefinitionConfigurationStatus::MissingConfiguration,
    }
}

/// 组装单行目录。
///
/// # 参数
/// * `document_type` - 固定单据类型
/// * `policy` - 该类型审批政策
/// * `visibility` - 当前用户类型级可见范围
/// * `by_kind` - 批量目录查询返回的发布/草稿版本
///
/// # 返回
/// 返回非敏感目录行，含正确的草稿/发布配置状态。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 必须同时消费 published 与 draft 事实；仅有草稿时不得报缺失配置。
pub(super) fn catalog_item(
    document_type: DocumentType,
    policy: &DocumentApprovalPolicy,
    visibility: &DefinitionManagementVisibility,
    by_kind: &HashMap<bpm::ProcessKind, DefinitionCatalogStatusFact>,
) -> DefinitionCatalogItem {
    let (published_version, draft_version) = catalog_versions_from_facts(policy, by_kind);
    DefinitionCatalogItem {
        document_type,
        document_type_label: document_type.label().to_string(),
        approval_requirement: requirement_view(policy.requirement()),
        published_version,
        draft_version,
        configuration_status: configuration_status(policy.requirement(), published_version, draft_version),
        allowed_actions: allowed_actions(
            policy.requirement(),
            visibility.can_define(document_type),
            published_version,
            draft_version,
        ),
    }
}

/// 从批量目录事实读取某政策的发布与草稿版本。
///
/// # 参数
/// * `policy` - 单据审批政策
/// * `by_kind` - 按流程种类索引的目录事实
///
/// # 返回
/// 无审批类型两个版本均为空；必须审批类型取对应事实，缺失种类视为双空。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 不得再按类型逐次查询仓储。
pub(super) fn catalog_versions_from_facts(
    policy: &DocumentApprovalPolicy,
    by_kind: &HashMap<bpm::ProcessKind, DefinitionCatalogStatusFact>,
) -> (Option<u32>, Option<u32>) {
    if !matches!(policy, DocumentApprovalPolicy::ProcessRequired(_)) {
        return (None, None);
    }
    match by_kind.get(&policy.process_kind()) {
        Some(fact) => (fact.published_version, fact.draft_version),
        None => (None, None),
    }
}

/// 按流程种类索引目录事实。
///
/// # 参数
/// * `facts` - 仓储一次查询返回的目录投影
///
/// # 返回
/// 返回以流程种类为键的映射。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 仓储已对重复状态失败关闭，本函数不再挑选版本。
pub(super) fn catalog_facts_by_kind(
    facts: Vec<DefinitionCatalogStatusFact>,
) -> HashMap<bpm::ProcessKind, DefinitionCatalogStatusFact> {
    facts.into_iter().map(|fact| (fact.process_kind, fact)).collect()
}

/// 类型级允许动作。
pub(super) fn allowed_actions(
    requirement: ApprovalRequirement,
    can_define: bool,
    published: Option<u32>,
    draft: Option<u32>,
) -> Vec<DefinitionAllowedAction> {
    if !can_define || requirement != ApprovalRequirement::ProcessRequired {
        return Vec::new();
    }
    let mut actions = Vec::new();
    if draft.is_none() {
        actions.push(DefinitionAllowedAction::CreateDraft);
    } else {
        actions.push(DefinitionAllowedAction::ReplaceNodes);
        actions.push(DefinitionAllowedAction::Publish);
    }
    if published.is_some() {
        actions.push(DefinitionAllowedAction::Retire);
    }
    actions
}

/// 审批要求视图。
fn requirement_view(requirement: ApprovalRequirement) -> ApprovalRequirementView {
    match requirement {
        ApprovalRequirement::NoApproval => ApprovalRequirementView::NoApproval,
        ApprovalRequirement::ProcessRequired => ApprovalRequirementView::ProcessRequired,
    }
}

/// 构造详情视图。
pub(super) fn detail_view(graph: &DefinitionGraph) -> DefinitionDetailView {
    let document_type = document_type_of(graph.definition.process_kind);
    let mut nodes = graph.nodes.clone();
    nodes.sort_by_key(|node| node.display_order);
    DefinitionDetailView {
        definition_id: graph.definition.base.id.clone(),
        document_type,
        document_type_label: document_type.label().to_string(),
        name: graph.definition.name.clone(),
        definition_version: graph.definition.definition_version,
        status: graph.definition.status.as_str().to_string(),
        entry_node_key: graph.definition.entry_node_key.clone(),
        definition_lock_version: graph.definition.definition_lock_version(),
        nodes: nodes.iter().map(node_view).collect(),
        created_by: graph.definition.created_by.as_str().to_string(),
        published_by: graph
            .definition
            .published_by
            .as_ref()
            .map(|item| item.as_str().to_string()),
        published_at: graph.definition.published_at.map(|item| item.unix_secs()),
        retired_by: graph
            .definition
            .retired_by
            .as_ref()
            .map(|item| item.as_str().to_string()),
        retired_at: graph.definition.retired_at.map(|item| item.unix_secs()),
    }
}

/// 构造节点视图。
pub(super) fn node_view(node: &ApprovalNodeDefinition) -> DefinitionNodeView {
    DefinitionNodeView {
        node_id: node.base.id.clone(),
        node_key: node.node_key.clone(),
        node_name: node.node_name.clone(),
        node_type: node.node_type.as_str().to_string(),
        node_purpose: node.node_purpose.clone(),
        display_order: node.display_order,
        assignee_user_id: node.assignee_participant_id.as_str().to_string(),
        assignee_name_snapshot: node.assignee_label_snapshot.clone(),
    }
}

/// 构造版本摘要。
pub(super) fn version_item(definition: &ApprovalProcessDefinition) -> DefinitionVersionItem {
    DefinitionVersionItem {
        definition_id: definition.base.id.clone(),
        definition_version: definition.definition_version,
        status: definition.status.as_str().to_string(),
        name: definition.name.clone(),
        definition_lock_version: definition.definition_lock_version(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::policy::policy_of;
    use super::super::test_support::{production_source, source_fn, two_node_publish_graph};
    use super::*;
    use bpm::{ParticipantId, ProcessKind, Timestamp};

    /// 目录状态必须同时消费 published/draft：仅草稿为 Draft，退役且无草稿为缺失。
    #[test]
    fn catalog_status_consumes_published_and_draft_facts() {
        assert_eq!(
            configuration_status(ApprovalRequirement::ProcessRequired, None, None),
            DefinitionConfigurationStatus::MissingConfiguration
        );
        assert_eq!(
            configuration_status(ApprovalRequirement::ProcessRequired, Some(1), None),
            DefinitionConfigurationStatus::Published
        );
        assert_eq!(
            configuration_status(ApprovalRequirement::ProcessRequired, Some(1), Some(2)),
            DefinitionConfigurationStatus::Published
        );
        assert_eq!(
            configuration_status(ApprovalRequirement::ProcessRequired, None, Some(1)),
            DefinitionConfigurationStatus::Draft
        );
        assert_eq!(
            configuration_status(ApprovalRequirement::NoApproval, None, None),
            DefinitionConfigurationStatus::NotApplicable
        );
        let production = production_source();
        let catalog = source_fn(
            production,
            "pub async fn definition_catalog",
            "pub async fn create_definition_draft",
        );
        assert!(catalog.contains("definition_catalog_facts"));
        assert!(!catalog.contains("find_published_by_process_kind"));
        assert!(!catalog.contains("find_active_draft"));
        let catalog_item_src = source_fn(production, "fn catalog_item(", "fn catalog_versions_from_facts");
        assert!(catalog_item_src.contains("catalog_versions_from_facts"));
        assert!(!production_source().contains("async fn catalog_versions"));
        assert!(production_source().contains("NodeReplacementDraft::new"));
        let facts = vec![
            DefinitionCatalogStatusFact {
                process_kind: ProcessKind::SalesOrder,
                published_version: Some(2),
                draft_version: Some(3),
            },
            DefinitionCatalogStatusFact {
                process_kind: ProcessKind::StockAdjustment,
                published_version: None,
                draft_version: Some(1),
            },
        ];
        let by_kind = catalog_facts_by_kind(facts);
        let sales = policy_of(DocumentType::SalesOrder).unwrap();
        let stock = policy_of(DocumentType::StockAdjustment).unwrap();
        assert_eq!(catalog_versions_from_facts(&sales, &by_kind), (Some(2), Some(3)));
        assert_eq!(catalog_versions_from_facts(&stock, &by_kind), (None, Some(1)));
        let purchase = policy_of(DocumentType::PurchaseOrder).unwrap();
        assert_eq!(catalog_versions_from_facts(&purchase, &by_kind), (None, None));
    }

    /// 详情按 display_order 排序；版本摘要带状态、名称和锁。
    #[test]
    fn detail_and_version_views_assemble_sorted_nodes_and_audit() {
        let mut graph = two_node_publish_graph();
        graph.nodes.reverse();
        let draft_view = detail_view(&graph);
        assert_eq!(
            draft_view
                .nodes
                .iter()
                .map(|item| item.display_order)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(draft_view.nodes[0].node_key, "n1");
        assert_eq!(draft_view.nodes[0].node_type, "USER_APPROVAL");
        assert_eq!(draft_view.nodes[1].node_type, "USER_APPROVAL");
        assert_eq!(draft_view.entry_node_key, "n1");
        assert_eq!(draft_view.created_by, "admin");
        assert_eq!(
            draft_view.definition_lock_version,
            graph.definition.definition_lock_version()
        );
        assert!(draft_view.published_by.is_none());
        assert!(draft_view.retired_by.is_none());

        let actor = ParticipantId::new("admin").unwrap();
        graph
            .definition
            .publish(actor.clone(), Timestamp::from_unix_secs(2).unwrap())
            .unwrap();
        let published_view = detail_view(&graph);
        assert_eq!(published_view.status, "PUBLISHED");
        assert_eq!(published_view.published_by.as_deref(), Some("admin"));
        assert_eq!(published_view.published_at, Some(2));

        graph
            .definition
            .retire(actor, Timestamp::from_unix_secs(3).unwrap())
            .unwrap();
        let retired_view = detail_view(&graph);
        assert_eq!(retired_view.status, "RETIRED");
        assert_eq!(retired_view.retired_by.as_deref(), Some("admin"));
        assert_eq!(retired_view.retired_at, Some(3));

        let item = version_item(&graph.definition);
        assert_eq!(item.definition_id, graph.definition.base.id);
        assert_eq!(item.definition_version, 1);
        assert_eq!(item.status, "RETIRED");
        assert_eq!(item.name, "测试流程");
        assert_eq!(
            item.definition_lock_version,
            graph.definition.definition_lock_version()
        );
        assert_eq!(node_view(&graph.nodes[1]).node_id, "id1");
    }
}
