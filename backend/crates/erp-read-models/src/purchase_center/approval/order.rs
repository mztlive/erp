//! 冻结绑定与业务状态的采购审批只读投影。
use bpm::engine::DefinitionGraph;
use erp_procurement::entity::purchase_order::PurchaseOrderStatus;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::approval::policy::ApprovalRequirement;

use super::super::dto::{
    DocumentApprovalDefinitionView, DocumentApprovalHistoryPageView, DocumentApprovalInstanceView,
    DocumentApprovalNodeView, DocumentApprovalView,
};
/// 详情首屏审批历史最多八项。
pub(in crate::purchase_center) const RECENT_HISTORY_LIMIT: usize = 8;
/// 由绑定与可选实例事实构造只读审批结构。
///
/// 创建后未提交只返回绑定定义；客户端不得据此选择定义或审批人。
///
/// # 参数
/// * `binding` - 创建时冻结的定义绑定
/// * `instance` - 已启动时的实例摘要
/// * `status` - 当前业务状态
///
/// # 返回
/// 返回有界只读审批结构。
pub fn document_approval_view(
    binding: Option<&ApprovalDefinitionBinding>,
    instance: Option<DocumentApprovalInstanceView>,
    status: PurchaseOrderStatus,
) -> DocumentApprovalView {
    document_approval_view_with_definition(binding, None, instance, status)
}

/// 由绑定、定义图与可选实例构造只读审批结构。
///
/// # 参数
/// * `binding` - 创建时冻结的定义绑定
/// * `graph` - 绑定对应的定义图；缺省时只保留绑定 id 与版本
/// * `instance` - 已启动时的实例摘要
/// * `status` - 当前业务状态
///
/// # 返回
/// 返回含流程名与有序节点的只读审批结构。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 节点只投影名称与顺序，不展开审批人。
pub fn document_approval_view_with_definition(
    binding: Option<&ApprovalDefinitionBinding>,
    graph: Option<&DefinitionGraph>,
    instance: Option<DocumentApprovalInstanceView>,
    status: PurchaseOrderStatus,
) -> DocumentApprovalView {
    DocumentApprovalView {
        requirement: match ApprovalRequirement::ProcessRequired {
            ApprovalRequirement::ProcessRequired => "PROCESS_REQUIRED",
            ApprovalRequirement::NoApproval => "NO_APPROVAL",
        }
        .to_string(),
        definition: binding.map(|item| definition_view_from_binding(item, graph)),
        instance,
        recent_history: Vec::new(),
        history_page: DocumentApprovalHistoryPageView { next_cursor: None, has_more: false },
        allowed_actions: allowed_document_actions(status),
    }
}

/// 由冻结绑定与可选定义图投影定义摘要。
///
/// # 参数
/// * `binding` - 创建时冻结的定义绑定
/// * `graph` - 绑定定义图；缺省时名称与节点为空
///
/// # 返回
/// 返回定义 id、名称、版本与有序节点。
pub(in crate::purchase_center) fn definition_view_from_binding(
    binding: &ApprovalDefinitionBinding,
    graph: Option<&DefinitionGraph>,
) -> DocumentApprovalDefinitionView {
    let mut nodes = graph.map(|item| item.nodes.iter().collect::<Vec<_>>()).unwrap_or_default();
    nodes.sort_by_key(|node| node.display_order);
    DocumentApprovalDefinitionView {
        id: binding.approval_process_definition_id.as_ref().to_string(),
        name: graph.map(|item| item.definition.name.clone()).unwrap_or_default(),
        version: binding.approval_definition_version,
        nodes: nodes
            .into_iter()
            .map(|node| DocumentApprovalNodeView { key: node.node_key.clone(), name: node.node_name.clone() })
            .collect(),
    }
}

/// 单据详情允许的审批相关动作。不含选择定义或审批人。
fn allowed_document_actions(status: PurchaseOrderStatus) -> Vec<String> {
    match status {
        PurchaseOrderStatus::Draft => vec!["SUBMIT".to_string()],
        PurchaseOrderStatus::InApproval => vec!["CANCEL".to_string()],
        PurchaseOrderStatus::PendingFinanceReview
        | PurchaseOrderStatus::Effective
        | PurchaseOrderStatus::PartiallyExecuted
        | PurchaseOrderStatus::Completed
        | PurchaseOrderStatus::Voided => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use bpm::ids::ApprovalProcessDefinitionId;
    use erp_core::common::time::Instant;
    use erp_workflow::service::approval::binding::binding_from_published;

    use super::*;
    /// 详情只读审批结构；允许动作不含选择定义或审批人。
    #[test]
    fn detail_approval_is_read_only_and_has_history_cap() {
        let binding =
            binding_from_published(ApprovalProcessDefinitionId::new("def-1"), 2, Instant::from_unix_secs(1))
                .unwrap();
        let view = document_approval_view(Some(&binding), None, PurchaseOrderStatus::Draft);
        assert_eq!(view.requirement, "PROCESS_REQUIRED");
        assert_eq!(view.definition.as_ref().unwrap().id, "def-1");
        assert!(view.definition.as_ref().unwrap().name.is_empty());
        assert!(view.definition.as_ref().unwrap().nodes.is_empty());
        assert!(view.instance.is_none());
        assert!(view.recent_history.len() <= RECENT_HISTORY_LIMIT);
        assert_eq!(view.allowed_actions, vec!["SUBMIT".to_string()]);
        assert!(!view.allowed_actions.iter().any(|item| item.contains("DEFINITION")));
        let running = document_approval_view(Some(&binding), None, PurchaseOrderStatus::InApproval);
        assert_eq!(running.allowed_actions, vec!["CANCEL".to_string()]);
    }
}
