//! 销售与采购详情使用的审批实例及历史摘要；不包含办理令牌字段。

use application_core::query::normalized_text;
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};
use erp_workflow::service::approval::execution::RuntimeHistoryItem;
use serde::Serialize;

/// 运行实例只读摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalInstanceSummaryView {
    /// 实例主键。
    pub id: String,
    /// 实例状态。
    pub status: String,
    /// 当前轮次。
    pub current_round_no: u32,
    /// 当前节点键。
    pub current_node: Option<String>,
    /// 当前节点名称。
    pub current_node_name: Option<String>,
    /// 当前审批人。
    pub current_assignee: Option<String>,
    /// 当前审批人显示名。
    pub current_assignee_name: Option<String>,
    /// 最近驳回原因。
    pub latest_rejection: Option<String>,
    /// 绑定定义业务版本。
    pub process_version: Option<u32>,
    /// 受阻代码；非 BLOCKED 为空。
    pub blocker_code: Option<String>,
}

impl DocumentApprovalInstanceSummaryView {
    /// 从持久化实例和当前执行构造只读摘要，不推导缺失的节点与人员。
    ///
    /// # 参数
    /// * `instance` - 最新审批实例。
    /// * `current` - 当前执行；缺失时保留空值。
    /// * `latest_rejection` - 调用方按原历史范围取得的最近驳回原因。
    ///
    /// # 返回
    /// 返回保留轮次、定义版本、受阻代码及人员名称快照的摘要。
    ///
    /// # 错误
    /// 不返回错误；空白人员快照转换为空值。
    pub(crate) fn from_runtime(
        instance: &ApprovalProcessInstance,
        current: Option<&ApprovalNodeExecution>,
        latest_rejection: Option<String>,
    ) -> Self {
        Self::new(instance.base.id.clone(), instance.status.as_str().to_string())
            .with_current_round_no(instance.current_round_no)
            .with_current_node(current.map(|item| item.node_key.clone()))
            .with_current_node_name(current.map(|item| item.node_name.clone()))
            .with_current_assignee(current.map(|item| item.assignee_participant_id.as_str().to_string()))
            .with_current_assignee_name(
                current.and_then(|item| normalized_text(Some(&item.assignee_name_snapshot))),
            )
            .with_latest_rejection(latest_rejection)
            .with_process_version(Some(instance.definition_version))
            .with_blocker_code(instance.blocker_code.map(|code| code.as_str().to_string()))
    }

    /// 构造运行实例只读摘要。
    ///
    /// # 参数
    /// * `id` - 实例主键
    /// * `status` - 实例状态
    ///
    /// # 返回
    /// 返回首轮、可选字段全空的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn new(id: String, status: String) -> Self {
        Self {
            id,
            status,
            current_round_no: 1,
            current_node: None,
            current_node_name: None,
            current_assignee: None,
            current_assignee_name: None,
            latest_rejection: None,
            process_version: None,
            blocker_code: None,
        }
    }

    /// 设置当前轮次。
    ///
    /// # 参数
    /// * `round_no` - 当前轮次
    ///
    /// # 返回
    /// 返回更新后的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn with_current_round_no(mut self, round_no: u32) -> Self {
        self.current_round_no = round_no;
        self
    }

    /// 设置当前节点。
    ///
    /// # 参数
    /// * `node` - 当前节点键
    ///
    /// # 返回
    /// 返回更新后的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn with_current_node(mut self, node: Option<String>) -> Self {
        self.current_node = node;
        self
    }

    /// 设置当前节点名称。
    ///
    /// # 参数
    /// * `node_name` - 当前节点名称
    ///
    /// # 返回
    /// 返回更新后的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn with_current_node_name(mut self, node_name: Option<String>) -> Self {
        self.current_node_name = node_name;
        self
    }

    /// 设置当前审批人。
    ///
    /// # 参数
    /// * `assignee` - 当前审批人
    ///
    /// # 返回
    /// 返回更新后的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn with_current_assignee(mut self, assignee: Option<String>) -> Self {
        self.current_assignee = assignee;
        self
    }

    /// 设置当前审批人显示名。
    ///
    /// # 参数
    /// * `assignee_name` - 当前审批人显示名
    ///
    /// # 返回
    /// 返回更新后的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn with_current_assignee_name(mut self, assignee_name: Option<String>) -> Self {
        self.current_assignee_name = assignee_name;
        self
    }

    /// 设置最近驳回原因。
    ///
    /// # 参数
    /// * `reason` - 最近驳回原因
    ///
    /// # 返回
    /// 返回更新后的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn with_latest_rejection(mut self, reason: Option<String>) -> Self {
        self.latest_rejection = reason;
        self
    }

    /// 设置绑定定义业务版本。
    ///
    /// # 参数
    /// * `version` - 绑定定义业务版本
    ///
    /// # 返回
    /// 返回更新后的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn with_process_version(mut self, version: Option<u32>) -> Self {
        self.process_version = version;
        self
    }

    /// 设置受阻代码。
    ///
    /// # 参数
    /// * `code` - 受阻代码
    ///
    /// # 返回
    /// 返回更新后的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn with_blocker_code(mut self, code: Option<String>) -> Self {
        self.blocker_code = code;
        self
    }
}

