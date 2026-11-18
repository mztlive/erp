//! 已授权销售单的当前办理摘要；不授予关联单据读取或任务操作权限。
mod repository;

use std::collections::{HashMap, HashSet};

use erp_core::common::time::Instant;
use erp_identity::AccessControlExt;
use erp_identity::repository::organization::OrganizationRepository;
use erp_identity::repository::prelude::*;
use erp_sales::entity::sales_order::{CommercialStatus, SalesOrder};
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use persistence_core::Executor;
use repository::CurrentWorkRepository;
use serde::Serialize;

use super::SalesOrderReadService;
use crate::Result;

/// 当前办理的最小公开摘要；不包含内部业务备注、金额、任务权限或关联单据链接。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesOrderCurrentWork {
    /// 稳定展示键，不作为任务操作授权。
    pub id: String,
    /// 固定任务种类，供页面判定重复提示，不直接展示枚举。
    pub kind: WorkItemType,
    /// 仅供服务端去重，不向销售详情返回关联对象身份。
    #[serde(skip)]
    object_key: String,
    /// 同一事项、环节与责任人下去重后的待办数量。
    pub count: usize,
    /// 当前审批节点名称；仅在匹配当前活动审批时返回。
    pub current_node: Option<String>,
    /// 详情页内导航分区。
    pub section: &'static str,
    /// 当前环节中文名称。
    pub label: &'static str,
    /// 责任岗位中文名称。
    pub role_label: &'static str,
    /// 实际任务责任人；缺失时表示尚未分配。
    pub owner_user_id: Option<String>,
    /// 实际任务责任人姓名；无法解析时保留空值，不以历史操作人补齐。
    pub owner_name: Option<String>,
    /// 当前处理人的有效主属组织名称；不使用业务单所属公司替代部门。
    pub owner_org_name: Option<String>,
    /// 下一步办理事项。
    pub instruction: &'static str,
}

impl SalesOrderReadService {
    // 读取销售当前办理时，在销售详情授权内汇总开放责任，只返回该销售单及其直接来源对象的办理摘要。
    //
    // 调用参数：
    // `order` 为已授权销售单。
    //
    // 返回按环节与责任人合并的待办摘要；终结任务与关联单据明细不进入结果。
    //
    // 关联读取、责任读取或姓名查询失败时返回错误，不伪装成无人处理。
    pub(super) async fn current_work(
        &self,
        order: &SalesOrder,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrderCurrentWork>> {
        if order.commercial_status == CommercialStatus::Voided {
            return Ok(Vec::new());
        }

        let current = CurrentWorkRepository { db: &self.db }.tasks(order, executor).await?;
        let tasks = current.tasks;
        let ids = tasks.iter().filter_map(|task| task.owner_user_id.clone()).collect::<Vec<_>>();
        let names = self.db.accounts().names_by_ids(&ids, executor).await?;
        let org_names =
            OrganizationRepository::new(&self.db).primary_org_names(&ids, Instant::now(), executor).await?;

        let mut rows = tasks
            .iter()
            .filter_map(|task| SalesOrderCurrentWork::from_task(task, &names))
            .collect::<Vec<_>>();
        for row in &mut rows {
            row.owner_org_name = row.owner_user_id.as_ref().and_then(|id| org_names.get(id).cloned());
            row.current_node = current.approval_nodes.get(&row.id).cloned();
        }

        rows.sort_by_key(|row| match row.section {
            "approval" => 0,
            "fulfillment" => 1,
            "acceptance" => 2,
            "invoice" => 3,
            _ => 4,
        });
        Ok(SalesOrderCurrentWork::summarize(rows))
    }
}

impl SalesOrderCurrentWork {
    // 依据事项、当前节点与责任人合并，业务身份只在内存中用于去重。
    fn summarize(rows: Vec<Self>) -> Vec<Self> {
        let mut positions = HashMap::new();
        let mut summaries: Vec<(Self, HashSet<String>)> = Vec::new();
        for mut row in rows {
            let key =
                format!("{:?}", (row.kind, row.label, row.role_label, &row.owner_user_id, &row.current_node));
            let object = std::mem::take(&mut row.object_key);
            if let Some(&index) = positions.get(&key) {
                let (summary, objects): &mut (Self, HashSet<String>) = &mut summaries[index];
                objects.insert(object);
                summary.count = objects.len();
            } else {
                row.id = key.clone();
                row.count = 1;
                positions.insert(key, summaries.len());
                summaries.push((row, HashSet::from([object])));
            }
        }
        summaries.into_iter().map(|(row, _)| row).collect()
    }

