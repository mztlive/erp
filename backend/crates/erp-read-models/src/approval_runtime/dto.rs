//! 单据详情运行摘要；所有版本按十进制字符串传输。

use serde::Serialize;

/// 单据详情的真实审批实例与当前执行、任务版本。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalInstanceView {
    /// 审批实例主键。
    pub id: String,
    /// 实例状态。
    pub status: String,
    /// 当前轮次。
    pub current_round_no: u32,
    /// 当前节点键。
    pub current_node: Option<String>,
    /// 当前节点名称。
    pub current_node_name: Option<String>,
    /// 当前审批人标识。
    pub current_assignee: Option<String>,
    /// 当前审批人显示名。
    pub current_assignee_name: Option<String>,
    /// 最近驳回原因，不受详情历史页大小限制。
    pub latest_rejection: Option<String>,
    /// 最近驳回操作人。
    pub latest_rejection_by: Option<String>,
    /// 冻结提交版本。
    pub subject_version: Option<String>,
    /// 当前实例乐观锁版本。
    pub instance_version: Option<String>,
    /// 当前执行主键。
    pub current_execution_id: Option<String>,
    /// 当前执行乐观锁版本。
    pub current_execution_version: Option<String>,
    /// 当前开放任务主键；没有时为空。
    pub current_task_id: Option<String>,
    /// 当前开放任务版本；没有时为空。
    pub current_task_version: Option<String>,
    /// 冻结流程定义版本。
    pub process_version: Option<u32>,
    /// 受阻原因代码。
    pub blocker_code: Option<String>,
    /// 审批提交人。
    pub started_by: Option<String>,
}

impl DocumentApprovalInstanceView {
    /// 构造没有附加运行事实的摘要；实际加载后补齐权威版本。
    ///
    /// # 参数
    /// * `id` - 实例主键
    /// * `status` - 实例状态
    /// # 返回
    /// 返回首轮摘要，未取得的事实保持为空。
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
            latest_rejection_by: None,
            subject_version: None,
            instance_version: None,
            current_execution_id: None,
            current_execution_version: None,
            current_task_id: None,
            current_task_version: None,
            process_version: None,
            blocker_code: None,
            started_by: None,
        }
    }

    /// 复制实际当前轮次，不推断审批路线。
    ///
    /// # 参数
    /// * `round_no` - 实际当前轮次
    ///
    /// # 返回
    /// 返回写入 `current_round_no` 后的摘要。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn with_current_round_no(mut self, round_no: u32) -> Self {
        self.current_round_no = round_no;
        self
    }

    /// 复制实际当前节点。
    ///
    /// # 参数
    /// * `node` - 实际当前节点键；没有则为 `None`
    ///
    /// # 返回
    /// 返回写入 `current_node` 后的摘要，不改节点名称。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn with_current_node(mut self, node: Option<String>) -> Self {
        self.current_node = node;
        self
    }

    /// 复制实际当前审批人。
    ///
    /// # 参数
    /// * `assignee` - 实际当前审批人；没有则为 `None`
    ///
    /// # 返回
    /// 返回写入 `current_assignee` 后的摘要，不改显示名。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn with_current_assignee(mut self, assignee: Option<String>) -> Self {
        self.current_assignee = assignee;
        self
    }

    /// 复制最近驳回原因。
    ///
    /// # 参数
    /// * `reason` - 最近驳回原因；没有则为 `None`
    ///
    /// # 返回
    /// 返回写入 `latest_rejection` 后的摘要，不改驳回人。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn with_latest_rejection(mut self, reason: Option<String>) -> Self {
        self.latest_rejection = reason;
        self
    }
}

/// 实际执行历史，包括驳回决定和责任人快照。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalHistoryItemView {
    /// 节点执行主键。
    pub execution_id: String,
    /// 审批轮次。
    pub round_no: u32,
    /// 实例内执行序号。
    pub execution_no: u32,
    /// 节点键。
    pub node_key: String,
    /// 节点名称。
    pub node_name: String,
    /// 实际执行结果。
    pub result: String,
    /// 当时审批人显示名。
    pub assignee_name: Option<String>,
    /// 决定操作人。
    pub decided_by: Option<String>,
    /// 决定原因。
    pub decision_reason: Option<String>,
    /// 决定时间。
    pub decided_at: Option<i64>,
}

impl DocumentApprovalHistoryItemView {
    /// 沿既有详情构造入口保留历史必填身份，未读取的决定保持为空。
    ///
    /// # 参数
    /// * `execution_id` - 节点执行主键
    /// * `node_key` - 节点键
    /// * `result` - 实际执行结果
    ///
    /// # 返回
    /// 返回轮次与执行序号为 1、节点名为空、决定字段为 `None` 的历史项。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(execution_id: String, node_key: String, result: String) -> Self {
        Self {
            execution_id,
            round_no: 1,
            execution_no: 1,
            node_name: String::new(),
            node_key,
            result,
            assignee_name: None,
            decided_by: None,
            decision_reason: None,
            decided_at: None,
        }
    }

    /// 复制真实执行轮次。
    ///
    /// # 参数
    /// * `round_no` - 实际执行轮次
    ///
    /// # 返回
    /// 返回写入 `round_no` 后的历史项。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn with_round_no(mut self, round_no: u32) -> Self {
        self.round_no = round_no;
        self
    }
}

impl From<erp_workflow::service::approval::execution::RuntimeHistoryItem>
    for DocumentApprovalHistoryItemView
{
    /// 直接复制持久化历史投影字段，不转换 JSON 或推断决定人。
    fn from(item: erp_workflow::service::approval::execution::RuntimeHistoryItem) -> Self {
        Self {
            execution_id: item.execution_id,
            round_no: item.round_no,
            execution_no: item.execution_no,
            node_key: item.node_key,
            node_name: item.node_name,
            result: item.result,
            assignee_name: item.assignee_name,
            decided_by: item.decided_by,
            decision_reason: item.decision_reason,
            decided_at: item.decided_at,
        }
    }
}
