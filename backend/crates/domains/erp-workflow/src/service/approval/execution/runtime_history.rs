//! 审批执行历史投影：按轮次与执行序号输出有界历史项。
//!
//! HTTP 历史接口与单据详情 `recent_history` 共用同一映射，禁止再复用列表行形状。

use bpm::model::types::ApprovalNodeExecutionStatus;
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};
use serde::{Deserialize, Serialize};

use super::runtime_route::RuntimeApprovalRoute;
pub use crate::repository::bpm::ExecutionHistoryOrder;

/// 单条执行历史。字段与前端审批 Tab 合同对齐。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeHistoryItem {
    /// 执行主键。
    pub execution_id: String,
    /// 所属轮次。
    pub round_no: u32,
    /// 实例内执行序号。
    pub execution_no: u32,
    /// 节点键。
    pub node_key: String,
    /// 节点名称。
    pub node_name: String,
    /// 执行结果，取执行状态码。
    pub result: String,
    /// 审批人显示名快照。
    pub assignee_name: Option<String>,
    /// 决定人。
    pub decided_by: Option<String>,
    /// 决定原因。
    pub decision_reason: Option<String>,
    /// 决定时间（unix 秒）。
    pub decided_at: Option<i64>,
}

/// 构造执行历史项。
impl RuntimeHistoryItem {
    /// 以必填执行身份构造历史项；决定维度默认为空。
    ///
    /// # 参数
    /// * `execution_id` - 执行主键
    /// * `node_key` - 节点键
    /// * `node_name` - 节点名称
    /// * `result` - 执行结果
    ///
    /// # 返回
    /// 返回首轮的历史项。
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
}

/// 执行历史分页。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeHistoryPage {
    /// 发起人已展示的实例版本；仅包含最新进展的页面提供，供客户端显式确认已读。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_instance_version: Option<String>,
    /// 当前页，按请求指定的 `execution_no` 方向排序。
    pub items: Vec<RuntimeHistoryItem>,
    /// 下一页游标，值为本页最后一条 `execution_no`。
    pub next_cursor: Option<String>,
    /// 是否还有后续执行。
    pub has_more: bool,
    /// 销售审批首页携带当前轮次完整链条；其它类型及后续分页省略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<RuntimeApprovalRoute>,
}

impl RuntimeHistoryPage {
    /// 仅向发起人提供本页已展示的最新进展版本。
    /// # 参数
    /// 实例快照、登录账号、历史游标与排序方向。
    /// # 返回
    /// 无；旧记录页和非发起人不获得确认版本。
    /// # 错误
    /// 无。
    pub(crate) fn observe_progress(
        &mut self,
        instance: &ApprovalProcessInstance,
        actor_id: &str,
        after_execution_no: Option<u32>,
        order: ExecutionHistoryOrder,
    ) {
        let contains_latest = match order {
            ExecutionHistoryOrder::Desc => after_execution_no.is_none(),
            ExecutionHistoryOrder::Asc => !self.has_more,
        };
        self.observed_instance_version = (instance.started_by.as_str() == actor_id && contains_latest)
            .then(|| instance.base.version.to_string());
    }
}

/// 将节点执行映射为历史项。不得按 `node_key` 去重。
///
/// # 参数
/// * `execution` - 已持久化的节点执行
///
/// # 返回
/// 返回审批 Tab 可分组渲染的历史项。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// `result` 取执行状态，`ACTIVE` 表示当前办理中，不得丢弃未结束执行。
pub fn history_item_from_execution(execution: &ApprovalNodeExecution) -> RuntimeHistoryItem {
    RuntimeHistoryItem {
        execution_id: execution.base.id.clone(),
        round_no: execution.round_no,
        execution_no: execution.execution_no,
        node_key: execution.node_key.clone(),
        node_name: execution.node_name.clone(),
        result: execution.status.as_str().to_string(),
        assignee_name: optional_text(&execution.assignee_display_name()),
        decided_by: execution.decided_by.as_ref().map(|participant| participant.as_str().to_string()),
        decision_reason: execution.decision_reason.clone(),
        decided_at: execution.decided_at.map(|stamp| stamp.unix_secs()),
    }
}

