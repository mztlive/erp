//! 销售变更详情与审批只读协议；字段保持原 HTTP 合同。

use erp_sales::entity::sales_review::SalesChangeType;
use serde::Serialize;

/// 销售变更单详情视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesChangeOrderDetailView {
    /// 实体主键。
    pub id: String,
    /// 原销售单。
    pub sales_order_id: String,
    /// 发起时当前版本。
    pub base_revision_id: String,
    /// 变更类型。
    pub change_type: SalesChangeType,
    /// 变更原因。
    pub reason: String,
    /// 变更状态。
    pub status: erp_sales::entity::sales_review::SalesChangeOrderStatus,
    /// 当前不可变目标提交。
    pub current_submission_id: Option<String>,
    /// 目标完整内容指纹。
    pub target_content_hash: Option<String>,
    /// 生效后生成的新销售版本。
    pub effective_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 统一只读审批结构。客户端不得据此选择定义或审批人。
    pub approval: DocumentApprovalView,
}
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
    /// 返回版本为零、节点为空的摘要。
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
    /// 当前审批人。
    pub current_assignee: Option<String>,
    /// 最近驳回原因。
    pub latest_rejection: Option<String>,
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
            current_assignee: None,
            latest_rejection: None,
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
}

/// 有界历史项。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalHistoryItemView {
    /// 执行主键。
    pub execution_id: String,
    /// 轮次。
    pub round_no: u32,
    /// 节点键。
    pub node_key: String,
    /// 结束结果。
    pub result: String,
}

impl DocumentApprovalHistoryItemView {
    /// 构造有界历史项。
    ///
    /// # 参数
    /// * `execution_id` - 执行主键
    /// * `node_key` - 节点键
    /// * `result` - 结束结果
    ///
    /// # 返回
    /// 返回首轮的历史项。
    ///
    /// # 错误
    /// 无。
    pub fn new(execution_id: String, node_key: String, result: String) -> Self {
        Self { execution_id, round_no: 1, node_key, result }
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
}

/// 完整历史分页。
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalHistoryPageView {
    /// 下一页游标。
    pub next_cursor: Option<String>,
    /// 是否还有更多。
    pub has_more: bool,
}

#[cfg(test)]
mod wire_tests {
    use super::*;

    #[test]
    fn history_page_default_is_closed_cursor() {
        let page = DocumentApprovalHistoryPageView::default();
        assert!(page.next_cursor.is_none());
        assert!(!page.has_more);
    }

    #[test]
    fn definition_view_builder_keeps_identity_and_version() {
        let view =
            DocumentApprovalDefinitionView::new("def-1".to_string(), "复核审批".to_string()).with_version(2);
        assert_eq!(view.id, "def-1");
        assert_eq!(view.name, "复核审批");
        assert_eq!(view.version, 2);
        assert!(view.nodes.is_empty());
    }

    #[test]
    fn instance_and_history_builders_preserve_mandatory_fields() {
        let instance = DocumentApprovalInstanceView::new("inst-1".to_string(), "RUNNING".to_string());
        assert_eq!(instance.id, "inst-1");
        assert_eq!(instance.status, "RUNNING");
        let item = DocumentApprovalHistoryItemView::new(
            "exec-1".to_string(),
            "node-1".to_string(),
            "APPROVED".to_string(),
        )
        .with_round_no(2);
        assert_eq!(item.execution_id, "exec-1");
        assert_eq!(item.round_no, 2);
    }
}
