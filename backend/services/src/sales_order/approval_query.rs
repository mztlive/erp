//! 销售单详情审批实例与最近历史投影。
//!
//! 查询按主体读取最新实例，再映射为单据只读审批结构；不得在此推导下一节点。

use bpm::ids::ApprovalProcessInstanceId;
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance, SubjectRef};
use database::{BpmExt, NoTransaction};
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::sales_order::{BusinessType, CommercialStatus, ReviewStatus};
use mongodb::Database;

use super::adapter::{document_approval_view_with_history, RECENT_HISTORY_LIMIT};
use super::dto::{
    DocumentApprovalHistoryItemView, DocumentApprovalHistoryPageView, DocumentApprovalInstanceView,
    DocumentApprovalView,
};
use crate::approval::execution::{
    history_item_from_execution, history_page_from, latest_rejection_reason, RuntimeHistoryItem,
    RuntimeHistoryPage,
};
use crate::errors::Result;

/// 加载销售单详情的只读审批结构。
///
/// 未提交时实例为空；已启动后填入最新实例摘要与有界历史。
///
/// # 参数
/// * `db` - 数据库
/// * `business_type` - 用于分派 `SalesOrder` / `VoucherSalesOrder` 主体
/// * `sales_order_id` - 销售单主键
/// * `binding` - 创建时冻结的定义绑定
/// * `commercial` - 当前商业主状态
/// * `review` - 当前审核轨
///
/// # 返回
/// 返回绑定、可选实例与最近历史。
///
/// # 错误
/// 主体引用非法或仓储失败时返回错误。
#[tracing::instrument(
    name = "sales_order.load_document_approval",
    skip_all,
    fields(
        layer = "service",
        domain = "sales_order",
        operation = "load_document_approval"
    )
)]
pub async fn load_document_approval(
    db: &Database,
    business_type: BusinessType,
    sales_order_id: &str,
    binding: Option<&ApprovalDefinitionBinding>,
    commercial: CommercialStatus,
    review: ReviewStatus,
) -> Result<DocumentApprovalView> {
    let subject =
        entities::approval_integration::subject_ref_for_sales_business(business_type, sales_order_id)
            .map_err(|error| crate::errors::Error::ValidationError(error.to_string()))?;
    let runtime = load_runtime(db, &subject).await?;
    Ok(document_approval_view_with_history(
        binding,
        runtime.instance,
        runtime.recent_history,
        runtime.history_page,
        commercial,
        review,
    ))
}

/// 已投影的运行事实。
struct LoadedRuntime {
    instance: Option<DocumentApprovalInstanceView>,
    recent_history: Vec<DocumentApprovalHistoryItemView>,
    history_page: DocumentApprovalHistoryPageView,
}

/// 按主体读取最新实例并投影。
///
/// # 参数
/// * `db` - 数据库
/// * `subject` - 销售单主体引用
///
/// # 返回
/// 没有实例时返回空投影。
///
/// # 错误
/// 仓储失败时返回错误。
async fn load_runtime(db: &Database, subject: &SubjectRef) -> Result<LoadedRuntime> {
    let Some(instance) = db
        .bpm_workflow()
        .find_latest_by_subject(subject, &mut NoTransaction)
        .await?
    else {
        return Ok(empty_runtime());
    };
    project_runtime(db, instance).await
}

/// 读取当前执行与有界历史，组装实例摘要。
///
/// # 参数
/// * `db` - 数据库
/// * `instance` - 最新审批实例
///
/// # 返回
/// 返回实例摘要、最近历史与分页游标。
///
/// # 错误
/// 仓储失败时返回错误。
async fn project_runtime(db: &Database, instance: ApprovalProcessInstance) -> Result<LoadedRuntime> {
    let instance_id = ApprovalProcessInstanceId::new(instance.base.id.clone());
    let current = db
        .bpm_workflow()
        .find_current_execution(&instance_id, &mut NoTransaction)
        .await?;
    let page = load_recent_history(db, &instance_id).await?;
    Ok(LoadedRuntime {
        instance: Some(instance_view(
            &instance,
            current.as_ref(),
            latest_rejection_reason(&page.items),
        )),
        recent_history: page.items.iter().map(history_item_view).collect(),
        history_page: DocumentApprovalHistoryPageView {
            next_cursor: page.next_cursor,
            has_more: page.has_more,
        },
    })
}

/// 读取详情首屏历史，多取一条以判断是否还有后续。
///
/// # 参数
/// * `db` - 数据库
/// * `instance_id` - 审批实例主键
///
/// # 返回
/// 返回截断到 [`RECENT_HISTORY_LIMIT`] 的历史页。
///
/// # 错误
/// 仓储失败时返回错误。
async fn load_recent_history(
    db: &Database,
    instance_id: &ApprovalProcessInstanceId,
) -> Result<RuntimeHistoryPage> {
    let limit = RECENT_HISTORY_LIMIT as u32;
    let rows = db
        .bpm_workflow()
        .list_execution_history(instance_id, None, limit.saturating_add(1), &mut NoTransaction)
        .await?;
    let items = rows.iter().map(history_item_from_execution).collect();
    Ok(history_page_from(items, limit))
}

/// 未启动审批时的空投影。
///
/// # 参数
/// 无。
///
/// # 返回
/// 实例与历史均为空。
///
/// # 错误
/// 无。
fn empty_runtime() -> LoadedRuntime {
    LoadedRuntime {
        instance: None,
        recent_history: Vec::new(),
        history_page: DocumentApprovalHistoryPageView {
            next_cursor: None,
            has_more: false,
        },
    }
}