/// 用多取一条的结果切出当前页并生成游标。
///
/// # 参数
/// * `items` - 已按请求方向排序、长度不超过 `limit + 1` 的映射结果
/// * `limit` - 合同页大小
///
/// # 返回
/// 返回截断后的当前页；超出 `limit` 时带上下一页游标。
///
/// # 错误
/// 无。
pub fn history_page_from(mut items: Vec<RuntimeHistoryItem>, limit: u32) -> RuntimeHistoryPage {
    let has_more = items.len() > limit as usize;
    if has_more {
        items.truncate(limit as usize);
    }
    let next_cursor = has_more.then(|| items.last().map(|item| item.execution_no.to_string())).flatten();
    RuntimeHistoryPage { items, next_cursor, has_more, route: None, observed_instance_version: None }
}

/// 从已排序历史中取最近一次驳回原因。
///
/// # 参数
/// * `items` - 按 `execution_no` 升序的历史项
///
/// # 返回
/// 最后一条 `REJECTED` 的决定原因；没有驳回，或该条没有决定原因时返回 `None`。
///
/// # 错误
/// 无。
pub fn latest_rejection_reason(items: &[RuntimeHistoryItem]) -> Option<String> {
    items
        .iter()
        .rev()
        .find(|item| item.result == ApprovalNodeExecutionStatus::Rejected.as_str())
        .and_then(|item| item.decision_reason.clone())
}

/// 去掉空白显示名，空快照不得上屏。
///
/// # 参数
/// * `value` - 原始显示名
///
/// # 返回
/// 非空文本；空白返回 `None`。
///
/// # 错误
/// 无。
fn optional_text(value: &str) -> Option<String> {
    let text = value.trim();
    if text.is_empty() { None } else { Some(text.to_string()) }
}

#[cfg(test)]
mod tests {
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use bpm::model::types::ApprovalExecutionAssignmentSource;
    use bpm::model::{
        ApprovalNodeExecution, ApprovalProcessInstance, NewNodeExecution, NewProcessInstance, ParticipantId,
        ProcessKind, SubjectRef, Timestamp,
    };

    use super::{
        ExecutionHistoryOrder, RuntimeHistoryItem, history_item_from_execution, history_page_from,
        latest_rejection_reason,
    };

    #[test]
    fn read_confirmation_requires_latest_page_and_initiator() {
        let instance = ApprovalProcessInstance::start_running(NewProcessInstance {
            id: ApprovalProcessInstanceId::new("i"),
            process_definition_id: ApprovalProcessDefinitionId::new("d"),
            definition_version: 1,
            process_kind: ProcessKind::StockAdjustment,
            subject: SubjectRef::new("stock_adjustment", "a").unwrap(),
            subject_version: 1,
            started_by: ParticipantId::new("starter").unwrap(),
            at: Timestamp::from_unix_secs(1).unwrap(),
        })
        .unwrap();
        for (actor, order, cursor, has_more, expected) in [
            ("starter", ExecutionHistoryOrder::Desc, None, true, true),
            ("starter", ExecutionHistoryOrder::Desc, Some(3), false, false),
            ("starter", ExecutionHistoryOrder::Asc, None, true, false),
            ("starter", ExecutionHistoryOrder::Asc, Some(3), false, true),
            ("starter", ExecutionHistoryOrder::Asc, None, false, true),
            ("other", ExecutionHistoryOrder::Desc, None, false, false),
        ] {
            let mut page = history_page_from(vec![history_item(1, "ACTIVE", None)], 20);
            page.has_more = has_more;
            page.observe_progress(&instance, actor, cursor, order);
            assert_eq!(page.observed_instance_version, expected.then(|| instance.base.version.to_string()));
        }
    }

