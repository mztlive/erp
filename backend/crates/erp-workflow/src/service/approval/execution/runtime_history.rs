//! 审批执行历史投影：按轮次与执行序号输出有界历史项。
//!
//! HTTP 历史接口与单据详情 `recent_history` 共用同一映射，禁止再复用列表行形状。

use bpm::model::types::ApprovalNodeExecutionStatus;
use bpm::model::ApprovalNodeExecution;
use serde::{Deserialize, Serialize};

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

/// 执行历史分页。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeHistoryPage {
    /// 当前页，按 `execution_no` 升序。
    pub items: Vec<RuntimeHistoryItem>,
    /// 下一页游标，值为本页最后一条 `execution_no`。
    pub next_cursor: Option<String>,
    /// 是否还有后续执行。
    pub has_more: bool,
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
        assignee_name: optional_text(&execution.assignee_name_snapshot),
        decided_by: execution
            .decided_by
            .as_ref()
            .map(|participant| participant.as_str().to_string()),
        decision_reason: execution.decision_reason.clone(),
        decided_at: execution.decided_at.map(|stamp| stamp.unix_secs()),
    }
}

/// 用多取一条的结果切出当前页并生成游标。
///
/// # 参数
/// * `items` - 已按 `execution_no` 升序、长度不超过 `limit + 1` 的映射结果
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
    let next_cursor = has_more
        .then(|| items.last().map(|item| item.execution_no.to_string()))
        .flatten();
    RuntimeHistoryPage {
        items,
        next_cursor,
        has_more,
    }
}

/// 从已排序历史中取最近一次驳回原因。
///
/// # 参数
/// * `items` - 按 `execution_no` 升序的历史项
///
/// # 返回
/// 最后一条 `REJECTED` 的决定原因；没有驳回时返回 `None`。
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
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        history_item_from_execution, history_page_from, latest_rejection_reason, RuntimeHistoryItem,
    };
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
    use bpm::model::types::ApprovalExecutionAssignmentSource;
    use bpm::model::{ApprovalNodeExecution, NewNodeExecution, ParticipantId, Timestamp};

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
}
