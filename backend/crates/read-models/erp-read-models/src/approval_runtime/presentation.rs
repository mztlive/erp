//! 单据详情共用的审批展示容器、定义摘要与历史分页合同。

use serde::Serialize;

use super::{DocumentApprovalHistoryItemView, DocumentApprovalInstanceView, DocumentApprovalNodeView};

/// 单据详情返回的统一只读审批结构。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalView<
    Instance = DocumentApprovalInstanceView,
    HistoryItem = DocumentApprovalHistoryItemView,
> {
    /// `PROCESS_REQUIRED` 或 `NO_APPROVAL`。
    pub requirement: String,
    /// 创建时冻结的定义摘要；未绑定为空。
    pub definition: Option<DocumentApprovalDefinitionView>,
    /// 已启动后的实例摘要；未提交为空。
    pub instance: Option<Instance>,
    /// 有界最近历史。
    pub recent_history: Vec<HistoryItem>,
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
    use serde_json::{Value, json, to_value};

    use super::*;
    use crate::finance::dto as finance;
    use crate::purchase_center::dto as purchase;
    use crate::returns_center::dto as returns;
    use crate::sales_center::order::dto as sales;
    use crate::sales_center::review;

    fn definition() -> DocumentApprovalDefinitionView {
        DocumentApprovalDefinitionView::new("def-1".into(), "单据审批".into()).with_version(3).with_nodes(
            vec![DocumentApprovalNodeView {
                key: "node-1".into(),
                name: "财务审核".into(),
                assignee_source: Some("finance".into()),
                assignee_name: Some("王慧敏".into()),
            }],
        )
    }

    fn approval<Instance, HistoryItem>(
        instance: Instance,
        item: HistoryItem,
    ) -> DocumentApprovalView<Instance, HistoryItem> {
        DocumentApprovalView {
            requirement: "PROCESS_REQUIRED".into(),
            definition: Some(definition()),
            instance: Some(instance),
            recent_history: vec![item],
            history_page: DocumentApprovalHistoryPageView {
                next_cursor: Some("cursor-2".into()),
                has_more: true,
            },
            allowed_actions: vec!["cancel".into()],
        }
    }

    fn expected_approval(instance: Value, history: Value) -> Value {
        json!({
            "requirement": "PROCESS_REQUIRED",
            "definition": {
                "id": "def-1", "name": "单据审批", "version": 3,
                "nodes": [{
                    "key": "node-1", "name": "财务审核",
                    "assignee_source": "finance", "assignee_name": "王慧敏"
                }]
            },
            "instance": instance,
            "recent_history": [history],
            "history_page": { "next_cursor": "cursor-2", "has_more": true },
            "allowed_actions": ["cancel"]
        })
    }

    #[test]
    fn all_five_detail_contracts_keep_unbound_approval_fields() {
        macro_rules! unbound {
            ($view:ty) => {{
                let view: $view = DocumentApprovalView {
                    requirement: "NO_APPROVAL".into(),
                    definition: None,
                    instance: None,
                    recent_history: Vec::new(),
                    history_page: DocumentApprovalHistoryPageView::default(),
                    allowed_actions: Vec::new(),
                };
                to_value(view).unwrap()
            }};
        }
        let expected = json!({
            "requirement": "NO_APPROVAL", "definition": null, "instance": null,
            "recent_history": [], "history_page": { "next_cursor": null, "has_more": false },
            "allowed_actions": []
        });
        for actual in [
            unbound!(finance::DocumentApprovalView),
            unbound!(review::DocumentApprovalView),
            unbound!(sales::DocumentApprovalView),
            unbound!(purchase::DocumentApprovalView),
            unbound!(returns::DocumentApprovalView),
        ] {
            assert_eq!(actual, expected);
        }
        assert_eq!(
            to_value(DocumentApprovalDefinitionView::new("def-1".into(), "单据审批".into())).unwrap(),
            json!({ "id": "def-1", "name": "单据审批", "version": 0, "nodes": [] })
        );
    }

    #[test]
    fn finance_and_change_keep_full_runtime_fields() {
        let mut instance = finance::DocumentApprovalInstanceView::new("inst-1".into(), "RUNNING".into())
            .with_current_round_no(2)
            .with_current_node(Some("node-1".into()))
            .with_current_assignee(Some("user-1".into()))
            .with_latest_rejection(Some("补充资料".into()));
        instance.current_node_name = Some("财务审核".into());
        instance.current_assignee_name = Some("王慧敏".into());
        instance.latest_rejection_by = Some("reviewer-1".into());
        instance.latest_rejection_by_name = Some("审核人".into());
        instance.subject_version = Some("subject-v3".into());
        instance.instance_version = Some("instance-v4".into());
        instance.current_execution_id = Some("exec-1".into());
        instance.current_execution_version = Some("execution-v5".into());
        instance.current_task_id = Some("task-1".into());
        instance.current_task_version = Some("task-v6".into());
        instance.process_version = Some(3);
        instance.blocker_code = Some("WAITING".into());
        instance.started_by = Some("sales-1".into());
        let mut item = finance::DocumentApprovalHistoryItemView::new(
            "exec-0".into(),
            "node-0".into(),
            "REJECTED".into(),
        )
        .with_round_no(1);
        item.execution_no = 2;
        item.node_name = "销售审核".into();
        item.assignee_name = Some("审核人".into());
        item.decided_by = Some("reviewer-1".into());
        item.decision_reason = Some("补充资料".into());
        item.decided_at = Some(1_800_000_000);
        let finance: finance::DocumentApprovalView = approval(instance, item);
        let change: review::DocumentApprovalView = finance.clone();
        let expected = expected_approval(
            json!({
                "id": "inst-1", "status": "RUNNING", "current_round_no": 2,
                "current_node": "node-1", "current_node_name": "财务审核",
                "current_assignee": "user-1", "current_assignee_name": "王慧敏",
                "latest_rejection": "补充资料", "latest_rejection_by": "reviewer-1",
                "latest_rejection_by_name": "审核人", "subject_version": "subject-v3",
                "instance_version": "instance-v4", "current_execution_id": "exec-1",
                "current_execution_version": "execution-v5", "current_task_id": "task-1",
                "current_task_version": "task-v6", "process_version": 3,
                "blocker_code": "WAITING", "started_by": "sales-1"
            }),
            json!({
                "execution_id": "exec-0", "round_no": 1, "execution_no": 2,
                "node_key": "node-0", "node_name": "销售审核", "result": "REJECTED",
                "assignee_name": "审核人", "decided_by": "reviewer-1",
                "decision_reason": "补充资料", "decided_at": 1_800_000_000
            }),
        );
        assert_eq!(to_value(finance).unwrap(), expected);
        assert_eq!(to_value(change).unwrap(), expected);
    }

    #[test]
    fn sales_and_purchase_keep_summary_fields_without_runtime_tokens() {
        let instance = sales::DocumentApprovalInstanceView::new("inst-1".into(), "RUNNING".into())
            .with_current_round_no(2)
            .with_current_node(Some("node-1".into()))
            .with_current_node_name(Some("财务审核".into()))
            .with_current_assignee(Some("user-1".into()))
            .with_current_assignee_name(Some("王慧敏".into()))
            .with_latest_rejection(Some("补充资料".into()))
            .with_process_version(Some(3))
            .with_blocker_code(Some("WAITING".into()));
        let item = sales::DocumentApprovalHistoryItemView::new(
            "exec-0".into(),
            "node-0".into(),
            "销售审核".into(),
            "REJECTED".into(),
        )
        .with_round_no(1)
        .with_execution_no(2)
        .with_assignee_name(Some("审核人".into()))
        .with_decided_by(Some("reviewer-1".into()))
        .with_decision_reason(Some("补充资料".into()))
        .with_decided_at(Some(1_800_000_000));
        let sales: sales::DocumentApprovalView = approval(instance, item);
        let purchase: purchase::DocumentApprovalView = sales.clone();
        let expected = expected_approval(
            json!({
                "id": "inst-1", "status": "RUNNING", "current_round_no": 2,
                "current_node": "node-1", "current_node_name": "财务审核",
                "current_assignee": "user-1", "current_assignee_name": "王慧敏",
                "latest_rejection": "补充资料", "process_version": 3, "blocker_code": "WAITING"
            }),
            json!({
                "execution_id": "exec-0", "round_no": 1, "execution_no": 2,
                "node_key": "node-0", "node_name": "销售审核", "result": "REJECTED",
                "assignee_name": "审核人", "decided_by": "reviewer-1",
                "decision_reason": "补充资料", "decided_at": 1_800_000_000
            }),
        );
        assert_eq!(to_value(sales).unwrap(), expected);
        assert_eq!(to_value(purchase).unwrap(), expected);
    }

    #[test]
    fn returns_keep_reduced_instance_and_history_fields() {
        let instance = returns::DocumentApprovalInstanceView::new("inst-1".into(), "RUNNING".into())
            .with_current_round_no(2)
            .with_current_node(Some("node-1".into()))
            .with_current_node_name(Some("财务审核".into()))
            .with_current_assignee(Some("user-1".into()))
            .with_current_assignee_name(Some("王慧敏".into()))
            .with_latest_rejection(Some("补充资料".into()));
        let item = returns::DocumentApprovalHistoryItemView::new(
            "exec-0".into(),
            "node-0".into(),
            "REJECTED".into(),
        )
        .with_round_no(1);
        let returns: returns::DocumentApprovalView = approval(instance, item);
        assert_eq!(
            to_value(returns).unwrap(),
            expected_approval(
                json!({
                    "id": "inst-1", "status": "RUNNING", "current_round_no": 2,
                    "current_node": "node-1", "current_node_name": "财务审核",
                    "current_assignee_name": "王慧敏", "current_assignee": "user-1",
                    "latest_rejection": "补充资料"
                }),
                json!({
                    "execution_id": "exec-0", "round_no": 1, "node_key": "node-0", "result": "REJECTED"
                })
            )
        );
    }
}