    // 从开放任务投影有限展示信息，不使用自由文本摘要或历史责任人。
    fn from_task(task: &WorkItem, names: &HashMap<String, String>) -> Option<Self> {
        if task.status != WorkItemStatus::Open {
            return None;
        }
        let (section, label, role_label, instruction) = task_copy(task)?;
        Some(Self {
            id: task.base.id.clone(),
            kind: task.work_item_type,
            object_key: format!("{}:{}", task.business_object_type, task.business_object_id),
            count: 1,
            current_node: None,
            section,
            label,
            role_label,
            instruction,
            owner_user_id: task.owner_user_id.clone(),
            owner_name: task.owner_user_id.as_ref().and_then(|id| names.get(id).cloned()),
            owner_org_name: None,
        })
    }
}

// 处理销售当前办理时，固定任务类型与对象对应的中文环节、岗位及动作；未知关系不作推断。
fn task_copy(task: &WorkItem) -> Option<(&'static str, &'static str, &'static str, &'static str)> {
    use WorkItemType::*;
    match (task.work_item_type, task.business_object_type.as_str()) {
        (ProcurementOrderCreation, "sales_order") => {
            Some(("fulfillment", "待安排采购 / 库存出库", "采购", "按负责商品安排采购或库存出库。"))
        },
        (PurchaseSourceUpdate, "purchase_order") => {
            Some(("fulfillment", "待更新采购依据", "采购", "按销售变更调整采购内容并提交。"))
        },
        (PurchaseOrderReview | DocumentApproval, "purchase_order") => {
            Some(("fulfillment", "采购审批中", "审批", "完成采购单当前节点审批。"))
        },
        (ApprovalRework, "purchase_order") => {
            Some(("fulfillment", "采购待修改重提", "采购", "按审批意见修改采购单并重新提交。"))
        },
        (CustomerAcceptanceRegistration, "sales_order") => {
            Some(("acceptance", "待客户验收", "销售", "核对客户实际收货结果并登记验收。"))
        },
        (SalesInvoiceExecution, "receivable_account" | "sales_invoice_request") => {
            Some(("invoice", "待开票", "财务", "按开票申请执行开票。"))
        },
        (DocumentApproval, "sales_invoice_request") => {
            Some(("invoice", "开票审批中", "审批", "完成开票申请当前节点审批。"))
        },
        (ApprovalRework, "sales_invoice_request") => {
            Some(("invoice", "开票申请待修改", "申请人", "按审批意见修改开票申请并重新提交。"))
        },
        (DocumentApproval, "sales_order" | "voucher_sales_order") => {
            Some(("approval", "销售审批中", "审批", "完成销售单当前节点审批。"))
        },
        _ => fulfillment_copy(task),
    }
}

// 处理销售当前办理时，履约岗位以任务冻结角色为准，未知角色不替换为采购。
fn fulfillment_copy(task: &WorkItem) -> Option<(&'static str, &'static str, &'static str, &'static str)> {
    use WorkItemType::FulfillmentOperation;
    match (task.work_item_type, task.business_object_type.as_str()) {
        (FulfillmentOperation, "purchase_receipt") => {
            Some(("fulfillment", "待采购入库", "仓储", "核对到货并登记采购入库。"))
        },
        (FulfillmentOperation, "delivery") => Some((
            "fulfillment",
            "待发货",
            match task.owner_role.as_str() {
                "warehouse_outbound_handler" => "仓储",
                "purchase_order_owner" => "采购",
                _ => "交付",
            },
            "完成发货并登记物流信息。",
        )),
        (FulfillmentOperation, "electronic_delivery") => {
            Some(("fulfillment", "待电子交付", "采购", "完成电子交付并确认交付结果。"))
        },
        (FulfillmentOperation, "service_fulfillment") => {
            Some(("fulfillment", "待服务交付", "采购", "完成服务并登记履约结果。"))
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::WorkItemId;
    use erp_workflow::entity::work_item::{AssignmentSource, WorkItemData, WorkItemPriority};

    use super::*;

    fn task(owner: &str, scope: &str) -> WorkItem {
        WorkItem::new_with_responsibility_scope(
            WorkItemId::new(format!("task-{owner}")),
            WorkItemData {
                work_item_type: WorkItemType::ProcurementOrderCreation,
                business_object_type: "sales_order".into(),
                business_object_id: "sale".into(),
                subject_version: "revision-1".into(),
                owner_role: "procurement".into(),
                owner_organization_id: "company".into(),
                owner_user_id: owner.into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: Some("不得展示内部备注".into()),
            },
            owner,
            vec![scope.into()],
        )
        .unwrap()
    }

    #[test]
    fn summary_merges_repeated_tasks_but_counts_distinct_objects() {
        let mut first = task("reviewer", "line");
        first.work_item_type = WorkItemType::DocumentApproval;
        first.business_object_type = "purchase_order".into();
        first.business_object_id = "private-purchase-a".into();
        let mut second = first.clone();
        second.business_object_id = "private-purchase-b".into();
        let names = HashMap::from([("reviewer".into(), "刘志强".into())]);
        let rows = [&first, &first, &second]
            .into_iter()
            .map(|task| SalesOrderCurrentWork::from_task(task, &names).unwrap())
            .collect();

        let summary = SalesOrderCurrentWork::summarize(rows);
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].count, 2);
        assert_eq!(summary[0].owner_name.as_deref(), Some("刘志强"));

        let wire = serde_json::to_value(&summary[0]).unwrap();
        let keys = wire.as_object().unwrap().keys().map(String::as_str).collect::<HashSet<_>>();
        assert_eq!(
            keys,
            HashSet::from([
                "id",
                "kind",
                "count",
                "current_node",
                "section",
                "label",
                "role_label",
                "owner_user_id",
                "owner_name",
                "owner_org_name",
                "instruction"
            ])
        );
        assert!(!wire.to_string().contains("private-purchase"));
        assert!(!wire.to_string().contains(&first.base.id));
    }

    #[test]
    fn summary_keeps_different_owners_and_current_nodes_separate() {
        let names = HashMap::new();
        let first = SalesOrderCurrentWork::from_task(&task("a", "line"), &names).unwrap();
        let second = SalesOrderCurrentWork::from_task(&task("b", "line"), &names).unwrap();
        let mut third = first.clone();
        third.current_node = Some("财务复核".into());
        let summary = SalesOrderCurrentWork::summarize(vec![first, second, third]);
        assert_eq!(summary.len(), 3);
        assert!(summary.iter().all(|row| row.count == 1));
        assert_eq!(summary[2].current_node.as_deref(), Some("财务复核"));
        assert!(SalesOrderCurrentWork::summarize(Vec::new()).is_empty());
    }

    #[test]
    fn completed_and_closed_tasks_never_become_current() {
        let mut item = task("buyer", "line");
        for status in [WorkItemStatus::Completed, WorkItemStatus::Closed] {
            item.status = status;
            assert!(SalesOrderCurrentWork::from_task(&item, &HashMap::new()).is_none());
        }
    }

    #[test]
    fn missing_name_and_unassigned_task_do_not_fall_back_to_historical_actor() {
        let mut item = task("buyer", "removed-line");
        item.completed_by = Some("old-reviewer".into());
        let names = HashMap::from([("old-reviewer".into(), "历史审批人".into())]);
        let missing = SalesOrderCurrentWork::from_task(&item, &names).unwrap();
        assert_eq!(missing.owner_user_id.as_deref(), Some("buyer"));
        assert!(missing.owner_name.is_none());

        item.owner_user_id = None;
        let unassigned = SalesOrderCurrentWork::from_task(&item, &names).unwrap();
        assert!(unassigned.owner_user_id.is_none());
        assert!(unassigned.owner_name.is_none());
    }

    #[test]
    fn delivery_acceptance_and_invoice_keep_independent_responsibility() {
        let mut item = task("handler", "line");
        let cases = [
            (
                WorkItemType::FulfillmentOperation,
                "delivery",
                "warehouse_outbound_handler",
                "fulfillment",
                "仓储",
            ),
            (WorkItemType::FulfillmentOperation, "delivery", "purchase_order_owner", "fulfillment", "采购"),
            (WorkItemType::CustomerAcceptanceRegistration, "sales_order", "sales", "acceptance", "销售"),
            (WorkItemType::SalesInvoiceExecution, "sales_invoice_request", "finance", "invoice", "财务"),
        ];
        for (kind, object, role, section, label) in cases {
            item.work_item_type = kind;
            item.business_object_type = object.into();
            item.owner_role = role.into();
            let row = SalesOrderCurrentWork::from_task(&item, &HashMap::new()).unwrap();
            assert_eq!(row.section, section);
            assert_eq!(row.role_label, label);
        }
        item.business_object_type = "unrelated".into();
        assert!(SalesOrderCurrentWork::from_task(&item, &HashMap::new()).is_none());
    }
}