/// 有界历史项。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalHistorySummaryItemView {
    /// 执行主键。
    pub execution_id: String,
    /// 轮次。
    pub round_no: u32,
    /// 实例内执行序号。
    pub execution_no: u32,
    /// 节点键。
    pub node_key: String,
    /// 节点名称。
    pub node_name: String,
    /// 结束结果。
    pub result: String,
    /// 审批人显示名。
    pub assignee_name: Option<String>,
    /// 决定人。
    pub decided_by: Option<String>,
    /// 决定原因。
    pub decision_reason: Option<String>,
    /// 决定时间（unix 秒）。
    pub decided_at: Option<i64>,
}

impl DocumentApprovalHistorySummaryItemView {
    /// 从工作流历史事实构造详情历史条目。
    ///
    /// # 参数
    /// * `item` - 已按原范围和顺序读取的历史事实。
    ///
    /// # 返回
    /// 返回完整保留执行身份、轮次、人员、原因和时间的历史条目。
    ///
    /// # 错误
    /// 不返回错误。
    pub(crate) fn from_runtime(item: &RuntimeHistoryItem) -> Self {
        Self::new(
            item.execution_id.clone(),
            item.node_key.clone(),
            item.node_name.clone(),
            item.result.clone(),
        )
        .with_round_no(item.round_no)
        .with_execution_no(item.execution_no)
        .with_assignee_name(item.assignee_name.clone())
        .with_decided_by(item.decided_by.clone())
        .with_decision_reason(item.decision_reason.clone())
        .with_decided_at(item.decided_at)
    }

    /// 构造有界历史项。
    ///
    /// # 参数
    /// * `execution_id` - 执行主键
    /// * `node_key` - 节点键
    /// * `node_name` - 节点名称
    /// * `result` - 结束结果
    ///
    /// # 返回
    /// 返回首轮、可选字段全空的历史项。
    ///
    /// # 错误
    /// 无。
    pub fn new(execution_id: String, node_key: String, node_name: String, result: String) -> Self {
        Self {
            execution_id,
            round_no: 1,
            execution_no: 1,
            node_key,
            node_name,
            result,
            assignee_name: None,
            decided_by: None,
            decision_reason: None,
            decided_at: None,
        }
    }

    /// 设置轮次。
    ///
    /// # 参数
    /// * `round_no` - 轮次
    ///
    /// # 返回
    /// 返回更新后的历史项。
    ///
    /// # 错误
    /// 无。
    pub fn with_round_no(mut self, round_no: u32) -> Self {
        self.round_no = round_no;
        self
    }

    /// 设置实例内执行序号。
    ///
    /// # 参数
    /// * `execution_no` - 实例内执行序号
    ///
    /// # 返回
    /// 返回更新后的历史项。
    ///
    /// # 错误
    /// 无。
    pub fn with_execution_no(mut self, execution_no: u32) -> Self {
        self.execution_no = execution_no;
        self
    }

    /// 设置审批人显示名。
    ///
    /// # 参数
    /// * `name` - 审批人显示名
    ///
    /// # 返回
    /// 返回更新后的历史项。
    ///
    /// # 错误
    /// 无。
    pub fn with_assignee_name(mut self, name: Option<String>) -> Self {
        self.assignee_name = name;
        self
    }

    /// 设置决定人。
    ///
    /// # 参数
    /// * `decided_by` - 决定人
    ///
    /// # 返回
    /// 返回更新后的历史项。
    ///
    /// # 错误
    /// 无。
    pub fn with_decided_by(mut self, decided_by: Option<String>) -> Self {
        self.decided_by = decided_by;
        self
    }

    /// 设置决定原因。
    ///
    /// # 参数
    /// * `reason` - 决定原因
    ///
    /// # 返回
    /// 返回更新后的历史项。
    ///
    /// # 错误
    /// 无。
    pub fn with_decision_reason(mut self, reason: Option<String>) -> Self {
        self.decision_reason = reason;
        self
    }

    /// 设置决定时间。
    ///
    /// # 参数
    /// * `decided_at` - 决定时间（unix 秒）
    ///
    /// # 返回
    /// 返回更新后的历史项。
    ///
    /// # 错误
    /// 无。
    pub fn with_decided_at(mut self, decided_at: Option<i64>) -> Self {
        self.decided_at = decided_at;
        self
    }
}

#[cfg(test)]
mod tests {
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use bpm::model::types::{ApprovalBlockerCode, ApprovalExecutionAssignmentSource};
    use bpm::model::{
        ApprovalNodeExecution, ApprovalProcessInstance, NewNodeExecution, NewProcessInstance, ParticipantId,
        ProcessKind, SubjectRef, Timestamp,
    };
    use erp_workflow::service::approval::execution::RuntimeHistoryItem;

    use super::{DocumentApprovalHistorySummaryItemView, DocumentApprovalInstanceSummaryView};

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
        let view = DocumentApprovalInstanceSummaryView::from_runtime(
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
        let view = DocumentApprovalInstanceSummaryView::from_runtime(&instance, None, None);
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
        let view = DocumentApprovalHistorySummaryItemView::from_runtime(&item);
        assert_eq!(view.node_name, "采购确认");
        assert_eq!(view.result, "REJECTED");
        assert_eq!(view.decision_reason.as_deref(), Some("价格过高"));
        let mut execution = current_execution();
        execution.assignee_name_snapshot = "  ".into();
        let view =
            DocumentApprovalInstanceSummaryView::from_runtime(&running_instance(), Some(&execution), None);
        assert!(view.current_assignee_name.is_none());
    }
}