    fn active_execution() -> ApprovalNodeExecution {
        ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new("exec-1"),
            process_instance_id: ApprovalProcessInstanceId::new("inst-1"),
            node_key: "procurement_confirm".into(),
            node_name: "采购确认".into(),
            round_no: 1,
            execution_no: 1,
            assignment_source: ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: ParticipantId::new("u1").unwrap(),
            assignee_name_snapshot: "李思勇".into(),
            at: Timestamp::from_unix_secs(10).unwrap(),
        })
        .unwrap()
    }

    fn history_item(execution_no: u32, result: &str, reason: Option<&str>) -> RuntimeHistoryItem {
        RuntimeHistoryItem {
            execution_id: format!("exec-{execution_no}"),
            round_no: 1,
            execution_no,
            node_key: "n1".into(),
            node_name: "采购确认".into(),
            result: result.into(),
            assignee_name: Some("李思勇".into()),
            decided_by: None,
            decision_reason: reason.map(str::to_string),
            decided_at: None,
        }
    }

    /// 活动执行必须出现在历史中，结果为 `ACTIVE`。
    #[test]
    fn maps_active_execution_as_history_item() {
        let item = history_item_from_execution(&active_execution());
        assert_eq!(item.execution_id, "exec-1");
        assert_eq!(item.round_no, 1);
        assert_eq!(item.execution_no, 1);
        assert_eq!(item.node_name, "采购确认");
        assert_eq!(item.result, "ACTIVE");
        assert_eq!(item.assignee_name.as_deref(), Some("李思勇"));
    }

    /// 驳回执行写入结果、原因与决定人。
    #[test]
    fn maps_rejected_execution_decision_fields() {
        let mut execution = active_execution();
        execution
            .record_reject(
                ParticipantId::new("u1").unwrap(),
                "价格过高",
                Timestamp::from_unix_secs(20).unwrap(),
            )
            .unwrap();
        let item = history_item_from_execution(&execution);
        assert_eq!(item.result, "REJECTED");
        assert_eq!(item.decision_reason.as_deref(), Some("价格过高"));
        assert_eq!(item.decided_by.as_deref(), Some("u1"));
        assert_eq!(item.decided_at, Some(20));
    }

    /// 多取一条用于判断是否还有后续页。
    #[test]
    fn history_page_truncates_and_sets_cursor() {
        let items = vec![
            history_item(1, "REJECTED", Some("价格过高")),
            history_item(2, "ACTIVE", None),
            history_item(3, "ACTIVE", None),
        ];
        let page = history_page_from(items, 2);
        assert_eq!(page.items.len(), 2);
        assert!(page.has_more);
        assert_eq!(page.next_cursor.as_deref(), Some("2"));
        assert_eq!(latest_rejection_reason(&page.items).as_deref(), Some("价格过高"));
    }

    /// 恰好一页时不得伪造 has_more。
    #[test]
    fn history_page_without_overflow_has_no_cursor() {
        let page = history_page_from(vec![history_item(1, "ACTIVE", None)], 8);
        assert!(!page.has_more);
        assert!(page.next_cursor.is_none());
        assert!(latest_rejection_reason(&page.items).is_none());
    }

    #[test]
    fn descending_history_keeps_latest_rows_and_uses_oldest_loaded_cursor() {
        let rows = (1..=51).rev().map(|number| history_item(number, "APPROVED", None)).collect();
        let page = history_page_from(rows, 50);
        assert_eq!(page.items.first().unwrap().execution_no, 51);
        assert_eq!(page.items.last().unwrap().execution_no, 2);
        assert_eq!(page.next_cursor.as_deref(), Some("2"));
        assert!(page.has_more);
        let tail = history_page_from(vec![history_item(1, "REJECTED", Some("资料不全"))], 50);
        assert_eq!(tail.items[0].execution_no, 1);
        assert!(!tail.has_more);
        assert!(tail.next_cursor.is_none());
        let empty = history_page_from(Vec::new(), 50);
        assert!(empty.items.is_empty());
        assert!(!empty.has_more);
        assert!(empty.next_cursor.is_none());
    }
}
