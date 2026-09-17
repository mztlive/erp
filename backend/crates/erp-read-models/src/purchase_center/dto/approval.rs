//! 单据统一只读审批展示视图。

use super::*;

/// 单据详情返回的统一只读审批结构。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalView {
    /// `PROCESS_REQUIRED` 或 `NO_APPROVAL`。
    pub requirement: String,
    /// 创建时冻结的定义摘要；未绑定为空。
    pub definition: Option<DocumentApprovalDefinitionView>,
    /// 已启动后的实例摘要；未提交为空。
    pub instance: Option<DocumentApprovalInstanceView>,
    /// 有界最近历史。
    pub recent_history: Vec<DocumentApprovalHistoryItemView>,
    /// 完整历史分页游标。
    pub history_page: DocumentApprovalHistoryPageView,
    /// 服务端允许的动作；不含选择定义或审批人。
    pub allowed_actions: Vec<String>,
}

/// 绑定定义只读摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalDefinitionView {
    /// 定义主键。
    pub id: String,
    /// 定义名称。
    pub name: String,
    /// 定义业务版本。
    pub version: u32,
    /// 节点摘要。单据详情不展开审批人。
    pub nodes: Vec<DocumentApprovalNodeView>,
}

impl DocumentApprovalDefinitionView {
    /// 构造绑定定义只读摘要。
    ///
    /// # 参数
    /// * `id` - 定义主键
    /// * `name` - 定义名称
    ///
    /// # 返回
    /// 返回版本为零、节点为空的摘要；调用方按需追加版本与节点。
    ///
    /// # 错误
    /// 无。
    pub fn new(id: String, name: String) -> Self {
        Self { id, name, version: 0, nodes: Vec::new() }
    }

    /// 设置定义业务版本。
    ///
    /// # 参数
    /// * `version` - 定义业务版本
    ///
    /// # 返回
    /// 返回更新后的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn with_version(mut self, version: u32) -> Self {
        self.version = version;
        self
    }

    /// 设置节点摘要。
    ///
    /// # 参数
    /// * `nodes` - 节点摘要
    ///
    /// # 返回
    /// 返回更新后的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn with_nodes(mut self, nodes: Vec<DocumentApprovalNodeView>) -> Self {
        self.nodes = nodes;
        self
    }
}

/// 定义节点只读摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalNodeView {
    /// 节点键。
    pub key: String,
    /// 节点名称。
    pub name: String,
}

/// 运行实例只读摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalInstanceView {
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

impl DocumentApprovalInstanceView {
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
pub struct DocumentApprovalHistoryItemView {
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

impl DocumentApprovalHistoryItemView {
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

/// 完整历史分页。
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalHistoryPageView {
    /// 下一页游标。
    pub next_cursor: Option<String>,
    /// 是否还有更多。
    pub has_more: bool,
}