/// 由实例与当前执行构造详情摘要。
///
/// # 参数
/// * `instance` - 最新审批实例
/// * `current` - 当前 `ACTIVE|BLOCKED` 执行
/// * `latest_rejection` - 最近驳回原因
///
/// # 返回
/// 返回审批摘要所需字段；缺失节点或审批人时省略，不得补默认称谓。
///
/// # 错误
/// 无。
fn instance_view(
    instance: &ApprovalProcessInstance,
    current: Option<&ApprovalNodeExecution>,
    latest_rejection: Option<String>,
) -> DocumentApprovalInstanceView {
    DocumentApprovalInstanceView {
        id: instance.base.id.clone(),
        status: instance.status.as_str().to_string(),
        current_round_no: instance.current_round_no,
        current_node: current.map(|item| item.node_key.clone()),
        current_node_name: current.map(|item| item.node_name.clone()),
        current_assignee: current.map(|item| item.assignee_participant_id.as_str().to_string()),
        current_assignee_name: current.and_then(|item| optional_text(&item.assignee_name_snapshot)),
        latest_rejection,
        process_version: Some(instance.definition_version),
        blocker_code: instance.blocker_code.map(|code| code.as_str().to_string()),
    }
}

/// 把运行历史项转为销售单详情 DTO。
///
/// # 参数
/// * `item` - 通用历史项
///
/// # 返回
/// 返回单据详情 `recent_history` 项。
///
/// # 错误
/// 无。
fn history_item_view(item: &RuntimeHistoryItem) -> DocumentApprovalHistoryItemView {
    DocumentApprovalHistoryItemView {
        execution_id: item.execution_id.clone(),
        round_no: item.round_no,
        execution_no: item.execution_no,
        node_key: item.node_key.clone(),
        node_name: item.node_name.clone(),
        result: item.result.clone(),
        assignee_name: item.assignee_name.clone(),
        decided_by: item.decided_by.clone(),
        decision_reason: item.decision_reason.clone(),
        decided_at: item.decided_at,
    }
}

/// 去掉空白显示名。
///
/// # 参数
/// * `value` - 原始文本
///
/// # 返回
/// 非空文本；空白返回 `None`。
///
/// # 错误
/// 无。
fn optional_text(value: &str) -> Option<String> {
    let text = value.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{history_item_view, instance_view, optional_text};
    use crate::approval::execution::RuntimeHistoryItem;
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use bpm::model::types::{ApprovalBlockerCode, ApprovalExecutionAssignmentSource};
    use bpm::model::{
        ApprovalNodeExecution, ApprovalProcessInstance, NewNodeExecution, NewProcessInstance, ParticipantId,
        ProcessKind, SubjectRef, Timestamp,
    };

    fn running_instance() -> ApprovalProcessInstance {
        ApprovalProcessInstance::start_running(NewProcessInstance {
            id: ApprovalProcessInstanceId::new("inst-1"),
            process_definition_id: ApprovalProcessDefinitionId::new("def-1"),
            definition_version: 2,
            process_kind: ProcessKind::SalesOrder,
            subject: SubjectRef::new("sales_order", "so-1").unwrap(),
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

    /// 实例摘要读取当前执行的节点名与审批人显示名。
    #[test]
    fn instance_view_uses_current_execution_snapshot() {
        let view = instance_view(
            &running_instance(),
            Some(&current_execution()),
            Some("价格过高".into()),
        );
        assert_eq!(view.id, "inst-1");
        assert_eq!(view.status, "RUNNING");
        assert_eq!(view.current_node.as_deref(), Some("procurement_confirm"));
        assert_eq!(view.current_node_name.as_deref(), Some("采购确认"));
        assert_eq!(view.current_assignee_name.as_deref(), Some("李思勇"));
        assert_eq!(view.latest_rejection.as_deref(), Some("价格过高"));
        assert_eq!(view.process_version, Some(2));
        assert!(view.blocker_code.is_none());
    }

    /// 受阻实例带出 blocker 代码；无当前执行时不得补节点。
    #[test]
    fn instance_view_omits_node_when_execution_missing() {
        let mut instance = running_instance();
        instance
            .enter_blocked(
                ApprovalBlockerCode::ApproverAccountInactive,
                Timestamp::from_unix_secs(12).unwrap(),
            )
            .unwrap();
        let view = instance_view(&instance, None, None);
        assert_eq!(view.blocker_code.as_deref(), Some("APPROVER_ACCOUNT_INACTIVE"));
        assert!(view.current_node.is_none());
        assert!(view.current_assignee_name.is_none());
    }

    /// 历史项字段完整复制，空白审批人不上屏。
    #[test]
    fn history_item_view_copies_runtime_fields() {
        let item = RuntimeHistoryItem {
            execution_id: "exec-1".into(),
            round_no: 1,
            execution_no: 1,
            node_key: "procurement_confirm".into(),
            node_name: "采购确认".into(),
            result: "REJECTED".into(),
            assignee_name: Some("李思勇".into()),
            decided_by: Some("u-li".into()),
            decision_reason: Some("价格过高".into()),
            decided_at: Some(20),
        };
        let view = history_item_view(&item);
        assert_eq!(view.node_name, "采购确认");
        assert_eq!(view.result, "REJECTED");
        assert_eq!(view.decision_reason.as_deref(), Some("价格过高"));
        assert!(optional_text("  ").is_none());
    }
}
