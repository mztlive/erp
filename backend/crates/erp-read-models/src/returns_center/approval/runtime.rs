//! 逆向资金单据详情的审批运行事实；列表仍保持批量摘要查询。
use bpm::ids::ApprovalProcessInstanceId;
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};
use erp_workflow::BpmExt;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::service::approval::execution::{
    history_item_from_execution, history_page_from, latest_rejection_reason,
};
use mongodb::Database;
use persistence_core::NoTransaction;

use super::super::dto::{
    DocumentApprovalHistoryItemView, DocumentApprovalHistoryPageView, DocumentApprovalInstanceView,
    DocumentApprovalView,
};
use super::RECENT_HISTORY_LIMIT;
use crate::Result;

/// 为已授权单据详情补齐真实实例、当前执行和有界历史。
///
/// # 参数
/// * `db` - 数据库
/// * `document_type` - 原业务单据类型
/// * `id` - 原业务单据主键
/// * `view` - 冻结绑定和业务状态生成的只读摘要
///
/// # 返回
/// 返回保留原允许动作、补齐审批事实的详情摘要；无实例时保持空摘要。
///
/// # 错误
/// 主体非法或审批仓储读取失败时返回错误，不补默认流程或节点。
pub async fn load_runtime(
    db: &Database,
    document_type: DocumentType,
    id: &str,
    mut view: DocumentApprovalView,
) -> Result<DocumentApprovalView> {
    let subject = erp_workflow::entity::approval_integration::subject_ref_for(document_type, id)
        .map_err(|error| crate::Error::ValidationError(error.to_string()))?;
    let Some(instance) = db.bpm_workflow().find_latest_by_subject(&subject, &mut NoTransaction).await? else {
        return Ok(view);
    };
    let instance_id = ApprovalProcessInstanceId::new(instance.base.id.clone());
    let current = db.bpm_workflow().find_current_execution(&instance_id, &mut NoTransaction).await?;
    let limit = RECENT_HISTORY_LIMIT as u32;
    let rows =
        db.bpm_workflow().list_execution_history(&instance_id, None, limit + 1, &mut NoTransaction).await?;
    let page = history_page_from(rows.iter().map(history_item_from_execution).collect(), limit);
    view.instance = Some(instance_view(&instance, current.as_ref(), latest_rejection_reason(&page.items)));
    view.recent_history = page
        .items
        .iter()
        .map(|item| {
            DocumentApprovalHistoryItemView::new(
                item.execution_id.clone(),
                item.node_key.clone(),
                item.result.clone(),
            )
            .with_round_no(item.round_no)
        })
        .collect();
    view.history_page =
        DocumentApprovalHistoryPageView { next_cursor: page.next_cursor, has_more: page.has_more };
    Ok(view)
}

/// 从当前执行快照投影显示名和稳定标识，不从角色推导审批人。
fn instance_view(
    instance: &ApprovalProcessInstance,
    current: Option<&ApprovalNodeExecution>,
    latest_rejection: Option<String>,
) -> DocumentApprovalInstanceView {
    DocumentApprovalInstanceView::new(instance.base.id.clone(), instance.status.as_str().to_string())
        .with_current_round_no(instance.current_round_no)
        .with_current_node(current.map(|item| item.node_key.clone()))
        .with_current_node_name(current.map(|item| item.node_name.clone()))
        .with_current_assignee(current.map(|item| item.assignee_participant_id.as_str().to_string()))
        .with_current_assignee_name(current.and_then(|item| {
            let name = item.assignee_name_snapshot.trim();
            (!name.is_empty()).then(|| name.to_string())
        }))
        .with_latest_rejection(latest_rejection)
}

#[cfg(test)]
mod tests {
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId};
    use bpm::model::types::ApprovalExecutionAssignmentSource;
    use bpm::model::{
        NewNodeExecution, NewProcessInstance, ParticipantId, ProcessKind, SubjectRef, Timestamp,
    };

    use super::*;
    fn running_instance() -> ApprovalProcessInstance {
        ApprovalProcessInstance::start_running(NewProcessInstance {
            id: ApprovalProcessInstanceId::new("inst-1"),
            process_definition_id: ApprovalProcessDefinitionId::new("def-1"),
            definition_version: 2,
            process_kind: ProcessKind::CustomerRefund,
            subject: SubjectRef::new("customer_refund", "refund-1").unwrap(),
            subject_version: 1,
            started_by: ParticipantId::new("sales-1").unwrap(),
            at: Timestamp::from_unix_secs(10).unwrap(),
        })
        .unwrap()
    }

    fn current_execution() -> ApprovalNodeExecution {
        ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new("exec-1"),
            process_instance_id: ApprovalProcessInstanceId::new("inst-1"),
            node_key: "procurement_confirm".into(),
            node_name: "采购确认".into(),
            round_no: 1,
            execution_no: 1,
            assignment_source: ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: ParticipantId::new("u-li").unwrap(),
            assignee_name_snapshot: "李思勇".into(),
            at: Timestamp::from_unix_secs(11).unwrap(),
        })
        .unwrap()
    }

    /// 退款详情必须同时提供稳定标识和当前执行显示名。
    #[test]
    fn refund_runtime_preserves_execution_identity_and_names() {
        let view =
            instance_view(&running_instance(), Some(&current_execution()), Some("核对退款依据".into()));
        assert_eq!(view.status, "RUNNING");
        assert_eq!(view.current_round_no, 1);
        assert_eq!(view.current_node.as_deref(), Some("procurement_confirm"));
        assert_eq!(view.current_node_name.as_deref(), Some("采购确认"));
        assert_eq!(view.current_assignee.as_deref(), Some("u-li"));
        assert_eq!(view.current_assignee_name.as_deref(), Some("李思勇"));
        assert_eq!(view.latest_rejection.as_deref(), Some("核对退款依据"));
        let no_current = instance_view(&running_instance(), None, None);
        assert!(no_current.current_node.is_none());
        assert!(no_current.current_assignee_name.is_none());
    }
}
